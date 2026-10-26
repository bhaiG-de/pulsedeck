//! Read-only passage/technique planner shared by the UI and control clients.
//! Scores are transparent heuristics, never probabilities of musical success.
use crate::suggest::{self, Camelot, EnergyDirection, KeyRelation, SuggestionOptions};
use deck_engine::{DeckSnapshot, EngineSnapshot, TrackAnalysis};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
    time::Instant,
};
use tauri::Manager;

pub const PLANNER_VERSION: u32 = 7;
pub const DEFAULT_MAX_STRETCH_PERCENT: f64 = 4.;
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OptionsRequest {
    pub deck: Option<usize>,
    pub energy_direction: EnergyDirection,
    pub playlist: Option<String>,
    pub provider: Option<String>,
    pub sources: Option<Vec<String>>,
    pub max_stretch_percent: Option<f64>,
    pub techniques: Option<Vec<String>>,
    pub limit: Option<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoundaryProposal {
    pub seconds: f64,
    pub event: i64,
    pub beat: f64,
    pub evidence: String,
    pub confirmed: bool,
    pub contrast: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PassageFeatures {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub low: f64,
    pub mid: f64,
    pub high: f64,
    pub level: f64,
    pub pulse_spread: f64,
    pub local_bpm: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MoveOption {
    pub id: String,
    pub source: String,
    pub title: String,
    pub artist: String,
    pub spotify: bool,
    pub reference_deck: usize,
    pub target_deck: usize,
    pub technique: String,
    pub overlap_bars: usize,
    pub entry_seconds: f64,
    pub launch_seconds: f64,
    pub audible_seconds: f64,
    pub handover_seconds: f64,
    pub clear_seconds: f64,
    pub launch_event: i64,
    pub handover_event: i64,
    pub clear_event: i64,
    pub target_bpm: f64,
    pub native_bpm: f64,
    pub tempo_percent: f64,
    pub max_stretch_percent: f64,
    pub reference_source: String,
    pub reference_load_revision: u64,
    pub reference_grid_revision: u64,
    pub reference_position_revision: u64,
    pub reference_control_revision: u64,
    pub engine_epoch: u64,
    pub review_required: bool,
    pub evidence: Vec<String>,
    pub warnings: Vec<String>,
    pub score: f64,
    pub score_components: HashMap<String, f64>,
    pub outgoing: PassageFeatures,
    pub incoming: PassageFeatures,
    pub launch_in_seconds: f64,
    pub prepare_within_seconds: f64,
    pub preparation_budget_seconds: f64,
    /// Wall-clock music remaining after native clearance/verification.
    #[serde(default)]
    pub continuation_seconds: f64,
    #[serde(default)]
    pub minimum_continuation_seconds: f64,
    #[serde(default)]
    pub next_preparation_budget_seconds: f64,
    #[serde(default)]
    pub next_decision_within_seconds: f64,
    pub readiness: String,
    pub incoming_fingerprint: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct TechniqueCapability {
    pub technique: &'static str,
    pub available: bool,
    pub reason: &'static str,
}
#[derive(Serialize)]
pub struct OptionsList {
    pub version: u32,
    pub reference_deck: Option<usize>,
    pub target_deck: Option<usize>,
    pub reference_title: Option<String>,
    pub reference_position_seconds: f64,
    pub options: Vec<MoveOption>,
    pub considered_tracks: usize,
    pub coverage: crate::library::PlannerCoverage,
    pub capabilities: Vec<TechniqueCapability>,
    pub reasons: Vec<String>,
    pub rejected_window_checks: HashMap<String, usize>,
    pub candidate_rejections: HashMap<String, usize>,
    pub elapsed_ms: f64,
}
#[derive(Default)]
struct Cache {
    options: HashMap<String, MoveOption>,
    evidence: HashMap<String, Vec<BoundaryProposal>>,
}
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
fn cache() -> &'static Mutex<Cache> {
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}
fn identity(value: impl Serialize) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value).unwrap_or_default())
    )
}
fn same_recording(a: &TrackAnalysis, b: &TrackAnalysis) -> bool {
    a.path == b.path
        || a.source_fingerprint
            .as_ref()
            .is_some_and(|fingerprint| b.source_fingerprint.as_ref() == Some(fingerprint))
}
fn eligible_successor(
    reference: Option<&TrackAnalysis>,
    candidate: &TrackAnalysis,
    played: &HashSet<String>,
) -> bool {
    !played.contains(&candidate.path)
        && reference.is_none_or(|reference| !same_recording(reference, candidate))
}
pub fn capabilities() -> Vec<TechniqueCapability> {
    [
        (
            "eq_blend",
            true,
            "Staged EQ and fader blend on a stable shared pulse",
        ),
        (
            "filter_blend",
            true,
            "Bounded high-pass filtering creates space during the overlap",
        ),
        (
            "short_exchange",
            true,
            "Four-bar exchange with a short controlled overlap",
        ),
        (
            "bass_exchange",
            false,
            "Bass arrival needs confirmed passage roles; EQ blend remains available",
        ),
        (
            "payoff_replacement",
            false,
            "Requires confirmed buildup and payoff relationships",
        ),
        (
            "breakdown_bridge",
            false,
            "Beatless clock and pulse-return validation are not available",
        ),
        (
            "vocal_handoff",
            false,
            "No validated vocal activity/phrase detector; vocals remain unknown",
        ),
        (
            "echo_out",
            true,
            "Eight-bar exchange with a tempo-synced independent echo return",
        ),
        (
            "reverb_exit",
            true,
            "Eight-bar exchange with an independent bounded reverb return",
        ),
        (
            "loop_blend",
            false,
            "Native guarded launches require linear, non-looping source clocks",
        ),
        (
            "tempo_bridge",
            false,
            "A validated tempo-curve recipe is not available",
        ),
        (
            "key_shift_bridge",
            false,
            "Local tonal/transposition quality has not been validated",
        ),
        (
            "stem_blend",
            false,
            "No general stem mixer or source separation capability",
        ),
        (
            "performance_gesture",
            false,
            "No leased scratch/brake/backspin trajectory recipe",
        ),
    ]
    .into_iter()
    .map(|(technique, available, reason)| TechniqueCapability {
        technique,
        available,
        reason,
    })
    .collect()
}
pub(crate) fn analysis_deck(track: &TrackAnalysis) -> Result<DeckSnapshot, String> {
    // Use the same effective-grid mapping as native rendering, including manual edits.
    let mut d = empty_deck();
    d.track = Some(track.clone());
    d.bpm = track.bpm;
    d.beatgrid_uniform = track.bpm_source.as_deref() == Some("manual");
    d.beatgrid_manual = track.beatgrid_source.as_deref() == Some("manual");
    d.beatgrid_offset_seconds = track.beatgrid_offset_seconds.unwrap_or(0.);
    d.beatgrid_anchor_index = track.manual_grid_anchor_index.or_else(|| {
        track.neural_rhythm.as_ref().and_then(|r| {
            r.downbeats_seconds
                .first()
                .and_then(|t| r.beats_seconds.iter().position(|b| (b - t).abs() < 1e-5))
        })
    });
    d.beatgrid_valid = track.bpm.is_some_and(|b| b.is_finite() && b > 0.)
        && (d.beatgrid_uniform
            || track
                .neural_rhythm
                .as_ref()
                .is_some_and(|r| r.beats_seconds.len() > 8));
    if !d.beatgrid_valid {
        return Err("Effective beat/downbeat grid is unavailable".into());
    }
    Ok(d)
}
fn empty_deck() -> DeckSnapshot {
    // Default is derived from the engine's own serialised empty transport in tests too.
    serde_json::from_value(serde_json::json!({"track":null,"playing":false,"position_seconds":0.,"playback_rate":1.,"actual_playback_rate":1.,"playback_bpm":null,"local_bpm":null,"actual_playback_bpm":null,"target_playback_bpm":null,"tempo_percent":0.,"gain":1.,"channel_fader":0.,"sync_enabled":false,"is_master":false,"eq_low":1.,"eq_mid":1.,"eq_high":1.,"filter":0.,"cue_seconds":0.,"hot_cues":[null,null,null,null,null,null,null,null],"memory_cues":[null,null,null,null,null,null,null,null,null,null],"peak":[0.,0.],"loop_start_seconds":null,"loop_end_seconds":null,"loop_edit":null,"fx_beats":1.,"cue_enabled":false,"echo":0.,"reverb":0.,"bpm":null,"beatgrid_offset_seconds":0.,"pitch_semitones":0.,"keylock":true,"preparation_pending":false})).expect("empty deck schema")
}
pub(crate) fn event(d: &DeckSnapshot, seconds: f64) -> Option<i64> {
    Some(
        d.grid_position(seconds)?.round() as i64
            + if d.beatgrid_uniform {
                0
            } else {
                d.beatgrid_anchor_index.unwrap_or(0) as i64
            },
    )
}
fn at(d: &DeckSnapshot, event: i64) -> Option<f64> {
    d.grid_time(
        (event
            - if d.beatgrid_uniform {
                0
            } else {
                d.beatgrid_anchor_index.unwrap_or(0) as i64
            }) as f64,
    )
}
fn bands(d: &DeckSnapshot, start: f64, end: f64) -> Option<(f64, f64, f64)> {
    let t = d.track.as_ref()?;
    let b = t.waveform_bands.as_ref()?;
    let n = b.low.len().min(b.mid.len()).min(b.high.len());
    if n < 16 || end <= start || t.duration_seconds <= 0. {
        return None;
    }
    let lo = ((start / t.duration_seconds * n as f64).floor().max(0.) as usize).min(n - 1);
    let hi = ((end / t.duration_seconds * n as f64).ceil() as usize).clamp(lo + 1, n);
    let rms = |a: &[f32]| {
        let v = a[lo..hi]
            .iter()
            .map(|v| *v as f64)
            .filter(|v| v.is_finite())
            .map(|v| v * v)
            .sum::<f64>();
        (v / (hi - lo) as f64).sqrt()
    };
    Some((rms(&b.low), rms(&b.mid), rms(&b.high)))
}
pub(crate) fn useful_end_seconds(d: &DeckSnapshot) -> f64 {
    let Some(t) = d.track.as_ref() else { return 0. };
    if let Some(m) = t.audio_measurements.as_ref().filter(|m| {
        m.current(
            t.sample_rate,
            (t.duration_seconds * t.sample_rate as f64).round() as usize,
        )
    }) {
        return m.audible_end_seconds().unwrap_or(0.);
    }
    let Some(b) = t.waveform_bands.as_ref() else {
        return 0.;
    };
    let n = b.low.len().min(b.mid.len()).min(b.high.len());
    (0..n)
        .rfind(|&i| {
            let level =
                (b.low[i] as f64).powi(2) + (b.mid[i] as f64).powi(2) + (b.high[i] as f64).powi(2);
            level.is_finite() && level.sqrt() >= 0.008
        })
        .map_or(0., |i| (i + 1) as f64 / n as f64 * t.duration_seconds)
}
fn passage(d: &DeckSnapshot, start: f64, end: f64) -> Option<PassageFeatures> {
    let (low, mid, high) = bands(d, start, end)?;
    let first = event(d, start)?;
    let last = event(d, end)?;
    if last - first < 8 {
        return None;
    }
    let mut tempi = Vec::new();
    for e in first..last - 3 {
        let a = at(d, e)?;
        let b = at(d, e + 4)?;
        if b <= a {
            return None;
        }
        tempi.push(240. / (b - a));
    }
    if tempi
        .iter()
        .any(|b| !b.is_finite() || !(30. ..=300.).contains(b))
    {
        return None;
    }
    tempi.sort_by(f64::total_cmp);
    let local_bpm = tempi[tempi.len() / 2];
    let pulse_spread =
        (tempi[(tempi.len() - 1) * 95 / 100] - tempi[(tempi.len() - 1) * 5 / 100]) / local_bpm;
    Some(PassageFeatures {
        start_seconds: start,
        end_seconds: end,
        low,
        mid,
        high,
        level: (low * low + mid * mid + high * high).sqrt(),
        pulse_spread,
        local_bpm,
    })
}
fn boundaries(d: &DeckSnapshot) -> Vec<BoundaryProposal> {
    let Some(t) = d.track.as_ref() else {
        return vec![];
    };
    let key = identity((
        PLANNER_VERSION,
        &t.path,
        &t.source_fingerprint,
        t.analysis_version,
        &t.analysis_stages,
        t.neural_rhythm.as_ref().map(|rhythm| &rhythm.model),
        d.grid_revision,
        d.bpm,
        d.beatgrid_offset_seconds,
        d.beatgrid_anchor_index,
        &t.cue_points,
    ));
    if let Ok(c) = cache().lock() {
        if let Some(v) = c.evidence.get(&key) {
            return v.clone();
        }
    }
    let meter = t
        .neural_rhythm
        .as_ref()
        .and_then(|r| r.beats_per_bar)
        .unwrap_or(4) as i64;
    let mut result = Vec::new();
    let last = event(d, t.duration_seconds - 1.).unwrap_or(0);
    for e in 0..last.min(30_000) {
        let beat = e - if d.beatgrid_uniform {
            0
        } else {
            d.beatgrid_anchor_index.unwrap_or(0) as i64
        };
        if beat.rem_euclid(meter) != 0 {
            continue;
        }
        let Some(seconds) = at(d, e) else { continue };
        if seconds < 0. {
            continue;
        }
        let mut contrast = 0.;
        let mut measured = 0;
        for width in [meter, meter * 2, meter * 4] {
            if let (Some(a), Some(b)) = (at(d, e - width), at(d, e + width)) {
                if a < 0. || b > t.duration_seconds {
                    continue;
                }
                if let (Some(x), Some(y)) = (bands(d, a, seconds), bands(d, seconds, b)) {
                    let delta = (x.0 - y.0).abs() + (x.1 - y.1).abs() + (x.2 - y.2).abs();
                    contrast += delta / (x.0 + x.1 + x.2 + y.0 + y.1 + y.2 + 1e-6);
                    measured += 1;
                }
            }
        }
        if measured > 0 {
            contrast /= measured as f64;
        }
        let cue = t
            .cue_points
            .iter()
            .find(|c| c.confirmed && (c.start_seconds - seconds).abs() < 0.08);
        let prior = beat.rem_euclid(meter * 8) == 0;
        if cue.is_some() || contrast > 0.14 || prior {
            result.push(BoundaryProposal {
                seconds,
                event: e,
                beat: beat as f64,
                confirmed: cue.is_some(),
                contrast,
                evidence: cue
                    .map(|c| format!("Confirmed cue: {}", c.name))
                    .unwrap_or_else(|| {
                        if contrast > 0.14 {
                            format!(
                                "Multiscale band-energy change {:.2}; inferred boundary",
                                contrast
                            )
                        } else {
                            "Eight-bar grid prior; phrase unconfirmed".into()
                        }
                    }),
            });
        }
    }
    if let Ok(mut c) = cache().lock() {
        if c.evidence.len() > 64 {
            c.evidence.clear();
        }
        c.evidence.insert(key, result.clone());
    }
    result
}
pub async fn options(
    app: tauri::AppHandle,
    request: OptionsRequest,
) -> Result<OptionsList, String> {
    let start = Instant::now();
    let stretch = request
        .max_stretch_percent
        .unwrap_or(DEFAULT_MAX_STRETCH_PERCENT);
    if !stretch.is_finite() || !(0. ..=100.).contains(&stretch) {
        return Err("max_stretch_percent must be finite and 0..100".into());
    }
    let limit = request.limit.unwrap_or(12);
    if !(1..=30).contains(&limit) {
        return Err("limit must be 1..30".into());
    }
    let valid = [
        "eq_blend",
        "filter_blend",
        "short_exchange",
        "echo_out",
        "reverb_exit",
    ];
    if request
        .techniques
        .as_ref()
        .is_some_and(|v| v.is_empty() || v.iter().any(|t| !valid.contains(&t.as_str())))
    {
        return Err(
            "Supported techniques: eq_blend, filter_blend, short_exchange, echo_out, reverb_exit"
                .into(),
        );
    }
    // Imported BPM tags cannot supply passage evidence or crowd analysed music out of the shortlist.
    let analysed_sources = {
        let state = app.state::<crate::AppState>();
        let library = state.library.lock().map_err(|_| crate::poisoned())?;
        let allowed = request
            .sources
            .as_ref()
            .map(|sources| library.normalize_suggestion_sources(sources))
            .transpose()?;
        library.with_features(|pool| {
            pool.values()
                .filter(|f| {
                    f.energy.is_some()
                        && (f.source.starts_with("spotify:")
                            || std::path::Path::new(&f.source).is_file())
                        && allowed
                            .as_ref()
                            .is_none_or(|sources| sources.contains(&f.source))
                })
                .map(|f| f.source.clone())
                .collect::<Vec<_>>()
        })?
    };
    let shortlist = suggest::suggest_next(
        app.clone(),
        request.deck,
        Some(request.energy_direction),
        Some(SuggestionOptions {
            provider: request.provider.clone(),
            playlist: request.playlist.clone(),
            sources: Some(analysed_sources),
            max_stretch_percent: Some(stretch),
            allow_half_double: Some(false),
            limit: Some(48),
        }),
    )
    .await?;
    tauri::async_runtime::spawn_blocking(move||{
        let state=app.state::<crate::AppState>();let snapshot=state.engine.scheduled_snapshot()?;
        let coverage=state.library.lock().map_err(|_|crate::poisoned())?.planner_coverage(request.playlist.as_deref(),request.sources.as_deref(),request.provider.as_deref())?;
        let mut list=OptionsList{version:PLANNER_VERSION,reference_deck:shortlist.deck,target_deck:shortlist.target_deck,reference_title:shortlist.reference.as_ref().map(|r|r.title.clone()),reference_position_seconds:0.,options:vec![],considered_tracks:0,coverage,capabilities:capabilities(),reasons:vec![],rejected_window_checks:HashMap::new(),candidate_rejections:HashMap::new(),elapsed_ms:0.};
        let Some(reference)=shortlist.deck else{list.reasons.push("Load a reference track to calculate next moves".into());return Ok(list)};
        if snapshot.decks[reference].track.as_ref().map(|t|t.path.as_str())!=shortlist.reference.as_ref().map(|r|r.source.as_str()) {return Err("Reference source changed while reading candidates; refresh suggestions".into());}
        list.reference_position_seconds=snapshot.decks[reference].position_seconds;
        if !snapshot.decks[reference].playing {list.reasons.push("Reference deck is paused; timings begin from its current source position. Arm after starting it.".into());}
        if snapshot.decks.iter().any(|d|d.loop_start_seconds.is_some()){list.reasons.push("Release active loops before generating linear transition timings".into());return Ok(list);}
        if snapshot.decks[1-reference].playing {list.reasons.push("Both decks are playing. Prepare a successor after the target deck is released.".into());}
        let target=1-reference;
        let played=state.play_history.lock().map_err(|_|crate::poisoned())?.played().clone();
        let mut candidates=Vec::new();
        for (reason, count) in [("already_played",shortlist.ranking.played),("requested_tempo_range",shortlist.ranking.out_of_range),("missing_tempo",shortlist.ranking.without_tempo),("analysis_not_cached",list.coverage.needs_analysis_members)] {
            if count > 0 {list.candidate_rejections.insert(reason.into(),count);}
        }
        if let Some(track)=snapshot.decks[target].track.as_ref(){
            let allowed={let lib=state.library.lock().map_err(|_|crate::poisoned())?;
                let sources=request.sources.as_ref().map(|s|lib.normalize_suggestion_sources(s)).transpose()?;
                let scope=request.playlist.as_ref().map(|p|lib.suggestion_scope(p)).transpose()?;
                sources.as_ref().is_none_or(|s|s.contains(&track.path)) && scope.as_ref().is_none_or(|(s,_)|s.contains(&track.path)) && request.provider.as_deref().is_none_or(|p|(p=="spotify")==track.path.starts_with("spotify:"))};
            if allowed {
                if eligible_successor(snapshot.decks[reference].track.as_ref(),track,&played){candidates.push(track.clone());}
                else {*list.candidate_rejections.entry(if played.contains(&track.path){"already_played"}else{"same_recording"}.into()).or_default()+=1;}
            }
        }
        for s in shortlist.ranking.suggestions.iter().take(24) {
            let analysis={let lib=state.library.lock().map_err(|_|crate::poisoned())?;if s.spotify{lib.spotify_analysis(&s.source)?}else{lib.track_by_path(&s.source)?.map(crate::record_analysis)}};
            if let Some(a)=analysis {if eligible_successor(snapshot.decks[reference].track.as_ref(),&a,&played) && !candidates.iter().any(|t|same_recording(t,&a)){candidates.push(a);}}
        }
        list.considered_tracks=candidates.len();
        for track in candidates {
            let next=if snapshot.decks[target].track.as_ref().is_some_and(|t|t.path==track.path){snapshot.decks[target].clone()}else{match analysis_deck(&track){Ok(d)=>d,Err(_)=>{*list.candidate_rejections.entry("missing_grid".into()).or_default()+=1;continue}}};
            let before=list.rejected_window_checks.values().sum::<usize>();
            let generated=generate_with_reasons(&snapshot,reference,&next,request.energy_direction,stretch,request.techniques.as_deref(),&mut list.rejected_window_checks);
            if generated.is_empty() && before==list.rejected_window_checks.values().sum::<usize>() {*list.rejected_window_checks.entry("no_passage_windows".into()).or_default()+=1;}
            list.options.extend(generated);
        }
        list.options.sort_by(|a,b|b.score.total_cmp(&a.score).then_with(||a.id.cmp(&b.id)));
        list.options=diversify(&list.options,limit);
        if list.options.is_empty(){
            list.reasons.extend(empty_options_reasons(list.considered_tracks,&list.candidate_rejections,&list.rejected_window_checks));
        }
        if let Ok(mut c)=cache().lock(){if c.options.len()>512 {c.options.clear();}for o in &list.options{c.options.insert(o.id.clone(),o.clone());}}
        list.elapsed_ms=start.elapsed().as_secs_f64()*1000.;Ok(list)
    }).await.map_err(|_|"Planner worker failed".to_string())?
}

fn empty_options_reasons(
    considered: usize,
    candidates: &HashMap<String, usize>,
    windows: &HashMap<String, usize>,
) -> Vec<String> {
    let mut reasons = vec![if considered == 0 {
        "No eligible successor candidates reached passage planning".into()
    } else {
        "No supported transition window fits the current sources and requested settings".into()
    }];
    for (label, counts) in [
        ("Candidate exclusions", candidates),
        ("Rejected window checks", windows),
    ] {
        let mut checks: Vec<_> = counts.iter().collect();
        checks.sort_by(|(a, na), (b, nb)| nb.cmp(na).then_with(|| a.cmp(b)));
        if !checks.is_empty() {
            reasons.push(format!(
                "{label}: {}",
                checks
                    .iter()
                    .map(|(reason, count)| format!("{} ({count})", reason.replace('_', " ")))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    reasons
}

fn diversify(options: &[MoveOption], limit: usize) -> Vec<MoveOption> {
    let mut selected: Vec<MoveOption> = Vec::new();
    for technique in [
        "eq_blend",
        "filter_blend",
        "short_exchange",
        "echo_out",
        "reverb_exit",
    ] {
        if selected.len() == limit {
            break;
        }
        if let Some(o) = options
            .iter()
            .find(|o| o.technique == technique && !selected.iter().any(|s| s.source == o.source))
            .or_else(|| options.iter().find(|o| o.technique == technique))
        {
            selected.push(o.clone());
        }
    }
    for pass in 0..3 {
        for o in options {
            if selected.len() == limit {
                break;
            }
            let count = selected.iter().filter(|s| s.source == o.source).count();
            if count != pass || selected.iter().any(|s| s.id == o.id) {
                continue;
            }
            if selected
                .iter()
                .any(|s| s.source == o.source && s.technique == o.technique)
            {
                continue;
            }
            selected.push(o.clone());
        }
    }
    selected
}

#[cfg(test)]
pub(crate) fn generate(
    snapshot: &EngineSnapshot,
    reference: usize,
    next: &DeckSnapshot,
    direction: EnergyDirection,
    stretch: f64,
    techniques: Option<&[String]>,
) -> Vec<MoveOption> {
    generate_with_reasons(
        snapshot,
        reference,
        next,
        direction,
        stretch,
        techniques,
        &mut HashMap::new(),
    )
}

fn generate_with_reasons(
    snapshot: &EngineSnapshot,
    reference: usize,
    next: &DeckSnapshot,
    direction: EnergyDirection,
    stretch: f64,
    techniques: Option<&[String]>,
    rejected: &mut HashMap<String, usize>,
) -> Vec<MoveOption> {
    generate_windows(
        snapshot,
        reference,
        next,
        direction,
        stretch,
        WindowSelection {
            techniques,
            original: None,
            fallback: false,
        },
        rejected,
    )
}

struct WindowSelection<'a> {
    techniques: Option<&'a [String]>,
    original: Option<&'a MoveOption>,
    fallback: bool,
}

/// Reserve a provider load, one recovery attempt, decision time, and the next short overlap.
fn continuation_budget(bpm: f64) -> (f64, f64) {
    let preparation = 60.;
    (preparation, preparation + 30. + 15. + 20. * 60. / bpm)
}

fn generate_windows(
    snapshot: &EngineSnapshot,
    reference: usize,
    next: &DeckSnapshot,
    direction: EnergyDirection,
    stretch: f64,
    selection: WindowSelection<'_>,
    rejected: &mut HashMap<String, usize>,
) -> Vec<MoveOption> {
    let WindowSelection {
        techniques,
        original,
        fallback,
    } = selection;
    let mut reject = |reason: &str| {
        *rejected.entry(reason.into()).or_default() += 1;
    };
    let outgoing = &snapshot.decks[reference];
    let Some(a) = outgoing.track.as_ref() else {
        reject("missing_reference");
        return vec![];
    };
    let Some(b) = next.track.as_ref() else {
        reject("missing_incoming");
        return vec![];
    };
    if !outgoing.beatgrid_valid || !next.beatgrid_valid {
        reject("missing_grid");
        return vec![];
    }
    if outgoing.jog_touch
        || outgoing.sync_enabled
        || (outgoing.actual_playback_rate - outgoing.playback_rate).abs() > 0.0005
    {
        reject("moving_transport");
        return vec![];
    }
    let bar_origin = |d: &DeckSnapshot, t: &TrackAnalysis| {
        d.beatgrid_manual
            || d.beatgrid_downbeat
            || t.neural_rhythm
                .as_ref()
                .is_some_and(|r| !r.downbeats_seconds.is_empty())
    };
    if !bar_origin(outgoing, a) || !bar_origin(next, b) {
        reject("missing_bar_origin");
        return vec![];
    }
    let meter = a
        .neural_rhythm
        .as_ref()
        .and_then(|r| r.beats_per_bar)
        .unwrap_or(4);
    if meter
        != b.neural_rhythm
            .as_ref()
            .and_then(|r| r.beats_per_bar)
            .unwrap_or(4)
        || meter != 4
    {
        reject("unsupported_meter");
        return vec![];
    }
    let rate = outgoing.actual_playback_rate;
    if !rate.is_finite() || rate <= 0. {
        reject("invalid_playback_rate");
        return vec![];
    }
    let playing = outgoing.actual_playback_bpm.or(outgoing.playback_bpm);
    if playing.is_none() {
        reject("missing_playback_tempo");
        return vec![];
    }
    let Some(native_a) = a.bpm else {
        reject("missing_reference_tempo");
        return vec![];
    };
    let Some(native_b) = b.bpm else {
        reject("missing_incoming_tempo");
        return vec![];
    };
    if (rate - 1.).abs() * 100. > stretch + 0.05 {
        reject("tempo_correction_limit");
        return vec![];
    }
    let target = 1 - reference;
    let already_loaded = snapshot.decks[target]
        .track
        .as_ref()
        .is_some_and(|t| t.path == b.path);
    // Conservative visible initial budgets; measured provider latency can replace these defaults.
    let budget = if original.is_some() {
        0.
    } else if already_loaded {
        8.
    } else if b.path.starts_with("spotify:") {
        60.
    } else {
        25.
    };
    let incoming_boundaries = boundaries(next);
    let outgoing_boundaries = boundaries(outgoing);
    if incoming_boundaries.is_empty() {
        reject("no_incoming_boundaries");
    }
    if outgoing_boundaries.is_empty() {
        reject("no_outgoing_boundaries");
    }
    let current = outgoing.position_seconds;
    let mut result = Vec::new();
    for technique in [
        "eq_blend",
        "filter_blend",
        "short_exchange",
        "echo_out",
        "reverb_exit",
    ] {
        if techniques.is_some_and(|v| !v.iter().any(|t| t == technique)) {
            continue;
        }
        let reserved_end = useful_end_seconds(outgoing);
        let mut exits: Vec<_> = outgoing_boundaries
            .iter()
            .filter(|x| {
                x.seconds > current + (budget + 4.) * rate
                    && x.seconds < a.duration_seconds - 4. * 60. / native_a
                    && at(outgoing, x.event + 2).is_some_and(|clear| clear <= reserved_end)
                    && original.is_none_or(|old| fallback || x.event == old.clear_event)
            })
            .collect();
        if exits.is_empty() {
            reject("no_future_exit_with_preparation_runway");
        }
        exits.sort_by(|x, y| {
            let fit = |v: &BoundaryProposal| {
                v.contrast + if v.confirmed { 1. } else { 0. }
                    - ((a.duration_seconds - 32. - v.seconds).abs() / a.duration_seconds) * 0.35
            };
            fit(y).total_cmp(&fit(x))
        });
        if fallback {
            exits.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
        }
        for exit in exits
            .into_iter()
            .take(if original.is_some() { usize::MAX } else { 12 })
        {
            let mut starts = outgoing_boundaries
                .iter()
                .filter(|start| {
                    let beats = exit.event - start.event;
                    let max = if technique == "short_exchange" {
                        16
                    } else {
                        128
                    };
                    (16..=max).contains(&beats)
                        && beats % meter as i64 == 0
                        && start.seconds > current + (budget + 4.) * rate
                        && original.is_none_or(|old| fallback || start.event == old.launch_event)
                })
                .collect::<Vec<_>>();
            starts.sort_by(|a, b| {
                (b.contrast + if b.confirmed { 1. } else { 0. })
                    .total_cmp(&(a.contrast + if a.confirmed { 1. } else { 0. }))
            });
            let mut launches = starts
                .into_iter()
                .take(4)
                .map(|start| start.event)
                .collect::<Vec<_>>();
            if technique == "short_exchange" && launches.is_empty() {
                launches.push(exit.event - 16);
            }
            for launch_event in launches {
                let beats = exit.event - launch_event;
                let bars = beats as usize / meter;

                let Some(launch) = at(outgoing, launch_event) else {
                    continue;
                };
                if (launch - current) / rate < budget + 4. {
                    reject("preparation_deadline");
                    continue;
                }
                let Some(audible) = at(outgoing, launch_event + 2) else {
                    continue;
                };
                let Some(pa) = passage(outgoing, launch, exit.seconds) else {
                    continue;
                };
                if pa.pulse_spread > 0.025 || pa.level < 0.008 {
                    reject("outgoing_pulse_or_silence");
                    continue;
                }
                let mut entries: Vec<_> = incoming_boundaries
                    .iter()
                    .filter(|x| x.seconds < 120. && x.seconds >= 0.)
                    .collect();
                entries.sort_by(|x, y| {
                    (y.contrast + if y.confirmed { 1. } else { 0. })
                        .total_cmp(&(x.contrast + if x.confirmed { 1. } else { 0. }))
                        .then_with(|| x.seconds.total_cmp(&y.seconds))
                });
                if let Some(old) = original {
                    entries.retain(|entry| (entry.seconds - old.entry_seconds).abs() < 0.05);
                }
                for entry in entries.into_iter().take(4) {
                    let arrival = incoming_boundaries
                        .iter()
                        .filter(|point| {
                            let offset = point.event - entry.event;
                            offset >= beats / 4
                                && offset <= beats * 3 / 4
                                && (point.confirmed || point.contrast > 0.14)
                        })
                        .max_by(|a, b| a.contrast.total_cmp(&b.contrast));
                    let handover_offset =
                        arrival.map_or(beats / 2, |point| point.event - entry.event);
                    let handover_event = launch_event + handover_offset;
                    let Some(handover) = at(outgoing, handover_event) else {
                        continue;
                    };
                    let Some(end) = at(next, entry.event + beats + meter as i64 * 4) else {
                        continue;
                    };
                    if end > b.duration_seconds - 2. {
                        reject("insufficient_incoming_continuation");
                        continue;
                    }
                    let Some(pb) = passage(next, entry.seconds, end) else {
                        continue;
                    };
                    if pb.pulse_spread > 0.025 || pb.level < 0.008 {
                        reject("incoming_pulse_or_silence");
                        continue;
                    }
                    let Some(entry_end) = at(next, entry.event + 4) else {
                        continue;
                    };
                    let entry_bpm = 240. / (entry_end - entry.seconds);
                    // Use the selected future passage, not the playhead's jittering four-beat BPM.
                    let playing = original.map_or(pa.local_bpm * rate, |old| old.target_bpm);
                    let percent = (playing / entry_bpm - 1.) * 100.;
                    if percent.abs() > stretch
                        || ((playing / pa.local_bpm) - 1.).abs() * 100. > stretch
                    {
                        reject("tempo_correction_limit");
                        continue;
                    }
                    // Both robust tails must fit the rate envelope, not just the medians.
                    if percent.abs() + pb.pulse_spread * 50. > stretch + 0.15 {
                        reject("rate_envelope");
                        continue;
                    }
                    // Fixed-rate recipes must preserve phase throughout the overlap, not only at launch.
                    let incoming_rate = playing / entry_bpm;
                    let Some(verified_clear) = at(outgoing, exit.event + 2) else {
                        continue;
                    };
                    if verified_clear > useful_end_seconds(outgoing) {
                        reject("outgoing_audible_end");
                        continue;
                    }
                    if b.audio_measurements
                        .as_ref()
                        .and_then(|m| m.audible_start_seconds())
                        .is_some_and(|start| entry.seconds < start)
                    {
                        reject("incoming_audible_start");
                        continue;
                    }
                    let continuation = (useful_end_seconds(next) - entry.seconds) / incoming_rate
                        - (verified_clear - launch) / rate;
                    let (next_preparation, minimum_continuation) = continuation_budget(playing);
                    let mut max_drift = 0.0_f64;
                    for k in 1..=beats {
                        let Some(ta) = at(outgoing, launch_event + k) else {
                            max_drift = f64::INFINITY;
                            break;
                        };
                        let Some(tb) = at(next, entry.event + k) else {
                            max_drift = f64::INFINITY;
                            break;
                        };
                        let drift = ((ta - launch) / rate - (tb - entry.seconds) / incoming_rate)
                            .abs()
                            * playing
                            / 60.;
                        max_drift = max_drift.max(drift);
                    }
                    if max_drift > 0.20 {
                        reject("fixed_rate_phase_drift");
                        continue;
                    }
                    let pitch_preserved = (outgoing.keylock || (rate - 1.).abs() < 0.0005)
                        && outgoing.pitch_semitones.abs() < 0.01;
                    let key = KeyRelation::between(
                        if pitch_preserved {
                            a.camelot.as_deref().and_then(Camelot::parse)
                        } else {
                            None
                        },
                        b.camelot.as_deref().and_then(Camelot::parse),
                    );
                    let tempo_fit = 1. - percent.abs() / stretch.max(0.1);
                    let delta = ((pb.level / pa.level).log10() / 2.).clamp(-1., 1.) as f32;
                    let energy = suggest::energy_fit(delta, direction) as f64;
                    let spectral = 1. - ((pa.mid - pb.mid).abs() / (pa.mid + pb.mid + 1e-6));
                    let start_boundary = outgoing_boundaries
                        .iter()
                        .find(|point| point.event == launch_event);
                    let boundary = ((exit.contrast
                        + entry.contrast
                        + start_boundary.map_or(0., |point| point.contrast)
                        + arrival.map_or(0., |point| point.contrast))
                        / 4.)
                        .min(1.);
                    let key_fit =
                        suggest::key_score(key, a.key_confidence, b.key_confidence) as f64;
                    let score = 0.25 * tempo_fit
                        + 0.2 * energy
                        + 0.15 * spectral
                        + 0.2 * key_fit
                        + 0.2 * boundary;
                    let mut warnings=vec!["Vocal activity and foreground phrases are unknown; waveform evidence does not identify a completed lyric or a suitable vocal overlap".into()];
                    if key == KeyRelation::Clash {
                        warnings.push("Estimated keys differ; choose the tonal overlap and transition style by musical judgment".into());
                    }
                    if continuation < minimum_continuation {
                        warnings.push(format!("Short continuation: {continuation:.1}s remains after clearance; prepare the following move promptly"));
                    }
                    if !exit.confirmed || !entry.confirmed {
                        warnings.push("Phrase boundaries are inferred from energy change/grid priors, not verified arrangement labels".into());
                    }
                    if !outgoing.keylock || !next.keylock {
                        warnings.push("Unlocked tempo changes alter playing pitch; key lock is available when preserving pitch suits the plan".into());
                    }
                    if outgoing.pitch_semitones.abs() > 0.01 {
                        warnings.push("Outgoing pitch is transposed; native key labels are excluded from tonal ranking".into());
                    }
                    let review_required = false;
                    let arrival_evidence = arrival.map_or(
                        "Overlap midpoint; musical arrival unconfirmed".to_string(),
                        |point| point.evidence.clone(),
                    );
                    let mut evidence=vec![exit.evidence.clone(),entry.evidence.clone(),format!("Local pulse spread {:.1}% / {:.1}%; maximum fixed-rate drift {max_drift:.2} beat",pa.pulse_spread*100.,pb.pulse_spread*100.),format!("{native_a:.1} → {native_b:.1} native BPM; {playing:.1} playing BPM; {percent:+.2}% incoming"),format!("{bars} bars of overlap; {} source beats counted on the effective grid",beats)];
                    evidence.push(format!("Handover after {handover_offset} beats: {arrival_evidence}; bounded native Sync follows the outgoing pulse"));
                    if let Some(start) = start_boundary {
                        evidence.push(format!("Outgoing entrance: {}", start.evidence));
                    }
                    if let Some(m) = &b.audio_measurements {
                        if let Some(lufs) = m.integrated_lufs {
                            evidence.push(format!("Measured {lufs:.1} LUFS; normalization {:+.1} dB toward -18 LUFS with -1 dBTP source cap",m.normalization_gain_db));
                        }
                        evidence.push("Runway uses full PCM audible bounds at -60 dBFS".into());
                    } else {
                        warnings.push("PCM loudness and precise bounds unavailable until this source is loaded; runway uses waveform evidence".into());
                    }
                    let id = identity((
                        PLANNER_VERSION,
                        &a.path,
                        &b.path,
                        &b.source_fingerprint,
                        reference,
                        snapshot.load_revisions,
                        snapshot.decks[reference].grid_revision,
                        snapshot.decks[reference].position_revision,
                        snapshot.deck_control_revisions[reference],
                        technique,
                        launch_event,
                        exit.event,
                        entry.event,
                        playing,
                    ));
                    result.push(MoveOption {
                        id,
                        source: b.path.clone(),
                        title: b.title.clone(),
                        artist: b.artist.clone().unwrap_or_default(),
                        spotify: b.path.starts_with("spotify:"),
                        reference_deck: reference,
                        target_deck: target,
                        technique: technique.into(),
                        overlap_bars: bars,
                        entry_seconds: entry.seconds,
                        launch_seconds: launch,
                        audible_seconds: audible,
                        handover_seconds: handover,
                        clear_seconds: exit.seconds,
                        launch_event,
                        handover_event,
                        clear_event: exit.event,
                        target_bpm: playing,
                        native_bpm: pb.local_bpm,
                        tempo_percent: percent,
                        max_stretch_percent: stretch,
                        reference_source: a.path.clone(),
                        reference_load_revision: snapshot.load_revisions[reference],
                        reference_grid_revision: outgoing.grid_revision,
                        reference_position_revision: outgoing.position_revision,
                        reference_control_revision: snapshot.deck_control_revisions[reference],
                        engine_epoch: snapshot.engine_epoch,
                        review_required,
                        evidence,
                        warnings,
                        score,
                        score_components: HashMap::from([
                            ("tempo_preservation".into(), tempo_fit),
                            ("energy_direction".into(), energy),
                            ("spectral_relation".into(), spectral),
                            ("tonal_estimate".into(), key_fit),
                            ("boundary_contrast".into(), boundary),
                        ]),
                        outgoing: pa.clone(),
                        incoming: pb,
                        launch_in_seconds: (launch - current) / rate,
                        prepare_within_seconds: (launch - current) / rate - budget,
                        preparation_budget_seconds: budget,
                        continuation_seconds: continuation,
                        minimum_continuation_seconds: minimum_continuation,
                        next_preparation_budget_seconds: next_preparation,
                        next_decision_within_seconds: (continuation - minimum_continuation + 15.)
                            .max(0.),
                        readiness: if snapshot.decks[target].playing {
                            "target_playing"
                        } else if snapshot.decks[target].track.is_some()
                            && snapshot.decks[target].channel_fader > 0.001
                        {
                            "target_requires_isolation"
                        } else if already_loaded {
                            "source_loaded"
                        } else {
                            "suggested"
                        }
                        .into(),
                        incoming_fingerprint: b.source_fingerprint.clone(),
                    });
                }
            }
        }
    }
    result
}
pub(crate) fn resolve_option(app: &tauri::AppHandle, id: &str) -> Result<MoveOption, String> {
    let option = cache()
        .lock()
        .map_err(|_| crate::poisoned())?
        .options
        .get(id)
        .cloned()
        .ok_or("Option expired; refresh suggestions")?;
    let state = app.state::<crate::AppState>();
    let s = state.engine.snapshot();
    let d = &s.decks[option.reference_deck];
    if s.engine_epoch != option.engine_epoch
        || s.load_revisions[option.reference_deck] != option.reference_load_revision
        || d.grid_revision != option.reference_grid_revision
        || d.position_revision != option.reference_position_revision
        || s.deck_control_revisions[option.reference_deck] != option.reference_control_revision
    {
        return Err("Option invalidated by a source, grid, seek, tempo or mixer change; refresh suggestions".into());
    }
    if d.position_seconds >= option.launch_seconds - 2. * d.actual_playback_rate {
        return Err("Transition window expired; refresh suggestions".into());
    }
    Ok(option)
}
pub(crate) fn refresh_prepared_option(
    app: &tauri::AppHandle,
    old: &MoveOption,
) -> Result<MoveOption, String> {
    let state = app.state::<crate::AppState>();
    let s = state.engine.scheduled_snapshot()?;
    let incoming = s.decks[old.target_deck]
        .track
        .as_ref()
        .ok_or("Incoming source not loaded")?;
    if incoming.path != old.source
        || old.incoming_fingerprint.is_some()
            && incoming.source_fingerprint != old.incoming_fingerprint
    {
        return Err("Incoming content identity changed; regenerate options".into());
    }
    revalidate_prepared_window(&s, old, false)
}

pub(crate) fn recovery_option(s: &EngineSnapshot, old: &MoveOption) -> Result<MoveOption, String> {
    for technique in [&old.technique, &"short_exchange".to_string()] {
        let mut options = generate_windows(
            s,
            old.reference_deck,
            &s.decks[old.target_deck],
            EnergyDirection::Keep,
            old.max_stretch_percent,
            WindowSelection {
                techniques: Some(std::slice::from_ref(technique)),
                original: None,
                fallback: true,
            },
            &mut HashMap::new(),
        );
        options.sort_by(|a, b| {
            a.launch_seconds
                .total_cmp(&b.launch_seconds)
                .then_with(|| b.score.total_cmp(&a.score))
        });
        if let Some(option) = options.into_iter().next() {
            return Ok(option);
        }
    }
    Err("No future passage remains for a safe retry of the prepared source".into())
}

pub(crate) fn revalidate_prepared_window(
    s: &EngineSnapshot,
    old: &MoveOption,
    fallback: bool,
) -> Result<MoveOption, String> {
    let next = &s.decks[old.target_deck];
    if next.track.as_ref().is_none_or(|t| {
        t.path != old.source
            || old.incoming_fingerprint.is_some()
                && t.source_fingerprint != old.incoming_fingerprint
    }) {
        return Err("Incoming content identity changed; regenerate options".into());
    }
    let techniques = [if fallback {
        "short_exchange".into()
    } else {
        old.technique.clone()
    }];
    let mut options = generate_windows(
        s,
        old.reference_deck,
        next,
        EnergyDirection::Keep,
        old.max_stretch_percent,
        WindowSelection {
            techniques: Some(&techniques),
            original: Some(old),
            fallback,
        },
        &mut HashMap::new(),
    );
    if fallback {
        options.sort_by(|a, b| a.launch_seconds.total_cmp(&b.launch_seconds));
    }
    options
        .into_iter()
        .find(|o| fallback || o.launch_event == old.launch_event)
        .ok_or_else(|| {
            "Selected entry has no feasible window with preparation and next-transition runway"
                .into()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_options_report_actual_exclusions_instead_of_guessing_missing_analysis() {
        let candidates = HashMap::from([("already_played".into(), 1)]);
        let reasons = empty_options_reasons(0, &candidates, &HashMap::new()).join("; ");
        assert!(reasons.contains("No eligible successor candidates"));
        assert!(reasons.contains("already played (1)"));
        assert!(!reasons.contains("analysis"));
        let windows = HashMap::from([("no_future_exit_with_preparation_runway".into(), 5)]);
        let reasons = empty_options_reasons(1, &HashMap::new(), &windows).join("; ");
        assert!(reasons.contains("No supported transition window"));
        assert!(reasons.contains("no future exit with preparation runway (5)"));
        assert!(!reasons.contains("unanalysed"));
    }
    fn track() -> TrackAnalysis {
        TrackAnalysis {
            path: "/test/a.wav".into(),
            title: "A".into(),
            duration_seconds: 300.,
            bpm: Some(120.),
            bpm_source: Some("manual".into()),
            beatgrid_source: Some("manual".into()),
            waveform_bands: Some(audio_analysis::WaveformBands {
                low: vec![0.12; 4096],
                mid: vec![0.1; 4096],
                high: vec![0.04; 4096],
            }),
            ..Default::default()
        }
    }
    #[test]
    fn inferred_boundaries_never_write_or_claim_confirmed_cues() {
        let t = track();
        let d = analysis_deck(&t).unwrap();
        let b = boundaries(&d);
        assert!(!b.is_empty());
        assert!(b.iter().all(|b| !b.confirmed));
        assert!(t.cue_points.is_empty());
    }
    #[test]
    fn variable_grid_timing_uses_actual_intervals() {
        let mut d = analysis_deck(&track()).unwrap();
        d.beatgrid_uniform = false;
        let mut t = track();
        let mut r:audio_analysis::NeuralRhythm=serde_json::from_value(serde_json::json!({"model":"test","bpm":120.,"beats_seconds":(0..600).map(|i|i as f64*0.505).collect::<Vec<_>>(),"downbeats_seconds":[0.],"beat_confidence":0.8,"downbeat_confidence":0.8,"analysis_seconds":1.,"beats_per_bar":4})).unwrap();
        r.bpm = 120.;
        t.neural_rhythm = Some(r);
        d.track = Some(t);
        assert!((at(&d, 64).unwrap() - 32.32).abs() < 1e-6);
    }
    #[test]
    fn passage_rejects_silent_material() {
        let d = analysis_deck(&track()).unwrap();
        let p = passage(&d, 0., 32.).unwrap();
        assert_eq!(p.local_bpm, 120.);
        assert_eq!(p.pulse_spread, 0.);
        assert!(p.level > 0.);
    }
    fn pair(bpm: f64) -> (EngineSnapshot, DeckSnapshot) {
        let mut a = analysis_deck(&track()).unwrap();
        a.playing = true;
        a.channel_fader = 1.;
        a.actual_playback_bpm = Some(120.);
        a.playback_bpm = Some(120.);
        let mut t = track();
        t.path = "/test/b.wav".into();
        t.bpm = Some(bpm);
        let b = analysis_deck(&t).unwrap();
        let s=serde_json::from_value(serde_json::json!({"decks":[a,b],"load_revisions":[1,2],"sample_rate":48000,"output_channels":2,"buffer_frames":512,"running":true,"rendered_frames":0,"master_peak":[0.,0.],"cue_peak":[0.,0.],"underruns":0,"master_gain":1.,"crossfader":0.,"cue_gain":1.,"cue_mix":1.,"sampler_gain":1.,"sampler_positions_seconds":vec![serde_json::Value::Null;16],"active_ramps":vec![false;deck_engine::CONTROL_TARGET_COUNT]})).unwrap();
        (s, b)
    }
    #[test]
    fn generates_three_real_recipes_with_separate_launch_audibility_and_clearance() {
        let (s, b) = pair(121.);
        let options = generate(&s, 0, &b, EnergyDirection::Keep, 2., None);
        for technique in [
            "eq_blend",
            "filter_blend",
            "short_exchange",
            "echo_out",
            "reverb_exit",
        ] {
            assert!(
                options.iter().any(|o| o.technique == technique),
                "{technique}"
            );
        }
        assert!(options.iter().all(|o| o.launch_seconds < o.audible_seconds
            && o.audible_seconds < o.handover_seconds
            && o.handover_seconds < o.clear_seconds
            && o.tempo_percent.abs() < 2.
            && o.clear_seconds < 296.
            && !o.review_required));
    }
    #[test]
    fn overlap_length_follows_source_boundaries_instead_of_a_technique_preset() {
        let (mut s, b) = pair(120.);
        s.decks[0].track.as_mut().unwrap().cue_points = vec![
            audio_analysis::CuePoint::manual("start".into(), 52., "Outgoing blend entry".into()),
            audio_analysis::CuePoint::manual("clear".into(), 72., "Outgoing statement ends".into()),
        ];
        let options = generate(
            &s,
            0,
            &b,
            EnergyDirection::Keep,
            2.,
            Some(&["filter_blend".into()]),
        );
        assert!(options
            .iter()
            .any(|o| o.launch_seconds == 52. && o.clear_seconds == 72. && o.overlap_bars == 10));
    }
    #[test]
    fn wide_tempo_and_expired_windows_abstain_instead_of_forcing_a_match() {
        let (s, b) = pair(130.);
        assert!(generate(&s, 0, &b, EnergyDirection::Keep, 2., None).is_empty());
        assert!(generate(
            &s,
            0,
            &b,
            EnergyDirection::Keep,
            DEFAULT_MAX_STRETCH_PERCENT,
            None
        )
        .is_empty());
        assert!(!generate(&s, 0, &b, EnergyDirection::Keep, 100., None).is_empty());
        let (s, b) = pair(122.);
        assert!(!generate(
            &s,
            0,
            &b,
            EnergyDirection::Keep,
            DEFAULT_MAX_STRETCH_PERCENT,
            None
        )
        .is_empty());
        let (mut s, b) = pair(121.);
        s.decks[0].position_seconds = 290.;
        assert!(generate(&s, 0, &b, EnergyDirection::Keep, 2., None).is_empty());
    }
    #[test]
    fn silent_candidate_and_missing_content_evidence_are_not_ranked_as_blends() {
        let (s, mut b) = pair(121.);
        b.track.as_mut().unwrap().waveform_bands = None;
        assert!(generate(&s, 0, &b, EnergyDirection::Keep, 2., None).is_empty());
        b.track.as_mut().unwrap().waveform_bands = Some(audio_analysis::WaveformBands {
            low: vec![0.; 4096],
            mid: vec![0.; 4096],
            high: vec![0.; 4096],
        });
        assert!(generate(&s, 0, &b, EnergyDirection::Keep, 2., None).is_empty());
    }
    #[test]
    fn estimated_key_clash_changes_ranking_and_warnings_without_banning_long_blends() {
        let (mut s, mut b) = pair(121.);
        s.decks[0].track.as_mut().unwrap().camelot = Some("8A".into());
        s.decks[0].keylock = false;
        b.track.as_mut().unwrap().camelot = Some("3B".into());
        let options = generate(&s, 0, &b, EnergyDirection::Keep, 2., None);
        assert!(!options.is_empty());
        assert!(options.iter().any(|o| o.overlap_bars >= 8));
        assert!(options.iter().all(|o| o
            .warnings
            .iter()
            .any(|w| w.starts_with("Estimated keys differ"))));
    }
    #[test]
    fn duplicate_files_of_the_same_recording_share_content_identity() {
        let mut a = track();
        a.source_fingerprint = Some("decoded-content-a".into());
        let mut b = a.clone();
        b.path = "/another/copy.mp3".into();
        assert!(same_recording(&a, &b));
        b.source_fingerprint = Some("decoded-content-b".into());
        assert!(!same_recording(&a, &b));
    }
    #[test]
    fn a_preloaded_previous_carrier_cannot_bypass_session_history() {
        let (s, b) = pair(121.);
        let reference = s.decks[0].track.as_ref();
        let candidate = b.track.as_ref().unwrap();
        let mut played = HashSet::new();
        assert!(eligible_successor(reference, candidate, &played));
        played.insert(candidate.path.clone());
        assert!(!eligible_successor(reference, candidate, &played));
        played.clear();
        assert!(eligible_successor(reference, candidate, &played));
        assert!(!eligible_successor(reference, reference.unwrap(), &played));
    }
    #[test]
    fn precise_pcm_bounds_override_coarse_waveform_runway() {
        let (mut s, _) = pair(121.);
        let d = &mut s.decks[0];
        let t = d.track.as_mut().unwrap();
        let rate = t.sample_rate.max(48000);
        t.sample_rate = rate;
        let mut m = audio_analysis::measure_audio(&[], rate);
        m.frames = (t.duration_seconds * rate as f64).round() as u64;
        m.audible_start_frame = Some(10);
        m.audible_end_frame = Some(123456);
        t.audio_measurements = Some(m);
        assert_eq!(useful_end_seconds(d), 123456. / rate as f64);
        d.track
            .as_mut()
            .unwrap()
            .audio_measurements
            .as_mut()
            .unwrap()
            .audible_end_frame = None;
        assert_eq!(useful_end_seconds(d), 0.);
        d.track
            .as_mut()
            .unwrap()
            .audio_measurements
            .as_mut()
            .unwrap()
            .version = 0;
        assert!(useful_end_seconds(d) > 0.);
    }
    #[test]
    fn short_continuation_is_advisory_instead_of_excluding_the_song() {
        let (s, mut b) = pair(121.);
        b.track.as_mut().unwrap().duration_seconds = 110.;
        let mut rejected = HashMap::new();
        let options =
            generate_with_reasons(&s, 0, &b, EnergyDirection::Keep, 2., None, &mut rejected);
        assert!(!options.is_empty());
        assert!(options
            .iter()
            .any(|o| o.continuation_seconds < o.minimum_continuation_seconds
                && o.warnings
                    .iter()
                    .any(|w| w.starts_with("Short continuation"))));
        assert!(!rejected.contains_key("next_transition_runway"));
        assert!(options.iter().all(|o| o.next_decision_within_seconds >= 0.));
    }

    #[test]
    fn revalidation_keeps_the_selected_future_tempo_despite_playhead_jitter() {
        let (mut s, b) = pair(121.);
        let original = generate(&s, 0, &b, EnergyDirection::Keep, 2., None).remove(0);
        s.decks[0].actual_playback_bpm = Some(118.8);
        s.decks[0].position_seconds = 2.;
        let refreshed = revalidate_prepared_window(&s, &original, false).unwrap();
        assert_eq!(refreshed.target_bpm, original.target_bpm);
        assert_eq!(refreshed.clear_event, original.clear_event);
        assert_eq!(refreshed.launch_event, original.launch_event);
        assert!(refreshed.continuation_seconds > 0.);
        s.decks[0].position_seconds = original.launch_seconds + 0.1;
        assert!(revalidate_prepared_window(&s, &original, false).is_err());
        let fallback = revalidate_prepared_window(&s, &original, true).unwrap();
        assert_eq!(fallback.source, original.source);
        assert_eq!(fallback.entry_seconds, original.entry_seconds);
        assert_eq!(fallback.technique, "short_exchange");
        assert!(fallback.launch_seconds > s.decks[0].position_seconds + 4.);
        assert!(fallback.continuation_seconds > 0.);
    }

    #[test]
    #[ignore = "Requires unchanged actual local-song snapshots; no audio hardware"]
    fn actual_local_library_runway_and_late_preparation_fallback() {
        let path = std::env::var("PULSEDECK_REAL_MOVE_INPUT").unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let mut s: EngineSnapshot = serde_json::from_value(value["snapshot"].clone()).unwrap();
        let original: MoveOption = serde_json::from_value(value["option"].clone()).unwrap();
        let a = original.reference_deck;
        let b = original.target_deck;
        for d in &s.decks {
            assert!(std::path::Path::new(&d.track.as_ref().unwrap().path).is_file());
        }
        let reference = s.decks[a].track.as_ref();
        let candidate = s.decks[b].track.as_ref().unwrap();
        assert!(eligible_successor(reference, candidate, &HashSet::new()));
        assert!(!eligible_successor(
            reference,
            candidate,
            &HashSet::from([candidate.path.clone()]),
        ));
        let options = generate(&s, a, &s.decks[b], EnergyDirection::Keep, 2., None);
        assert!(!options.is_empty());
        assert!(options.iter().all(|o| o.continuation_seconds > 0.));
        if let Ok(path) = std::env::var("PULSEDECK_REAL_OPTIONS_EXPORT") {
            std::fs::write(
                path,
                serde_json::to_vec(&serde_json::json!({"snapshot":s,"options":options})).unwrap(),
            )
            .unwrap();
        }
        let selected = options.iter().find(|o| o.technique == "eq_blend").unwrap();
        s.decks[a].actual_playback_bpm = Some(selected.target_bpm - 1.5);
        let same = revalidate_prepared_window(&s, selected, false).unwrap();
        assert_eq!(same.target_bpm, selected.target_bpm);
        s.decks[a].position_seconds = selected.launch_seconds + 0.1;
        assert!(revalidate_prepared_window(&s, selected, false).is_err());
        let fallback = revalidate_prepared_window(&s, selected, true).unwrap();
        assert_eq!(fallback.entry_seconds, selected.entry_seconds);
        assert_eq!(fallback.source, selected.source);
        println!(
            "{}",
            serde_json::json!({"reference":s.decks[a].track.as_ref().unwrap().title,
            "incoming":s.decks[b].track.as_ref().unwrap().title,"options":options.len(),
            "continuation_seconds":same.continuation_seconds,"minimum_seconds":same.minimum_continuation_seconds,
            "missed_launch_seconds":selected.launch_seconds,"fallback_launch_seconds":fallback.launch_seconds,
            "fallback_technique":fallback.technique})
        );
    }
}
