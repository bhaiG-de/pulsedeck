//! Next-track suggestions. Every analysed track, local or cached Spotify, is
//! scored against the track on the audible deck by key, tempo and energy, and
//! the best are offered for the other deck with a one-line reason built from
//! the scores. No language model is involved.
//!
//! Scoring reads a compact feature set per track ([`TrackFeatures`]) that the
//! library stores in `suggest_features` and keeps in memory, so a request never
//! parses the large analysis records. Older records get their features from a
//! background pass the first time suggestions are asked for.

use crate::library::TrackRecord;
use audio_analysis::WaveformBands;
use deck_engine::{EngineSnapshot, TrackAnalysis};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    fmt,
    sync::atomic::Ordering as AtomicOrdering,
    time::{Duration, Instant},
};
use tauri::Manager;

/// Bump when the stored features or the energy score change.
pub const FEATURES_VERSION: u32 = 1;
/// Suggestions returned per request.
pub const LIMIT: usize = 20;
/// Audible playback after which a track counts as played this session.
pub const PLAYED_AFTER_SECONDS: f64 = 30.0;
/// A deck is audible above this channel-fader times crossfader gain.
const AUDIBLE_GAIN: f32 = 0.05;
/// Energy differences smaller than this read as "same energy".
const SAME_ENERGY: f32 = 0.05;
/// Weights of the key, tempo and energy fits in the overall score.
const WEIGHTS: [f32; 3] = [0.45, 0.35, 0.2];
/// Records read per background feature batch.
const BACKFILL_BATCH: usize = 16;

/// The parts of a track's analysis that suggestions use.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackFeatures {
    /// Local file path or `spotify:track:` URI.
    pub source: String,
    /// Library id for local tracks.
    pub track_id: Option<i64>,
    pub title: String,
    pub artist: String,
    pub duration_seconds: f64,
    pub bpm: Option<f64>,
    /// The tempo may be half or double (the beat model and DSP disagreed by
    /// a factor of two). Never set for a manual BPM.
    pub bpm_ambiguous: bool,
    pub camelot: Option<String>,
    pub key_confidence: Option<f32>,
    /// 0 to 1, from the waveform bands; see [`energy_score`].
    pub energy: Option<f32>,
}

pub fn usable_bpm(bpm: Option<f64>) -> Option<f64> {
    bpm.filter(|b| b.is_finite() && (30.0..=300.0).contains(b))
}

impl TrackFeatures {
    pub fn from_record(record: &TrackRecord) -> Self {
        Self {
            source: record.path.clone(),
            track_id: Some(record.id),
            title: record.title.clone(),
            artist: record.artist.clone(),
            duration_seconds: record.duration_seconds,
            bpm: usable_bpm(Some(record.bpm)),
            bpm_ambiguous: record.analysis.bpm_ambiguous
                && record.analysis.bpm_source.as_deref() != Some("manual"),
            camelot: record.camelot.clone(),
            key_confidence: record.key_confidence,
            energy: record
                .analysis
                .waveform_bands
                .as_ref()
                .and_then(energy_score),
        }
    }

    pub fn from_analysis(analysis: &TrackAnalysis) -> Self {
        Self {
            source: analysis.path.clone(),
            track_id: None,
            title: analysis.title.clone(),
            artist: analysis.artist.clone().unwrap_or_default(),
            duration_seconds: analysis.duration_seconds,
            bpm: usable_bpm(analysis.bpm),
            bpm_ambiguous: analysis.bpm_ambiguous
                && analysis.bpm_source.as_deref() != Some("manual"),
            camelot: analysis.camelot.clone(),
            key_confidence: analysis.key_confidence,
            energy: analysis.waveform_bands.as_ref().and_then(energy_score),
        }
    }

    /// A stored placeholder for a source that cannot be suggested (it has no
    /// usable analysis), so the background pass does not retry it.
    pub fn unusable(source: String) -> Self {
        Self {
            source,
            track_id: None,
            title: String::new(),
            artist: String::new(),
            duration_seconds: 0.0,
            bpm: None,
            bpm_ambiguous: false,
            camelot: None,
            key_confidence: None,
            energy: None,
        }
    }
}

/// Raw stored records that still need features. Parsing them is the slow part,
/// so the background pass does it outside the library lock.
pub enum PendingFeatures {
    /// `features` already holds the library columns; the rest comes from
    /// `analysis_json`.
    Local {
        features: TrackFeatures,
        analysis_json: String,
    },
    Spotify {
        uri: String,
        metadata: String,
    },
}

/// The few `analysis_json` fields features need; serde skips the rest.
#[derive(Default, Deserialize)]
#[serde(default)]
struct FeatureFields {
    bpm_ambiguous: bool,
    bpm_source: Option<String>,
    waveform_bands: Option<WaveformBands>,
}

impl PendingFeatures {
    pub fn compute(self) -> TrackFeatures {
        match self {
            Self::Local {
                mut features,
                analysis_json,
            } => {
                let fields: FeatureFields =
                    serde_json::from_str(&analysis_json).unwrap_or_default();
                features.bpm_ambiguous =
                    fields.bpm_ambiguous && fields.bpm_source.as_deref() != Some("manual");
                features.energy = fields.waveform_bands.as_ref().and_then(energy_score);
                features
            }
            Self::Spotify { uri, metadata } => serde_json::from_str::<TrackAnalysis>(&metadata)
                .ok()
                .filter(|a| crate::library::usable_spotify_analysis(&uri, a))
                .map(|a| TrackFeatures::from_analysis(&a))
                .unwrap_or_else(|| TrackFeatures::unusable(uri)),
        }
    }
}

/// Energy from 0 (calm) to 1 (intense), comparable across tracks: 65% the
/// loudness of the louder half of the track (its main sections, so long intros
/// and breakdowns do not pull it down) and 35% its brightness (the share of
/// hi-hats, claps and cymbals). The bands are RMS on one shared full-scale
/// reference, so loudness is absolute. A heuristic, not a trained model.
pub fn energy_score(bands: &WaveformBands) -> Option<f32> {
    let n = bands.low.len().min(bands.mid.len()).min(bands.high.len());
    if n < 16 {
        return None;
    }
    let level: Vec<f32> = (0..n)
        .map(|i| {
            let (l, m, h) = (bands.low[i], bands.mid[i], bands.high[i]);
            if l.is_finite() && m.is_finite() && h.is_finite() {
                (l * l + m * m + h * h).sqrt()
            } else {
                0.0
            }
        })
        .collect();
    let mut sorted = level.clone();
    let (_, median, _) = sorted.select_nth_unstable_by(n / 2, f32::total_cmp);
    let median = *median;
    let (mut sum, mut high, mut count) = (0.0_f64, 0.0_f64, 0_usize);
    for (i, &value) in level.iter().enumerate() {
        if value >= median && value > 0.0 {
            sum += value as f64;
            high += bands.high[i] as f64;
            count += 1;
        }
    }
    if count == 0 || sum <= 1e-6 {
        return None;
    }
    let loudness_db = 20.0 * (sum / count as f64).log10();
    let loudness = ((loudness_db + 16.0) / 11.0).clamp(0.0, 1.0);
    let brightness = ((high / sum - 0.08) / 0.22).clamp(0.0, 1.0);
    Some((0.65 * loudness + 0.35 * brightness) as f32)
}

/// Energy as the familiar 1 to 10 level.
pub fn energy_level(energy: f32) -> u8 {
    (1.0 + (energy.clamp(0.0, 1.0) * 9.0).round()) as u8
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergyDirection {
    #[default]
    Keep,
    Raise,
    Lower,
}

/// A Camelot wheel position: 1 to 12, A (minor) or B (major).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Camelot {
    pub number: u8,
    pub major: bool,
}

impl Camelot {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let major = match text.chars().last()?.to_ascii_uppercase() {
            'A' => false,
            'B' => true,
            _ => return None,
        };
        // The letter is ASCII, so this slice ends on a character boundary.
        let number: u8 = text[..text.len() - 1].trim().parse().ok()?;
        (1..=12).contains(&number).then_some(Self { number, major })
    }
}

impl fmt::Display for Camelot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.number, if self.major { 'B' } else { 'A' })
    }
}

/// How the candidate's key sits against the playing key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyRelation {
    Same,
    /// One step clockwise (8A to 9A).
    StepUp,
    /// One step anticlockwise (8A to 7A).
    StepDown,
    /// The A/B switch between relative minor and major (8A to 8B).
    Relative,
    Clash,
    /// Either key is missing.
    Unknown,
}

impl KeyRelation {
    pub fn between(playing: Option<Camelot>, next: Option<Camelot>) -> Self {
        let (Some(a), Some(b)) = (playing, next) else {
            return Self::Unknown;
        };
        let steps = (b.number + 12 - a.number) % 12;
        match (a.major == b.major, steps) {
            (true, 0) => Self::Same,
            (true, 1) => Self::StepUp,
            (true, 11) => Self::StepDown,
            (false, 0) => Self::Relative,
            _ => Self::Clash,
        }
    }

    fn fit(self) -> Option<f32> {
        match self {
            Self::Same => Some(1.0),
            Self::StepUp | Self::StepDown => Some(0.9),
            Self::Relative => Some(0.85),
            Self::Clash => Some(0.0),
            Self::Unknown => None,
        }
    }
}

/// Key fit weighted by confidence: estimates under 0.6 count proportionally
/// less, so an uncertain key neither rewards nor punishes much. Unknown keys
/// score the neutral 0.5.
pub fn key_score(relation: KeyRelation, playing: Option<f32>, next: Option<f32>) -> f32 {
    let Some(fit) = relation.fit() else {
        return 0.5;
    };
    let confidence = playing.unwrap_or(0.5).min(next.unwrap_or(0.5));
    let trust = if confidence.is_finite() {
        (confidence / 0.6).clamp(0.0, 1.0)
    } else {
        0.0
    };
    0.5 + (fit - 0.5) * trust
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct TempoMatch {
    /// Tempo change the other deck needs to match the playing deck.
    pub percent: f64,
    /// 1, or 2 when the candidate is counted at double time (a half-time
    /// track), or 0.5 when it is counted at half time.
    pub multiple: f64,
    pub score: f32,
}

/// The best beat match of `next_bpm` against the heard `playing_bpm` within
/// ±`range` percent: as is, or counted at half or double time. Half/double
/// costs little when either tempo is flagged ambiguous and more otherwise.
pub fn tempo_match(
    playing_bpm: f64,
    next_bpm: f64,
    ambiguous: bool,
    range: f64,
) -> Option<TempoMatch> {
    let mut best: Option<TempoMatch> = None;
    for multiple in [1.0, 2.0, 0.5] {
        let percent = (playing_bpm / (next_bpm * multiple) - 1.0) * 100.0;
        if !percent.is_finite() || percent.abs() > range + 1e-9 {
            continue;
        }
        let penalty = if multiple == 1.0 {
            1.0
        } else if ambiguous {
            0.95
        } else {
            0.7
        };
        let score = ((-(percent / 5.0).powi(2)).exp() * penalty) as f32;
        if best.is_none_or(|b| score > b.score) {
            best = Some(TempoMatch {
                percent,
                multiple,
                score,
            });
        }
    }
    best
}

/// How well an energy change (`next - playing`) suits the chosen direction.
pub fn energy_fit(delta: f32, direction: EnergyDirection) -> f32 {
    match direction {
        EnergyDirection::Keep => (-(delta / 0.08).powi(2)).exp(),
        EnergyDirection::Raise => step_fit(delta),
        EnergyDirection::Lower => step_fit(-delta),
    }
}

/// Prefers a step of about one level the wanted way, accepts bigger steps and
/// scores the wrong direction low.
fn step_fit(delta: f32) -> f32 {
    const STEP: f32 = 0.1;
    let width = if delta < STEP { 0.06 } else { 0.15 };
    (-((delta - STEP) / width).powi(2)).exp()
}

/// The one-line reason, for example "8A→9A · +1.5% · energy up".
pub fn reason(
    playing: Option<Camelot>,
    next: Option<Camelot>,
    tempo: Option<&TempoMatch>,
    energy_delta: Option<f32>,
) -> String {
    let mut parts = Vec::with_capacity(3);
    parts.push(match (playing, next) {
        (Some(a), Some(b)) if KeyRelation::between(playing, next) == KeyRelation::Clash => {
            format!("{a}→{b} clash")
        }
        (Some(a), Some(b)) => format!("{a}→{b}"),
        _ => "key ?".into(),
    });
    if let Some(tempo) = tempo {
        let percent = (tempo.percent * 10.0).round() / 10.0;
        let mut text = if percent == 0.0 {
            "±0.0%".to_string()
        } else {
            format!("{percent:+.1}%")
        };
        if tempo.multiple > 1.5 {
            text.push_str(" half-time");
        } else if tempo.multiple < 0.75 {
            text.push_str(" double-time");
        }
        parts.push(text);
    }
    if let Some(delta) = energy_delta {
        parts.push(
            if delta >= SAME_ENERGY {
                "energy up"
            } else if delta <= -SAME_ENERGY {
                "energy down"
            } else {
                "same energy"
            }
            .into(),
        );
    }
    parts.join(" · ")
}

/// The track the suggestions follow, as heard.
#[derive(Clone, Debug, Serialize)]
pub struct Reference {
    pub source: String,
    pub title: String,
    /// BPM including the deck's tempo change.
    pub bpm: Option<f64>,
    pub bpm_ambiguous: bool,
    pub camelot: Option<String>,
    pub key_confidence: Option<f32>,
    pub energy: Option<f32>,
    pub energy_level: Option<u8>,
}

impl Reference {
    /// The loaded track as it plays: the deck's BPM (which may be a manual
    /// edit) times its tempo.
    pub fn from_deck(track: &TrackAnalysis, deck_bpm: Option<f64>, tempo_percent: f64) -> Self {
        let features = TrackFeatures::from_analysis(track);
        let bpm = usable_bpm(deck_bpm).or(features.bpm);
        let edited = match (bpm, features.bpm) {
            (Some(deck), Some(analysed)) => (deck - analysed).abs() > 0.01,
            _ => false,
        };
        let rate = 1.0 + tempo_percent / 100.0;
        Self {
            source: features.source,
            title: features.title,
            bpm: bpm.map(|b| b * rate).filter(|b| b.is_finite() && *b > 0.0),
            bpm_ambiguous: features.bpm_ambiguous && !edited,
            camelot: features.camelot,
            key_confidence: features.key_confidence,
            energy: features.energy,
            energy_level: features.energy.map(energy_level),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Suggestion {
    pub source: String,
    pub spotify: bool,
    pub track_id: Option<i64>,
    pub title: String,
    pub artist: String,
    pub duration_seconds: f64,
    pub bpm: f64,
    pub camelot: Option<String>,
    pub energy: Option<f32>,
    pub energy_level: Option<u8>,
    pub key: KeyRelation,
    pub key_clash: bool,
    pub tempo: Option<TempoMatch>,
    pub energy_delta: Option<f32>,
    pub score: f32,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Ranking {
    pub suggestions: Vec<Suggestion>,
    /// Tracks scored.
    pub considered: usize,
    /// Left out because they were played this session.
    pub played: usize,
    /// Left out because the other deck's tempo range cannot reach them.
    pub out_of_range: usize,
    /// Left out because they have no BPM.
    pub without_tempo: usize,
}

struct Scored<'a> {
    features: &'a TrackFeatures,
    score: f32,
    relation: KeyRelation,
    next_key: Option<Camelot>,
    tempo: Option<TempoMatch>,
    energy_delta: Option<f32>,
}

/// Higher score first, then the smaller tempo change, then source for a
/// stable order.
fn better(a: &Scored<'_>, b: &Scored<'_>) -> Ordering {
    let change = |s: &Scored<'_>| s.tempo.map_or(f64::INFINITY, |t| t.percent.abs());
    b.score
        .total_cmp(&a.score)
        .then_with(|| change(a).total_cmp(&change(b)))
        .then_with(|| a.features.source.cmp(&b.features.source))
}

/// Score `pool` against the playing track and return the best `limit`.
/// `excluded` are the tracks on the decks; `played` were heard this session.
pub fn rank<'a>(
    pool: impl IntoIterator<Item = &'a TrackFeatures>,
    playing: &Reference,
    direction: EnergyDirection,
    tempo_range: f64,
    excluded: &HashSet<String>,
    played: &HashSet<String>,
    limit: usize,
) -> Ranking {
    let mut ranking = Ranking::default();
    let playing_key = playing.camelot.as_deref().and_then(Camelot::parse);
    let mut scored = Vec::new();
    for features in pool {
        if excluded.contains(&features.source) {
            continue;
        }
        if played.contains(&features.source) {
            ranking.played += 1;
            continue;
        }
        let Some(bpm) = features.bpm else {
            ranking.without_tempo += 1;
            continue;
        };
        let tempo = match playing.bpm {
            Some(heard) => match tempo_match(
                heard,
                bpm,
                features.bpm_ambiguous || playing.bpm_ambiguous,
                tempo_range,
            ) {
                Some(tempo) => Some(tempo),
                None => {
                    ranking.out_of_range += 1;
                    continue;
                }
            },
            None => None,
        };
        ranking.considered += 1;
        let next_key = features.camelot.as_deref().and_then(Camelot::parse);
        let relation = KeyRelation::between(playing_key, next_key);
        let energy_delta = playing.energy.zip(features.energy).map(|(a, b)| b - a);
        let fits = [
            key_score(relation, playing.key_confidence, features.key_confidence),
            tempo.map_or(0.5, |t| t.score),
            energy_delta.map_or(0.5, |d| energy_fit(d, direction)),
        ];
        scored.push(Scored {
            features,
            score: fits.iter().zip(WEIGHTS).map(|(fit, w)| fit * w).sum(),
            relation,
            next_key,
            tempo,
            energy_delta,
        });
    }
    if scored.len() > limit && limit > 0 {
        scored.select_nth_unstable_by(limit - 1, better);
    }
    scored.truncate(limit);
    scored.sort_by(better);
    ranking.suggestions = scored
        .into_iter()
        .map(|s| Suggestion {
            source: s.features.source.clone(),
            spotify: s.features.source.starts_with("spotify:"),
            track_id: s.features.track_id,
            title: s.features.title.clone(),
            artist: s.features.artist.clone(),
            duration_seconds: s.features.duration_seconds,
            bpm: s.features.bpm.unwrap_or_default(),
            camelot: s.next_key.map(|k| k.to_string()),
            energy: s.features.energy,
            energy_level: s.features.energy.map(energy_level),
            key: s.relation,
            key_clash: s.relation == KeyRelation::Clash,
            reason: reason(playing_key, s.next_key, s.tempo.as_ref(), s.energy_delta),
            tempo: s.tempo,
            energy_delta: s.energy_delta,
            score: s.score,
        })
        .collect();
    ranking
}

/// Constant-power crossfader gains for decks A and B, as the mixer applies them.
pub fn crossfader_gains(position: f32) -> [f32; 2] {
    let position = if position.is_finite() {
        position.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let angle = (position + 1.0) * std::f32::consts::FRAC_PI_4;
    [angle.cos(), angle.sin()]
}

/// How loud each deck's channel fader and the crossfader let it reach the
/// master, whether or not it plays.
fn fader_levels(snapshot: &EngineSnapshot) -> [f32; 2] {
    let fade = crossfader_gains(snapshot.crossfader);
    std::array::from_fn(|deck| {
        let fader = snapshot.decks[deck].channel_fader;
        fade[deck]
            * if fader.is_finite() {
                fader.clamp(0.0, 1.0)
            } else {
                0.0
            }
    })
}

/// The deck the audience hears: the louder playing deck, the master when
/// both are about as loud. With nothing audible, the loaded deck whose faders
/// are up, so suggestions are ready before the first track starts.
pub fn audible_deck(snapshot: &EngineSnapshot, loaded: [bool; 2]) -> Option<usize> {
    let level = fader_levels(snapshot);
    let pick = |decks: Vec<usize>| -> Option<usize> {
        match decks.as_slice() {
            [] => None,
            [deck] => Some(*deck),
            _ if (level[0] - level[1]).abs() <= 0.1 => Some(
                decks
                    .iter()
                    .copied()
                    .find(|&d| snapshot.decks[d].is_master)
                    .unwrap_or(0),
            ),
            _ => Some(if level[0] >= level[1] { 0 } else { 1 }),
        }
    };
    pick(
        (0..2)
            .filter(|&d| loaded[d] && snapshot.decks[d].playing && level[d] > AUDIBLE_GAIN)
            .collect(),
    )
    .or_else(|| pick((0..2).filter(|&d| loaded[d]).collect()))
}

/// Tracks heard this session, kept in memory until the app quits.
#[derive(Debug, Default)]
pub struct PlayHistory {
    heard: HashMap<String, f64>,
    played: HashSet<String>,
}

impl PlayHistory {
    /// Count `seconds` of audible playback for each deck's track (`sources`
    /// are their paths or URIs). A track heard for [`PLAYED_AFTER_SECONDS`]
    /// is played; previewing it in headphones with the fader down is not.
    pub fn observe(
        &mut self,
        snapshot: &EngineSnapshot,
        sources: &[Option<String>; 2],
        seconds: f64,
    ) {
        let level = fader_levels(snapshot);
        for (deck, source) in sources.iter().enumerate() {
            let Some(source) = source else {
                continue;
            };
            if !snapshot.decks[deck].playing
                || level[deck] <= AUDIBLE_GAIN
                || self.played.contains(source)
            {
                continue;
            }
            let heard = self.heard.entry(source.clone()).or_default();
            *heard += seconds;
            if *heard >= PLAYED_AFTER_SECONDS {
                self.heard.remove(source);
                self.played.insert(source.clone());
            }
        }
    }

    pub fn played(&self) -> &HashSet<String> {
        &self.played
    }

    pub fn clear(&mut self) {
        self.heard.clear();
        self.played.clear();
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SuggestionList {
    /// The audible deck the suggestions follow.
    pub deck: Option<usize>,
    /// The deck suggestions load into.
    pub target_deck: Option<usize>,
    pub direction: EnergyDirection,
    pub reference: Option<Reference>,
    /// The other deck's tempo range, in percent.
    pub tempo_range: f64,
    #[serde(flatten)]
    pub ranking: Ranking,
    /// Tracks whose features are still being prepared in the background.
    pub pending: usize,
    pub elapsed_ms: f64,
}

/// Suggest what to play after the track on `deck` (the audible deck when
/// omitted), for loading into the other deck.
#[tauri::command(rename_all = "snake_case")]
pub async fn suggest_next(
    app: tauri::AppHandle,
    deck: Option<usize>,
    energy_direction: Option<EnergyDirection>,
) -> Result<SuggestionList, String> {
    if deck.is_some_and(|d| d > 1) {
        return Err("Deck must be 0 or 1".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        suggest(&app, deck, energy_direction.unwrap_or_default())
    })
    .await
    .map_err(|_| "Suggestion worker failed".to_string())?
}

/// Forget which tracks were played this session.
#[tauri::command]
pub fn clear_play_history(app: tauri::AppHandle) -> Result<(), String> {
    app.state::<crate::AppState>()
        .play_history
        .lock()
        .map_err(|_| crate::poisoned())?
        .clear();
    Ok(())
}

fn suggest(
    app: &tauri::AppHandle,
    deck: Option<usize>,
    direction: EnergyDirection,
) -> Result<SuggestionList, String> {
    let started = Instant::now();
    let state = app.state::<crate::AppState>();
    let transport = state.engine.transport_snapshot();
    let metadata = state.engine.metadata_snapshot();
    let loaded = [metadata.tracks[0].is_some(), metadata.tracks[1].is_some()];
    let deck = deck.or_else(|| audible_deck(&transport, loaded));
    let mut list = SuggestionList {
        deck,
        target_deck: deck.map(|d| 1 - d),
        direction,
        reference: None,
        tempo_range: 0.0,
        ranking: Ranking::default(),
        pending: 0,
        elapsed_ms: 0.0,
    };
    let Some((deck, track)) = deck.and_then(|d| metadata.tracks[d].as_ref().map(|t| (d, t))) else {
        return Ok(list);
    };
    list.tempo_range = state
        .controller
        .lock()
        .map_err(|_| crate::poisoned())?
        .surface()
        .tempo_ranges[1 - deck];
    let played = state
        .play_history
        .lock()
        .map_err(|_| crate::poisoned())?
        .played()
        .clone();
    let excluded: HashSet<String> = metadata
        .tracks
        .iter()
        .flatten()
        .map(|t| t.path.clone())
        .collect();
    let reference = Reference::from_deck(
        track,
        transport.decks[deck].bpm,
        transport.decks[deck].tempo_percent,
    );
    {
        let library = state.library.lock().map_err(|_| crate::poisoned())?;
        list.pending = library.features_pending()?;
        list.ranking = library.with_features(|pool| {
            rank(
                pool.values(),
                &reference,
                direction,
                list.tempo_range,
                &excluded,
                &played,
                LIMIT,
            )
        })?;
    }
    list.reference = Some(reference);
    if list.pending > 0 {
        spawn_backfill(app);
    }
    list.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    Ok(list)
}

/// Compute features for records stored before suggestions existed, a batch at
/// a time. Records are read under the library lock and parsed outside it, so
/// deck loads are never held up for long.
fn spawn_backfill(app: &tauri::AppHandle) {
    let state = app.state::<crate::AppState>();
    if state.features_backfill.swap(true, AtomicOrdering::AcqRel) {
        return;
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("pulsedeck-suggest-features".into())
        .spawn(move || {
            let state = app.state::<crate::AppState>();
            let result = (|| -> Result<(), String> {
                loop {
                    let batch = state
                        .library
                        .lock()
                        .map_err(|_| crate::poisoned())?
                        .pending_features(BACKFILL_BATCH)?;
                    if batch.is_empty() {
                        return Ok(());
                    }
                    let features: Vec<TrackFeatures> =
                        batch.into_iter().map(PendingFeatures::compute).collect();
                    state
                        .library
                        .lock()
                        .map_err(|_| crate::poisoned())?
                        .store_features(&features)?;
                    std::thread::sleep(Duration::from_millis(2));
                }
            })();
            if let Err(e) = result {
                state.message(format!("Suggestions could not prepare the library: {e}"));
            }
            state
                .features_backfill
                .store(false, AtomicOrdering::Release);
        });
    if spawned.is_err() {
        state
            .features_backfill
            .store(false, AtomicOrdering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn features(source: &str, bpm: f64, camelot: &str, energy: f32) -> TrackFeatures {
        TrackFeatures {
            source: source.into(),
            track_id: None,
            title: source.into(),
            artist: String::new(),
            duration_seconds: 300.0,
            bpm: Some(bpm),
            bpm_ambiguous: false,
            camelot: (!camelot.is_empty()).then(|| camelot.into()),
            key_confidence: Some(0.8),
            energy: Some(energy),
        }
    }

    fn playing(bpm: f64, camelot: &str, energy: f32) -> Reference {
        Reference {
            source: "/music/playing.mp3".into(),
            title: "Playing".into(),
            bpm: Some(bpm),
            bpm_ambiguous: false,
            camelot: Some(camelot.into()),
            key_confidence: Some(0.8),
            energy: Some(energy),
            energy_level: Some(energy_level(energy)),
        }
    }

    fn ranked(
        pool: &[TrackFeatures],
        reference: &Reference,
        direction: EnergyDirection,
    ) -> Ranking {
        rank(
            pool,
            reference,
            direction,
            6.0,
            &HashSet::new(),
            &HashSet::new(),
            LIMIT,
        )
    }

    fn sources(ranking: &Ranking) -> Vec<&str> {
        ranking
            .suggestions
            .iter()
            .map(|s| s.source.as_str())
            .collect()
    }

    fn key(text: &str) -> Option<Camelot> {
        Camelot::parse(text)
    }

    fn snapshot(crossfader: f32) -> EngineSnapshot {
        let deck = serde_json::json!({
            "track": null, "playing": false, "position_seconds": 0.0, "playback_rate": 1.0,
            "tempo_percent": 0.0, "gain": 1.0, "channel_fader": 1.0, "sync_enabled": false,
            "is_master": false, "eq_low": 1.0, "eq_mid": 1.0, "eq_high": 1.0, "filter": 0.0,
            "cue_seconds": 0.0, "hot_cues": [null, null, null, null, null, null, null, null],
            "memory_cues": [null, null, null, null, null, null, null, null, null, null],
            "peak": [0.0, 0.0], "loop_start_seconds": null, "loop_end_seconds": null,
            "loop_edit": null, "fx_beats": 1.0, "cue_enabled": false, "echo": 0.0, "reverb": 0.0,
            "bpm": null, "beatgrid_offset_seconds": 0.0, "pitch_semitones": 0.0,
            "keylock": false, "preparation_pending": false
        });
        serde_json::from_value(serde_json::json!({
            "sample_rate": 48000, "output_channels": 2, "buffer_frames": 256, "running": true, "load_revisions": [0,0],
            "rendered_frames": 0, "master_peak": [0.0, 0.0], "cue_peak": [0.0, 0.0],
            "underruns": 0, "master_gain": 1.0, "crossfader": crossfader, "cue_gain": 1.0,
            "cue_mix": 0.0, "sampler_gain": 1.0, "sampler_positions_seconds": vec![Option::<f64>::None; 16], "active_ramps": vec![false; 17], "decks": [deck.clone(), deck]
        }))
        .unwrap()
    }

    #[test]
    fn camelot_rules_cover_same_key_one_step_and_the_ab_switch() {
        use KeyRelation::*;
        assert_eq!(
            key("8A"),
            Some(Camelot {
                number: 8,
                major: false
            })
        );
        assert_eq!(
            key(" 12b "),
            Some(Camelot {
                number: 12,
                major: true
            })
        );
        assert_eq!(key("08A").map(|k| k.to_string()).as_deref(), Some("8A"));
        for bad in ["", "A", "13A", "0B", "8C", "Am", "8♯"] {
            assert_eq!(key(bad), None, "{bad}");
        }
        for (from, to, relation) in [
            ("8A", "8A", Same),
            ("8A", "9A", StepUp),
            ("8A", "7A", StepDown),
            ("12B", "1B", StepUp),
            ("1B", "12B", StepDown),
            ("8A", "8B", Relative),
            ("3B", "3A", Relative),
            ("8A", "10A", Clash),
            ("8A", "9B", Clash),
            ("8A", "2A", Clash),
            ("5B", "11B", Clash),
        ] {
            assert_eq!(
                KeyRelation::between(key(from), key(to)),
                relation,
                "{from}→{to}"
            );
        }
        assert_eq!(KeyRelation::between(key("8A"), None), Unknown);
        assert_eq!(KeyRelation::between(None, key("8A")), Unknown);
    }

    #[test]
    fn key_confidence_shrinks_rewards_and_penalties_towards_neutral() {
        let sure = key_score(KeyRelation::Same, Some(0.9), Some(0.7));
        let unsure = key_score(KeyRelation::Same, Some(0.9), Some(0.15));
        assert_eq!(sure, 1.0);
        assert!(unsure < sure && unsure > 0.5);
        assert_eq!(key_score(KeyRelation::Clash, Some(0.8), Some(0.8)), 0.0);
        assert!(key_score(KeyRelation::Clash, Some(0.8), Some(0.15)) > 0.3);
        assert_eq!(key_score(KeyRelation::Unknown, Some(1.0), Some(1.0)), 0.5);
        assert!(
            key_score(KeyRelation::StepUp, Some(0.8), Some(0.8))
                > key_score(KeyRelation::Relative, Some(0.8), Some(0.8))
        );
    }

    #[test]
    fn tempo_matches_directly_or_at_half_and_double_time_within_the_range() {
        let direct = tempo_match(126.0, 124.0, false, 6.0).unwrap();
        assert_eq!(direct.multiple, 1.0);
        assert!((direct.percent - 1.6129).abs() < 1e-3);
        // A 71 BPM reading of a drill beat matches 142 at double its BPM.
        let half_time = tempo_match(142.0, 71.2, true, 6.0).unwrap();
        assert_eq!(half_time.multiple, 2.0);
        assert!((half_time.percent + 0.2809).abs() < 1e-3);
        let double_time = tempo_match(70.0, 141.0, true, 6.0).unwrap();
        assert_eq!(double_time.multiple, 0.5);
        // Unflagged half/double still mixes, at a cost.
        let unflagged = tempo_match(142.0, 71.2, false, 6.0).unwrap();
        assert!(unflagged.score < half_time.score && unflagged.score > 0.5);
        // House at 124 against hip-hop at 92 is out of reach of ±6%, ±10% and ±16%.
        for range in [6.0, 10.0, 16.0] {
            assert!(tempo_match(124.0, 92.0, false, range).is_none());
        }
        assert!(tempo_match(124.0, 92.0, false, 100.0).is_some());
        assert!(tempo_match(120.0, 128.0, false, 6.0).is_none());
        assert!(tempo_match(120.0, 128.0, false, 10.0).is_some());
        assert!(tempo_match(124.0, f64::NAN, false, 6.0).is_none());
        assert!(
            tempo_match(124.0, 124.0, false, 6.0).unwrap().score
                > tempo_match(124.0, 121.0, false, 6.0).unwrap().score
        );
    }

    #[test]
    fn energy_direction_prefers_the_chosen_way() {
        use EnergyDirection::*;
        let fit = energy_fit;
        assert!(fit(0.0, Keep) > fit(0.05, Keep));
        assert!(fit(0.05, Keep) > fit(0.15, Keep));
        assert!((fit(0.05, Keep) - fit(-0.05, Keep)).abs() < 1e-6);
        assert!(fit(0.1, Raise) > fit(0.05, Raise));
        assert!(fit(0.05, Raise) > fit(0.0, Raise));
        assert!(fit(0.0, Raise) > fit(-0.05, Raise));
        assert!(fit(0.25, Raise) > fit(0.0, Raise));
        assert!(fit(-0.1, Lower) > fit(0.0, Lower));
        assert!(fit(0.0, Lower) > fit(0.1, Lower));
        // The same pool reorders with the direction.
        let pool = [
            features("/up.mp3", 124.0, "8A", 0.7),
            features("/same.mp3", 124.0, "8A", 0.6),
            features("/down.mp3", 124.0, "8A", 0.5),
        ];
        let reference = playing(124.0, "8A", 0.6);
        assert_eq!(sources(&ranked(&pool, &reference, Raise))[0], "/up.mp3");
        assert_eq!(sources(&ranked(&pool, &reference, Keep))[0], "/same.mp3");
        assert_eq!(sources(&ranked(&pool, &reference, Lower))[0], "/down.mp3");
    }

    #[test]
    fn energy_score_rises_with_loudness_and_brightness_and_ignores_quiet_intros() {
        let bands = |low: f32, high: f32, quiet_bins: usize| {
            let mut b = WaveformBands {
                low: vec![low; 4096],
                mid: vec![low * 0.5; 4096],
                high: vec![high; 4096],
            };
            for i in 0..quiet_bins {
                b.low[i] = 0.01;
                b.mid[i] = 0.01;
                b.high[i] = 0.0;
            }
            b
        };
        let quiet = energy_score(&bands(0.15, 0.03, 0)).unwrap();
        let loud = energy_score(&bands(0.4, 0.08, 0)).unwrap();
        let bright = energy_score(&bands(0.4, 0.2, 0)).unwrap();
        assert!(quiet < loud && loud < bright, "{quiet} {loud} {bright}");
        assert!((0.0..=1.0).contains(&quiet) && (0.0..=1.0).contains(&bright));
        // A long quiet intro (a third of the track) does not lower the score.
        let intro = energy_score(&bands(0.4, 0.08, 1365)).unwrap();
        assert!((intro - loud).abs() < 0.02, "{intro} {loud}");
        assert_eq!(energy_score(&bands(0.0, 0.0, 0)), None);
        let empty = WaveformBands {
            low: vec![],
            mid: vec![],
            high: vec![],
        };
        assert_eq!(energy_score(&empty), None);
        assert_eq!(energy_level(0.0), 1);
        assert_eq!(energy_level(0.5), 6);
        assert_eq!(energy_level(1.0), 10);
    }

    #[test]
    fn reasons_name_the_keys_tempo_change_and_energy() {
        let tempo = |percent, multiple| TempoMatch {
            percent,
            multiple,
            score: 1.0,
        };
        assert_eq!(
            reason(key("8A"), key("9A"), Some(&tempo(1.5, 1.0)), Some(0.08)),
            "8A→9A · +1.5% · energy up"
        );
        assert_eq!(
            reason(key("8A"), key("8B"), Some(&tempo(-2.04, 1.0)), Some(-0.1)),
            "8A→8B · -2.0% · energy down"
        );
        assert_eq!(
            reason(key("2A"), key("2A"), Some(&tempo(0.01, 2.0)), Some(0.0)),
            "2A→2A · ±0.0% half-time · same energy"
        );
        assert_eq!(
            reason(key("8A"), key("3B"), Some(&tempo(0.3, 0.5)), None),
            "8A→3B clash · +0.3% double-time"
        );
        assert_eq!(reason(None, key("3B"), None, None), "key ?");
    }

    #[test]
    fn exclusions_drop_deck_tracks_played_tracks_and_unreachable_tempos() {
        let pool = [
            features("/music/playing.mp3", 124.0, "8A", 0.6),
            features("/music/other-deck.mp3", 124.0, "8A", 0.6),
            features("/music/played.mp3", 124.0, "8A", 0.6),
            features("/music/hip-hop.mp3", 92.0, "8A", 0.6),
            features("/music/fresh.mp3", 125.0, "9A", 0.6),
            TrackFeatures {
                bpm: None,
                ..features("/music/no-bpm.mp3", 0.0, "8A", 0.6)
            },
        ];
        let excluded = HashSet::from([
            "/music/playing.mp3".to_string(),
            "/music/other-deck.mp3".to_string(),
        ]);
        let played = HashSet::from(["/music/played.mp3".to_string()]);
        let ranking = rank(
            &pool,
            &playing(124.0, "8A", 0.6),
            EnergyDirection::Keep,
            6.0,
            &excluded,
            &played,
            LIMIT,
        );
        assert_eq!(sources(&ranking), ["/music/fresh.mp3"]);
        assert_eq!(ranking.considered, 1);
        assert_eq!(ranking.played, 1);
        assert_eq!(ranking.out_of_range, 1);
        assert_eq!(ranking.without_tempo, 1);
        let s = &ranking.suggestions[0];
        assert_eq!(s.reason, "8A→9A · -0.8% · same energy");
        assert_eq!(s.key, KeyRelation::StepUp);
        assert!(!s.key_clash && !s.spotify);
    }

    #[test]
    fn compatible_keys_outrank_clashes_and_every_clash_is_flagged() {
        let mut pool = Vec::new();
        for (i, camelot) in ["8A", "9A", "7A", "8B", "3A", "10B", "1B", "6A"]
            .iter()
            .enumerate()
        {
            for (j, bpm) in [124.0, 125.0, 122.5].iter().enumerate() {
                pool.push(features(&format!("/{i}-{j}.mp3"), *bpm, camelot, 0.6));
            }
        }
        let ranking = ranked(&pool, &playing(124.0, "8A", 0.6), EnergyDirection::Keep);
        let compatible = 4 * 3;
        for (index, s) in ranking.suggestions.iter().enumerate() {
            assert_eq!(s.key_clash, s.key == KeyRelation::Clash);
            assert_eq!(s.key_clash, s.reason.contains("clash"), "{}", s.reason);
            assert_eq!(s.key_clash, index >= compatible, "{index}: {}", s.reason);
        }
        // Cached Spotify tracks are suggested alongside local files.
        pool.push(features(
            "spotify:track:0000000000000000000000",
            124.0,
            "8A",
            0.6,
        ));
        let ranking = ranked(&pool, &playing(124.0, "8A", 0.6), EnergyDirection::Keep);
        assert!(ranking.suggestions.iter().take(3).any(|s| s.spotify));
    }

    #[test]
    fn audible_deck_follows_play_state_faders_and_crossfader() {
        let mut snapshot = snapshot(0.0);
        let both = [true, true];
        assert_eq!(audible_deck(&snapshot, [false, false]), None);
        // Nothing playing: the loaded deck, or the master on a tie.
        assert_eq!(audible_deck(&snapshot, [false, true]), Some(1));
        snapshot.decks[1].is_master = true;
        assert_eq!(audible_deck(&snapshot, both), Some(1));
        snapshot.decks[0].playing = true;
        assert_eq!(audible_deck(&snapshot, both), Some(0));
        snapshot.decks[1].playing = true;
        assert_eq!(audible_deck(&snapshot, both), Some(1));
        snapshot.crossfader = -0.8;
        assert_eq!(audible_deck(&snapshot, both), Some(0));
        snapshot.crossfader = 0.0;
        snapshot.decks[1].channel_fader = 0.2;
        assert_eq!(audible_deck(&snapshot, both), Some(0));
        // With its channel fader closed, a playing deck is not audible.
        snapshot.decks[0].channel_fader = 0.0;
        snapshot.decks[1].playing = false;
        assert_eq!(audible_deck(&snapshot, both), Some(1));
        assert!(crossfader_gains(-1.0)[1].abs() < 1e-6);
        assert!((crossfader_gains(0.0)[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    }

    #[test]
    fn played_means_heard_for_thirty_seconds_with_the_fader_up() {
        let mut snapshot = snapshot(-1.0);
        snapshot.decks[0].playing = true;
        snapshot.decks[1].playing = true;
        let sources = [Some("/a.mp3".to_string()), Some("/b.mp3".to_string())];
        let mut history = PlayHistory::default();
        for _ in 0..299 {
            history.observe(&snapshot, &sources, 0.1);
        }
        assert!(history.played().is_empty());
        history.observe(&snapshot, &sources, 0.2);
        // Deck B plays in headphones behind a closed crossfader: not played.
        assert_eq!(history.played(), &HashSet::from(["/a.mp3".to_string()]));
        history.clear();
        assert!(history.played().is_empty());
    }

    #[test]
    fn reference_uses_the_heard_tempo_and_drops_ambiguity_after_a_manual_bpm() {
        let samples = vec![[0.0; 2]; 4000];
        let mut track = deck_engine::analyze_decoded_audio(
            &samples,
            8000,
            &deck_engine::AudioSourceInfo {
                id: "/a.mp3".into(),
                location: "/a.mp3".into(),
                title: "A".into(),
                artist: None,
                extension: None,
            },
            true,
        )
        .unwrap();
        track.bpm = Some(71.0);
        track.bpm_source = Some("model".into());
        track.bpm_ambiguous = true;
        let as_loaded = Reference::from_deck(&track, Some(71.0), 2.0);
        assert!((as_loaded.bpm.unwrap() - 72.42).abs() < 1e-9);
        assert!(as_loaded.bpm_ambiguous);
        let edited = Reference::from_deck(&track, Some(142.0), 0.0);
        assert_eq!(edited.bpm, Some(142.0));
        assert!(!edited.bpm_ambiguous);
    }

    /// The acceptance target is 50 ms per request on a 10,000-track library.
    /// Optimised builds take a few milliseconds; debug builds get the full
    /// budget so the test stays meaningful without flaking.
    #[test]
    fn ranking_ten_thousand_tracks_is_fast() {
        let keys = ["1A", "2A", "8A", "9A", "8B", "12B", "5A", "6B"];
        let pool: Vec<TrackFeatures> = (0..10_000)
            .map(|i| TrackFeatures {
                bpm_ambiguous: i % 7 == 0,
                key_confidence: Some(0.3 + (i % 5) as f32 * 0.1),
                ..features(
                    &format!("/music/{i:05}.mp3"),
                    70.0 + (i % 90) as f64,
                    keys[i % keys.len()],
                    (i % 100) as f32 / 100.0,
                )
            })
            .collect();
        let reference = playing(124.0, "8A", 0.55);
        let played: HashSet<String> = (0..300).map(|i| format!("/music/{i:05}.mp3")).collect();
        let started = Instant::now();
        let runs = 5;
        for _ in 0..runs {
            let ranking = rank(
                &pool,
                &reference,
                EnergyDirection::Raise,
                16.0,
                &HashSet::new(),
                &played,
                LIMIT,
            );
            assert_eq!(ranking.suggestions.len(), LIMIT);
            assert!(ranking.suggestions.iter().take(10).all(|s| !s.key_clash));
        }
        let per_request = started.elapsed().as_secs_f64() * 1000.0 / runs as f64;
        let budget = if cfg!(debug_assertions) { 50.0 } else { 5.0 };
        assert!(per_request < budget, "{per_request:.2} ms per request");
    }
}
