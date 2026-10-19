use crate::model::TrackAnalysis;
use audio_analysis::{StageMemo, StageOutcome, StageStamp};
use std::{
    cell::RefCell,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    sync::Arc,
    time::Instant,
};
const RHYTHM_VERSION: u32 = 1;
const WAVEFORM_VERSION: u32 = 1;
const KEY_VERSION: u32 = 1;
const NEURAL_VERSION: u32 = 1;
static RHYTHM_MEMO: StageMemo<(
    audio_analysis::TempoEstimate,
    Option<audio_analysis::BeatPhaseEstimate>,
)> = StageMemo::new(16);
static WAVEFORM_MEMO: StageMemo<audio_analysis::WaveformAnalysis> = StageMemo::new(16);
static KEY_MEMO: StageMemo<audio_analysis::KeyAnalysis> = StageMemo::new(16);
static NEURAL_MEMO: StageMemo<audio_analysis::NeuralRhythm> = StageMemo::new(16);
static RHYTHM_RUNS: AtomicU64 = AtomicU64::new(0);
static WAVEFORM_RUNS: AtomicU64 = AtomicU64::new(0);
static KEY_RUNS: AtomicU64 = AtomicU64::new(0);
static NEURAL_RUNS: AtomicU64 = AtomicU64::new(0);
pub fn analysis_cache_counts() -> [u64; 4] {
    std::array::from_fn(|i| {
        [&RHYTHM_RUNS, &WAVEFORM_RUNS, &KEY_RUNS, &NEURAL_RUNS][i].load(Ordering::Relaxed)
    })
}
/// Per-worker wall timings for the last offline source preparation; never callback data.
#[derive(Clone, Debug, serde::Serialize)]
pub struct AnalysisDiagnostics {
    pub cache_decision: &'static str,
    pub fingerprint_ms: f64,
    pub fingerprint_passes: u32,
    pub base_analysis_ms: f64,
    pub neural_ms: f64,
    pub total_ms: f64,
}
thread_local! {
    static LAST_ANALYSIS_DIAGNOSTICS: RefCell<Option<AnalysisDiagnostics>> = const { RefCell::new(None) };
}
pub fn take_analysis_diagnostics() -> Option<AnalysisDiagnostics> {
    LAST_ANALYSIS_DIAGNOSTICS.with(|value| value.borrow_mut().take())
}
const LEGACY_SPOTIFY_ORIGIN: &str = "Legacy Spotify analysis adopted by immutable release URI; original analysis PCM was not fingerprinted";

fn source_key(a: &TrackAnalysis) -> String {
    a.source_fingerprint
        .clone()
        .unwrap_or_else(|| format!("unverified:{}", a.id))
}
fn stage_current(stamp: &Option<StageStamp>, version: u32, dep: &str) -> bool {
    stamp.as_ref().is_some_and(|s| s.current(version, dep))
}
fn neural_dependency(a: &TrackAnalysis) -> String {
    format!(
        "{}:{}",
        source_key(a),
        audio_analysis::neural_cache_identity()
    )
}
/// A terminal neural result is cached too; passive loads do not retry failures.
pub fn beat_stage_current(a: &TrackAnalysis) -> bool {
    a.source_fingerprint.is_some()
        && stage_current(
            &a.analysis_stages.neural,
            NEURAL_VERSION,
            &neural_dependency(a),
        )
}

/// Completed derived results have no expiry. Explicit reanalysis can refresh
/// algorithms; passive loads only validate content and required display data.
pub fn playback_analysis_ready(a: &TrackAnalysis) -> bool {
    a.source_fingerprint.is_some()
        && a.analysis_stages.key.is_some()
        && a.analysis_stages.neural.is_some()
        && a.bpm.is_some_and(|b| b.is_finite() && b > 0.0)
        && a.waveform.len() == 4096
        && a.waveform_bands
            .as_ref()
            .is_some_and(|b| b.low.len() == 4096 && b.mid.len() == 4096 && b.high.len() == 4096)
}
/// All derived stages are compatible with this content and current algorithms.
pub fn analysis_current(a: &TrackAnalysis) -> bool {
    let fp = source_key(a);
    a.audio_measurements.as_ref().is_some_and(|m| {
        m.current(
            a.sample_rate,
            (a.duration_seconds * a.sample_rate as f64).round() as usize,
        )
    }) && beat_stage_current(a)
        && stage_current(&a.analysis_stages.rhythm, RHYTHM_VERSION, &fp)
        && stage_current(&a.analysis_stages.key, KEY_VERSION, &fp)
        && stage_current(&a.analysis_stages.waveform, WAVEFORM_VERSION, &fp)
        && a.waveform.len() == 4096
        && a.waveform_bands
            .as_ref()
            .is_some_and(|b| b.low.len() == 4096 && b.mid.len() == 4096 && b.high.len() == 4096)
}

use symphonia::core::{
    audio::SampleBuffer,
    codecs::DecoderOptions,
    errors::Error,
    formats::FormatOptions,
    io::{MediaSource, MediaSourceStream},
    meta::{MetadataOptions, StandardTagKey},
    probe::Hint,
};

pub(crate) struct TrackData {
    pub samples: Vec<[f32; 2]>,
    pub sample_rate: u32,
    pub analysis: TrackAnalysis,
    pub analysis_pending: bool,
}

/// Public display identity for a provider-owned audio source. Never put tokens here.
#[derive(Clone, Debug)]
pub struct AudioSourceInfo {
    pub id: String,
    pub location: String,
    pub title: String,
    pub artist: Option<String>,
    pub extension: Option<String>,
    pub release_identity: Option<String>,
}

/// A local file view starting after damaged ID3 metadata. Source bytes stay
/// unchanged, and seeks/lengths remain relative to the audio container.
pub(crate) struct AudioFileView {
    file: File,
    start: u64,
    len: u64,
}
impl Read for AudioFileView {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}
impl Seek for AudioFileView {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let absolute = match pos {
            SeekFrom::Start(offset) => self.file.seek(SeekFrom::Start(
                self.start
                    .checked_add(offset)
                    .ok_or_else(|| std::io::Error::other("Seek overflow"))?,
            ))?,
            other => self.file.seek(other)?,
        };
        absolute
            .checked_sub(self.start)
            .ok_or_else(|| std::io::Error::other("Seek before audio"))
    }
}
impl MediaSource for AudioFileView {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.len - self.start)
    }
}
pub(crate) fn audio_after_id3(path: &Path) -> Option<AudioFileView> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut start = 0_u64;
    // Handle stacked tags too. All sizes are validated before skipping them.
    for _ in 0..8 {
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut header = [0_u8; 10];
        if file.read_exact(&mut header).is_err() || &header[..3] != b"ID3" {
            break;
        }
        if !(2..=4).contains(&header[3]) || header[6..].iter().any(|b| *b >= 128) {
            return None;
        }
        let size = header[6..].iter().fold(0_u64, |n, b| (n << 7) | *b as u64);
        let next = start.checked_add(10 + size)?;
        if next >= len {
            return None;
        }
        start = next;
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut footer = [0_u8; 3];
        if file.read_exact(&mut footer).is_ok() && &footer == b"3DI" {
            start = start.checked_add(10)?;
        }
    }
    if start == 0 || start >= len {
        return None;
    }
    file.seek(SeekFrom::Start(start)).ok()?;
    Some(AudioFileView { file, start, len })
}
fn probe_media(
    source: Box<dyn MediaSource>,
    extension: Option<&str>,
    local: Option<&Path>,
) -> Result<symphonia::core::probe::ProbeResult, String> {
    let mut hint = Hint::new();
    if let Some(extension) = extension {
        hint.with_extension(extension);
    }
    let probe = |source| {
        symphonia::default::get_probe().format(
            &hint,
            MediaSourceStream::new(source, Default::default()),
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
    };
    match probe(source) {
        Ok(result) => Ok(result),
        Err(error) => {
            if let Some(path) = local {
                if let Some(source) = audio_after_id3(path) {
                    if let Ok(result) = probe(Box::new(source)) {
                        return Ok(result);
                    }
                }
            }
            Err(format!("Unsupported audio container: {error}"))
        }
    }
}

/// Read container metadata and tags without decoding PCM or running analysis.
fn aiff_frame_count(path: &Path) -> Option<u64> {
    if !path
        .extension()?
        .to_str()
        .is_some_and(|s| s.eq_ignore_ascii_case("aiff") || s.eq_ignore_ascii_case("aif"))
    {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let mut header = [0; 12];
    file.read_exact(&mut header).ok()?;
    if &header[..4] != b"FORM" || (&header[8..12] != b"AIFF" && &header[8..12] != b"AIFC") {
        return None;
    }
    for _ in 0..1024 {
        let mut chunk = [0; 8];
        file.read_exact(&mut chunk).ok()?;
        let size = u32::from_be_bytes(chunk[4..].try_into().ok()?);
        if &chunk[..4] == b"COMM" && size >= 6 {
            let mut value = [0; 6];
            file.read_exact(&mut value).ok()?;
            return Some(u64::from(u32::from_be_bytes(value[2..].try_into().ok()?)));
        }
        file.seek(SeekFrom::Current(i64::from(size) + i64::from(size % 2)))
            .ok()?;
    }
    None
}

pub fn probe_file(path: &Path) -> Result<TrackAnalysis, String> {
    let path = path
        .canonicalize()
        .map_err(|e| format!("Cannot open track: {e}"))?;
    let file = File::open(&path).map_err(|e| format!("Cannot open track: {e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() == 0 {
        return Err("Audio file is empty (0 bytes)".into());
    }
    let mut probed = probe_media(
        Box::new(file),
        path.extension().and_then(|s| s.to_str()),
        Some(&path),
    )?;
    let track = probed
        .format
        .default_track()
        .ok_or("No audio track found")?;
    let rate = track.codec_params.sample_rate.unwrap_or(0);
    let duration = aiff_frame_count(&path)
        .or(track.codec_params.n_frames)
        .map(|frames| {
            if let Some(base) = track.codec_params.time_base {
                let time = base.calc_time(frames);
                time.seconds as f64 + time.frac
            } else if rate > 0 {
                frames as f64 / rate as f64
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);
    let mut analysis = TrackAnalysis {
        id: path.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        title: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        duration_seconds: duration,
        sample_rate: rate,
        channels: 2,
        recording_allowed: true,
        ..Default::default()
    };
    let mut apply_tags = |tags: &[symphonia::core::meta::Tag]| {
        for tag in tags {
            match tag.std_key {
                Some(StandardTagKey::TrackTitle) => analysis.title = tag.value.to_string(),
                Some(StandardTagKey::Artist) => analysis.artist = Some(tag.value.to_string()),
                Some(StandardTagKey::Bpm) => {
                    analysis.bpm = tag
                        .value
                        .to_string()
                        .parse::<f64>()
                        .ok()
                        .filter(|b| b.is_finite() && (30.0..=300.0).contains(b));
                    analysis.bpm_source = analysis.bpm.map(|_| "tag".into());
                    analysis.bpm_confidence = if analysis.bpm.is_some() { 1.0 } else { 0.0 };
                }
                _ => {}
            }
        }
    };
    if let Some(probe_metadata) = probed.metadata.get() {
        if let Some(metadata) = probe_metadata.current() {
            apply_tags(metadata.tags());
        }
    }
    if let Some(metadata) = probed.format.metadata().current() {
        apply_tags(metadata.tags());
    }
    Ok(analysis)
}

/// Salvage readable fields from damaged containers with the existing tolerant probe.
pub(crate) fn recover_file_metadata(path: &Path) -> Result<Vec<crate::EmbeddedTag>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut probed = probe_media(
        Box::new(file),
        path.extension().and_then(|s| s.to_str()),
        Some(path),
    )?;
    let mut fields = Vec::new();
    let mut collect = |tags: &[symphonia::core::meta::Tag]| {
        for tag in tags {
            if matches!(tag.value, symphonia::core::meta::Value::Binary(_)) {
                continue;
            }
            let text = tag.value.to_string();
            for value in text.split('\0').map(str::trim).filter(|v| !v.is_empty()) {
                fields.push(crate::EmbeddedTag {
                    format: "RecoveredContainer".into(),
                    key: tag.key.clone(),
                    standard_key: tag.std_key.map(|key| format!("{key:?}")),
                    value: value.into(),
                    description: String::new(),
                    language: String::new(),
                });
            }
        }
    };
    if let Some(metadata) = probed.metadata.get() {
        if let Some(revision) = metadata.current() {
            collect(revision.tags());
        }
    }
    if let Some(revision) = probed.format.metadata().current() {
        collect(revision.tags());
    }
    Ok(fields)
}

/// Extract one embedded cover on demand. No PCM decode or audio analysis.
pub fn embedded_artwork(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut probed = probe_media(
        Box::new(file),
        path.extension().and_then(|s| s.to_str()),
        Some(path),
    )?;
    let choose = |visuals: &[symphonia::core::meta::Visual]| {
        visuals
            .iter()
            .filter(|v| !v.data.is_empty() && v.data.len() <= 8 * 1024 * 1024)
            .min_by_key(|v| v.usage != Some(symphonia::core::meta::StandardVisualKey::FrontCover))
            .map(|v| v.data.to_vec())
    };
    if let Some(metadata) = probed.metadata.get() {
        if let Some(revision) = metadata.current() {
            if let Some(bytes) = choose(revision.visuals()) {
                return Ok(Some(bytes));
            }
        }
    }
    Ok(probed
        .format
        .metadata()
        .current()
        .and_then(|revision| choose(revision.visuals())))
}

pub fn analyze_file(path: &Path) -> Result<TrackAnalysis, String> {
    Ok(decode_file(path)?.analysis.clone())
}

/// Decode and analyze a file including the neural beat stage, for import and
/// library-update workers. A model failure keeps the DSP analysis and returns
/// the reason; decoding failure and cancellation are errors.
pub fn analyze_file_with_beat_model(
    path: &Path,
    cancel: &dyn Fn() -> bool,
) -> Result<(TrackAnalysis, Option<String>), String> {
    analyze_file_cached(path, None, cancel)
}

/// Reuse persisted source analysis and refresh only missing neural timing.
pub fn analyze_file_cached(
    path: &Path,
    cached: Option<TrackAnalysis>,
    cancel: &dyn Fn() -> bool,
) -> Result<(TrackAnalysis, Option<String>), String> {
    let track = decode_file_cached(path, cached)?;
    if cancel() {
        return Err("Beat analysis cancelled".into());
    }
    let mut analysis = track.analysis.clone();
    match analyze_beats(&track.samples, track.sample_rate, &mut analysis, cancel) {
        Ok(()) => Ok((analysis, None)),
        Err(_) if cancel() => Err("Beat analysis cancelled".into()),
        Err(e) => Ok((analysis, Some(e))),
    }
}

/// Refresh neural timing without generating structural labels or cues.
pub fn analyze_beats(
    samples: &[[f32; 2]],
    sample_rate: u32,
    analysis: &mut TrackAnalysis,
    cancel: &dyn Fn() -> bool,
) -> Result<(), String> {
    if cancel() {
        return Err("Beat analysis cancelled".into());
    }
    if analysis.neural_rhythm.is_none() {
        apply_neural_rhythm(analysis, samples, sample_rate);
    }
    if let Some(rhythm) = &analysis.neural_rhythm {
        analysis.beat_model = Some(audio_analysis::stage_from_rhythm(rhythm));
    }
    if cancel() {
        return Err("Beat analysis cancelled".into());
    }
    analysis.rhythm_error.clone().map_or(Ok(()), Err)
}

/// Merge a beat-model stage into an analysis. A usable model grid replaces the
/// DSP or tag estimate unless the BPM or grid was set manually. The previous BPM
/// stays visible as a candidate, and a half/double or tag disagreement marks
/// the BPM ambiguous. The stage is always recorded so it is not re-run.
pub fn apply_beat_model(analysis: &mut TrackAnalysis, stage: audio_analysis::BeatModelStage) {
    if analysis.neural_rhythm.is_some() {
        analysis.beat_model = Some(stage);
        return;
    }
    if let Some(grid) = stage.usable_grid() {
        let confidence = grid.on_grid.clamp(0.0, 1.0);
        let manual_bpm = analysis.bpm_source.as_deref() == Some("manual");
        // A manual BPM keeps its grid unless it is the same pulse (or half/double).
        let grid_fits_bpm = !manual_bpm
            || analysis.bpm.is_some_and(|b| {
                [0.5, 1.0, 2.0]
                    .iter()
                    .any(|r| (b / (grid.bpm * r) - 1.0).abs() <= 0.02)
            });
        if !manual_bpm {
            let previous = analysis.bpm.map(|bpm| audio_analysis::TempoCandidate {
                bpm,
                score: analysis.bpm_confidence,
            });
            let differs =
                |c: &audio_analysis::TempoCandidate| (c.bpm / grid.bpm - 1.0).abs() > 0.02;
            let mut candidates = vec![audio_analysis::TempoCandidate {
                bpm: grid.bpm,
                score: confidence,
            }];
            for c in previous.iter().chain(&analysis.bpm_candidates) {
                if differs(c)
                    && candidates
                        .iter()
                        .all(|k| (k.bpm / c.bpm - 1.0).abs() > 0.02)
                {
                    candidates.push(c.clone());
                }
            }
            candidates.truncate(4);
            analysis.bpm_ambiguous = previous.as_ref().is_some_and(differs);
            analysis.bpm = Some(grid.bpm);
            analysis.bpm_confidence = confidence;
            analysis.bpm_candidates = candidates;
            analysis.bpm_source = Some("model".into());
        }
        if grid_fits_bpm && analysis.beatgrid_source.as_deref() != Some("manual") {
            analysis.beatgrid_offset_seconds = Some(grid.downbeat_offset_seconds);
            analysis.beatgrid_confidence = Some(grid.bar_agreement.clamp(0.0, 1.0));
            analysis.beatgrid_source = Some("model".into());
        }
    }
    analysis.beat_model = Some(stage);
}

pub(crate) fn decode_file(path: &Path) -> Result<Arc<TrackData>, String> {
    decode_file_cached(path, None)
}

/// Decode a file, reusing a cached analysis when it still matches the audio.
pub(crate) fn decode_file_cached(
    path: &Path,
    cached: Option<TrackAnalysis>,
) -> Result<Arc<TrackData>, String> {
    let file = File::open(path).map_err(|e| format!("Cannot open track: {e}"))?;
    decode_source_cached(
        Box::new(file),
        AudioSourceInfo {
            id: path
                .canonicalize()
                .unwrap_or_else(|_| path.to_owned())
                .to_string_lossy()
                .into(),
            location: path.to_string_lossy().into(),
            title: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            artist: None,
            extension: path.extension().and_then(|s| s.to_str()).map(str::to_owned),
            release_identity: None,
        },
        cached,
        true,
        Some(path),
    )
}

/// Decode local audio for immediate playback; missing analysis is deferred.
pub(crate) fn decode_file_playable(
    path: &Path,
    cached: Option<TrackAnalysis>,
) -> Result<Arc<TrackData>, String> {
    let metadata = probe_file(path)?;
    let file = File::open(&metadata.path).map_err(|e| format!("Cannot open track: {e}"))?;
    decode_source_cached(
        Box::new(file),
        AudioSourceInfo {
            id: metadata.id,
            location: metadata.path,
            title: metadata.title,
            artist: metadata.artist,
            extension: path.extension().and_then(|s| s.to_str()).map(str::to_owned),
            release_identity: None,
        },
        cached,
        false,
        Some(path),
    )
}

/// Decode provider reads on the calling worker, never in the render callback.
pub(crate) fn decode_source(
    source: Box<dyn MediaSource>,
    info: AudioSourceInfo,
) -> Result<Arc<TrackData>, String> {
    decode_source_cached(source, info, None, true, None)
}

fn decode_source_cached(
    source: Box<dyn MediaSource>,
    info: AudioSourceInfo,
    cached: Option<TrackAnalysis>,
    analyze: bool,
    local: Option<&Path>,
) -> Result<Arc<TrackData>, String> {
    let started = Instant::now();
    let file_identity = local
        .map(|path| path.metadata().and_then(|m| Ok((m.len(), m.modified()?))))
        .transpose()
        .map_err(|e| e.to_string())?;
    let mut probed = probe_media(source, info.extension.as_deref(), local)?;
    let mut format = probed.format;
    let selected = format.default_track().ok_or("No audio track found")?;
    let track_id = selected.id;
    let mut sample_rate = selected.codec_params.sample_rate.unwrap_or(48_000);
    let frame_hint = selected.codec_params.n_frames.unwrap_or(0).min(24_000_000) as usize;
    // The AIFF demuxer counts its SSND header as samples; COMM owns the frame count.
    let aiff_frames = local
        .and_then(aiff_frame_count)
        .and_then(|n| usize::try_from(n).ok())
        .filter(|n| *n > 0);

    let mut decoder = symphonia::default::get_codecs()
        .make(&selected.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("Unsupported audio codec: {e}"))?;
    let mut title = info.title;
    let mut artist = info.artist;
    let mut tagged_bpm = None;
    if let Some(probe_metadata) = probed.metadata.get() {
        if let Some(metadata) = probe_metadata.current() {
            for tag in metadata.tags() {
                match tag.std_key {
                    Some(StandardTagKey::TrackTitle) => title = tag.value.to_string(),
                    Some(StandardTagKey::Artist) => artist = Some(tag.value.to_string()),
                    Some(StandardTagKey::Bpm) => {
                        tagged_bpm = tag
                            .value
                            .to_string()
                            .parse::<f64>()
                            .ok()
                            .filter(|b| b.is_finite() && (30.0..=300.0).contains(b))
                    }
                    _ => {}
                }
            }
        }
    }
    if let Some(metadata) = format.metadata().current() {
        for tag in metadata.tags() {
            match tag.std_key {
                Some(StandardTagKey::TrackTitle) => title = tag.value.to_string(),
                Some(StandardTagKey::Artist) => artist = Some(tag.value.to_string()),
                Some(StandardTagKey::Bpm) => {
                    tagged_bpm = tag
                        .value
                        .to_string()
                        .parse::<f64>()
                        .ok()
                        .filter(|bpm| bpm.is_finite() && (30.0..=300.0).contains(bpm))
                }
                _ => {}
            }
        }
    }
    let mut pcm = Vec::new();
    // Reserve once for typical tracks, without trusting an unbounded container hint.
    pcm.try_reserve_exact(frame_hint)
        .map_err(|e| format!("Cannot allocate decoded audio: {e}"))?;
    let mut sample_buffer = None::<SampleBuffer<f32>>;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(format!("Cannot read audio packet: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(audio) => audio,
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(format!("Cannot decode track: {e}")),
        };
        let spec = *decoded.spec();
        if !pcm.is_empty() && spec.rate != sample_rate {
            return Err("Tracks with changing sample rate are unsupported".into());
        }
        sample_rate = spec.rate;
        let channels = spec.channels.count();
        if channels == 0 {
            return Err("Audio packet has no channels".into());
        }
        let samples = sample_buffer
            .get_or_insert_with(|| SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
        if samples.capacity() < decoded.capacity() * channels {
            *samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        }
        samples.copy_interleaved_ref(decoded);
        for frame in samples.samples().chunks_exact(channels) {
            let left = sanitize(frame[0]);
            let right = sanitize(if channels > 1 { frame[1] } else { frame[0] });
            pcm.push([left, right]);
        }
        if let Some(frames) = aiff_frames {
            if pcm.len() >= frames {
                pcm.truncate(frames);
                break;
            }
        }
        // Bound memory used by untrusted/accidentally huge media to ~1.4 GiB of PCM.
        if pcm.len() > 180_000_000 {
            return Err("Track exceeds the in-memory deck limit".into());
        }
    }
    if pcm.is_empty() {
        return Err("Track contains no decoded audio".into());
    }
    let info = AudioSourceInfo {
        title,
        artist,
        ..info
    };
    let mut track = if analyze {
        prepare_pcm_cached(pcm, sample_rate, info, true, cached)?
    } else {
        // Samples were sanitized during decode. Reuse only content-verified
        // metadata; never start DSP or model work on the playable load path.
        let fingerprint_started = Instant::now();
        let encoded = local
            .map(audio_analysis::encoded_file_fingerprint)
            .transpose()?;
        if let Some(path) = local {
            let m = path.metadata().map_err(|e| e.to_string())?;
            if Some((m.len(), m.modified().map_err(|e| e.to_string())?)) != file_identity {
                return Err("Track changed while loading; retry the selected file".into());
            }
        }
        let encoded_match = cached.as_ref().is_some_and(|a| {
            a.analysis_stages
                .source_file
                .as_ref()
                .is_some_and(|s| s.version == 1 && Some(&s.dependency) == encoded.as_ref())
        });
        let fingerprint = if encoded_match {
            cached.as_ref().and_then(|a| a.source_fingerprint.clone())
        } else {
            cached
                .as_ref()
                .and_then(|a| a.source_fingerprint.as_ref())
                .map(|_| audio_analysis::source_fingerprint(&pcm, sample_rate, None))
        };
        let fingerprint_ms = fingerprint_started.elapsed().as_secs_f64() * 1000.;
        let mut analysis = cached
            .filter(|a| {
                a.source_fingerprint.is_some()
                    && a.source_fingerprint == fingerprint
                    && (1..=audio_analysis::ANALYSIS_VERSION).contains(&a.analysis_version)
                    && a.sample_rate == sample_rate
                    && a.channels == 2
                    && (a.duration_seconds - pcm.len() as f64 / sample_rate as f64).abs() < 0.1
            })
            .unwrap_or_default();
        analysis.id = info.id;
        analysis.path = info.location;
        analysis.title = info.title;
        analysis.artist = info.artist;
        analysis.duration_seconds = pcm.len() as f64 / sample_rate as f64;
        analysis.sample_rate = sample_rate;
        analysis.channels = 2;
        analysis.recording_allowed = true;
        let cache_decision = if analysis.analysis_version == 0 {
            "deferred"
        } else {
            "content_match"
        };
        analysis.source_fingerprint = fingerprint;
        ensure_measurements(&mut analysis, &pcm, sample_rate);
        analysis.analysis_stages.source_file = encoded.map(|hash| StageStamp::ready(1, hash));
        LAST_ANALYSIS_DIAGNOSTICS.with(|value| {
            *value.borrow_mut() = Some(AnalysisDiagnostics {
                cache_decision,
                fingerprint_ms,
                fingerprint_passes: u32::from(
                    !encoded_match && analysis.source_fingerprint.is_some(),
                ),
                base_analysis_ms: 0.0,
                neural_ms: 0.0,
                total_ms: started.elapsed().as_secs_f64() * 1000.,
            });
        });
        Arc::new(TrackData {
            analysis_pending: !playback_analysis_ready(&analysis),
            samples: pcm,
            sample_rate,
            analysis,
        })
    };
    // Tags refine only a fresh DSP estimate; model and manual results stand.
    let fresh_estimate = track.analysis.beat_model.is_none()
        && matches!(
            track.analysis.bpm_source.as_deref(),
            None | Some("analysis")
        );
    if let Some(bpm) = tagged_bpm.filter(|_| fresh_estimate) {
        if let Some(track) = Arc::get_mut(&mut track) {
            track.analysis.bpm = Some(bpm);
            track.analysis.bpm_confidence = 1.0;
            track.analysis.bpm_source = Some("tag".into());
            track.analysis.bpm_ambiguous = false;
            if analyze && track.analysis.neural_rhythm.is_none() {
                let phase =
                    audio_analysis::estimate_phase(&track.samples, track.sample_rate, Some(bpm));
                track.analysis.beatgrid_offset_seconds = phase.as_ref().map(|p| p.offset_seconds);
                track.analysis.beatgrid_confidence = phase.as_ref().map(|p| p.confidence);
                track.analysis.beatgrid_source = phase.map(|_| "tag".into());
            }
        }
    }
    Ok(track)
}

#[cfg(test)]
pub(crate) fn prepare_pcm(
    pcm: Vec<[f32; 2]>,
    sample_rate: u32,
    info: AudioSourceInfo,
    recording_allowed: bool,
) -> Result<Arc<TrackData>, String> {
    prepare_pcm_cached(pcm, sample_rate, info, recording_allowed, None)
}

pub(crate) fn prepare_pcm_cached(
    mut pcm: Vec<[f32; 2]>,
    sample_rate: u32,
    info: AudioSourceInfo,
    recording_allowed: bool,
    cached: Option<TrackAnalysis>,
) -> Result<Arc<TrackData>, String> {
    if !(8000..=192000).contains(&sample_rate) || pcm.is_empty() || pcm.len() > 180_000_000 {
        return Err("Invalid or oversized decoded audio source".into());
    }
    for frame in &mut pcm {
        frame[0] = sanitize(frame[0]);
        frame[1] = sanitize(frame[1]);
    }
    let analysis = analyze_pcm_cached(&pcm, sample_rate, &info, recording_allowed, cached);
    Ok(Arc::new(TrackData {
        analysis_pending: false,
        samples: pcm,
        sample_rate,
        analysis,
    }))
}
pub fn analyze_pcm_cached(
    pcm: &[[f32; 2]],
    sample_rate: u32,
    info: &AudioSourceInfo,
    recording_allowed: bool,
    cached: Option<TrackAnalysis>,
) -> TrackAnalysis {
    let started = Instant::now();
    let hash_started = Instant::now();
    let fingerprint =
        audio_analysis::source_fingerprint(pcm, sample_rate, info.release_identity.as_deref());
    let fingerprint_ms = hash_started.elapsed().as_secs_f64() * 1000.;
    let legacy = cached.as_ref().is_some_and(|a| {
        legacy_spotify_reusable(a, sample_rate, pcm.len(), info, recording_allowed)
    });
    let cache_decision = match cached.as_ref() {
        None => "missing",
        Some(_) if legacy => "legacy_spotify_uri_adopted",
        Some(a) if a.source_fingerprint.as_deref() == Some(fingerprint.as_str()) => "content_match",
        Some(a) if a.source_fingerprint.is_some() => "content_mismatch",
        Some(_) => "legacy_unverified_recomputed",
    };
    let overrides = cached.as_ref().map(|a| {
        (
            a.bpm_source.as_deref() == Some("manual"),
            a.bpm,
            a.beatgrid_source.as_deref() == Some("manual"),
            a.beatgrid_offset_seconds,
            a.neural_rhythm
                .as_ref()
                .and_then(|g| {
                    a.manual_grid_anchor_index
                        .and_then(|i| g.beats_seconds.get(i))
                })
                .copied(),
        )
    });
    let base_started = Instant::now();
    let mut analysis = cached
        .filter(|a| {
            (legacy || a.source_fingerprint.as_deref() == Some(fingerprint.as_str()))
                && (1..=audio_analysis::ANALYSIS_VERSION).contains(&a.analysis_version)
                && a.sample_rate == sample_rate
                && a.channels == 2
                && (a.duration_seconds - pcm.len() as f64 / sample_rate as f64).abs() < 0.1
        })
        .unwrap_or_else(|| {
            analyze_pcm_with_fingerprint(
                pcm,
                sample_rate,
                info,
                recording_allowed,
                fingerprint.clone(),
            )
        });
    analysis.source_fingerprint = Some(fingerprint);
    if legacy {
        adopt_legacy_spotify_stages(&mut analysis);
    }
    ensure_base_stages(&mut analysis, pcm, sample_rate);
    let base_analysis_ms = base_started.elapsed().as_secs_f64() * 1000.;
    let neural_started = Instant::now();
    apply_neural_rhythm(&mut analysis, pcm, sample_rate);
    if let Some(rhythm) = &analysis.neural_rhythm {
        analysis.beat_model = Some(audio_analysis::stage_from_rhythm(rhythm));
    }
    let neural_ms = neural_started.elapsed().as_secs_f64() * 1000.;
    analysis.analysis_version = audio_analysis::ANALYSIS_VERSION;
    if let Some((manual_bpm, bpm, manual_grid, offset, anchor_time)) = overrides {
        if manual_bpm {
            analysis.bpm = bpm;
            analysis.bpm_source = Some("manual".into());
            analysis.bpm_confidence = 1.0;
            analysis.bpm_ambiguous = false;
        }
        if manual_grid {
            analysis.beatgrid_offset_seconds = offset;
            analysis.beatgrid_source = Some("manual".into());
            analysis.beatgrid_confidence = None;
            analysis.manual_grid_anchor_index = analysis.neural_rhythm.as_ref().map(|g| {
                crate::grid::nearest_index(&g.beats_seconds, anchor_time.or(offset).unwrap_or(0.0))
            });
        }
    }
    if analysis.bpm_source.as_deref() != Some("manual") {
        if let Some(rhythm) = &analysis.neural_rhythm {
            analysis.bpm_ambiguous = neural_tempo_ambiguous(rhythm);
        }
    }
    analysis.id = info.id.clone();
    analysis.path = info.location.clone();
    analysis.source_release = info.release_identity.clone();
    analysis.recording_allowed = recording_allowed;
    analysis.title = info.title.clone();
    analysis.artist = info.artist.clone();
    LAST_ANALYSIS_DIAGNOSTICS.with(|value| {
        *value.borrow_mut() = Some(AnalysisDiagnostics {
            cache_decision,
            fingerprint_ms,
            fingerprint_passes: 1,
            base_analysis_ms,
            neural_ms,
            total_ms: started.elapsed().as_secs_f64() * 1000.,
        })
    });
    analysis
}
fn legacy_spotify_reusable(
    a: &TrackAnalysis,
    rate: u32,
    frames: usize,
    info: &AudioSourceInfo,
    recording_allowed: bool,
) -> bool {
    // Only legacy, current-model provider data can inherit this trust. A present
    // content hash always wins, including when the provider URI stayed the same.
    let uri = &info.location;
    let valid_uri = uri
        .strip_prefix("spotify:track:")
        .is_some_and(|id| id.len() == 22 && id.bytes().all(|b| b.is_ascii_alphanumeric()));
    !recording_allowed
        && !a.recording_allowed
        && valid_uri
        && a.source_fingerprint.is_none()
        && a.analysis_stages == audio_analysis::AnalysisStages::default()
        && a.analysis_version == audio_analysis::ANALYSIS_VERSION
        && a.id == *uri
        && a.path == *uri
        && info.id == *uri
        && info.release_identity.as_deref() == Some(uri.as_str())
        && a.source_release
            .as_ref()
            .is_none_or(|release| release == uri)
        && a.sample_rate == rate
        && a.channels == 2
        && (a.duration_seconds - frames as f64 / rate as f64).abs() < 0.1
        && a.waveform.len() == 4096
        && a.waveform_bands
            .as_ref()
            .is_some_and(|b| b.low.len() == 4096 && b.mid.len() == 4096 && b.high.len() == 4096)
        && a.rhythm_error.is_none()
        && a.neural_rhythm.as_ref().is_some_and(|r| {
            audio_analysis::legacy_neural_provenance_current(r, a.duration_seconds)
        })
}
fn adopt_legacy_spotify_stages(a: &mut TrackAnalysis) {
    let stamp = |version, dependency| StageStamp {
        version,
        dependency,
        outcome: StageOutcome::Ready,
        reason: Some(LEGACY_SPOTIFY_ORIGIN.into()),
    };
    let source = source_key(a);
    a.analysis_stages.rhythm = Some(stamp(RHYTHM_VERSION, source.clone()));
    a.analysis_stages.waveform = Some(stamp(WAVEFORM_VERSION, source.clone()));
    // An absent key is a terminal statistical abstention, not a reason to rerun.
    let mut key = stamp(KEY_VERSION, source.clone());
    if a.key.is_none() {
        key.outcome = StageOutcome::Unavailable;
    }
    a.analysis_stages.key = Some(key);
    a.analysis_stages.neural = Some(stamp(NEURAL_VERSION, neural_dependency(a)));
}

pub(crate) fn retry_neural(a: &mut TrackAnalysis) {
    NEURAL_MEMO.forget(&neural_dependency(a));
    a.analysis_stages.neural = None;
}
fn ensure_measurements(a: &mut TrackAnalysis, pcm: &[[f32; 2]], rate: u32) {
    if a.audio_measurements
        .as_ref()
        .is_none_or(|m| !m.current(rate, pcm.len()))
    {
        a.audio_measurements = Some(audio_analysis::measure_audio(pcm, rate));
    }
}
fn ensure_base_stages(a: &mut TrackAnalysis, pcm: &[[f32; 2]], rate: u32) {
    ensure_measurements(a, pcm, rate);
    let fp = source_key(a);
    if !stage_current(&a.analysis_stages.rhythm, RHYTHM_VERSION, &fp) {
        let value = RHYTHM_MEMO
            .get(format!("{fp}:rhythm-v{RHYTHM_VERSION}"), || {
                RHYTHM_RUNS.fetch_add(1, Ordering::Relaxed);
                Ok(audio_analysis::analyze_rhythm(pcm, rate))
            })
            .expect("derived stage cache");
        if !matches!(
            a.bpm_source.as_deref(),
            Some("manual" | "tag" | "model" | "beat_this")
        ) {
            a.bpm = value.0.bpm;
            a.bpm_confidence = value.0.confidence;
            a.bpm_candidates = value.0.candidates.clone();
            a.bpm_ambiguous = value.0.ambiguous;
            a.bpm_source = a.bpm.map(|_| "analysis".into());
        }
        if !matches!(
            a.beatgrid_source.as_deref(),
            Some("manual" | "model" | "beat_this")
        ) {
            a.beatgrid_offset_seconds = value.1.as_ref().map(|p| p.offset_seconds);
            a.beatgrid_confidence = value.1.as_ref().map(|p| p.confidence);
            a.beatgrid_source = a.beatgrid_offset_seconds.map(|_| "analysis".into());
        }
        a.analysis_stages.rhythm = Some(StageStamp::ready(RHYTHM_VERSION, fp.clone()));
    }
    if !stage_current(&a.analysis_stages.waveform, WAVEFORM_VERSION, &fp)
        || a.waveform.len() != 4096
        || a.waveform_bands
            .as_ref()
            .is_none_or(|b| b.low.len() != 4096 || b.mid.len() != 4096 || b.high.len() != 4096)
    {
        let value = WAVEFORM_MEMO
            .get(format!("{fp}:wave-v{WAVEFORM_VERSION}"), || {
                WAVEFORM_RUNS.fetch_add(1, Ordering::Relaxed);
                Ok(audio_analysis::analyze_waveform(pcm, rate, 4096))
            })
            .expect("derived stage cache");
        a.waveform = value.peaks.clone();
        a.waveform_bands = Some(value.bands.clone());
        a.analysis_stages.waveform = Some(StageStamp::ready(WAVEFORM_VERSION, fp.clone()));
    }
    if !stage_current(&a.analysis_stages.key, KEY_VERSION, &fp) {
        let value = KEY_MEMO
            .get(format!("{fp}:key-v{KEY_VERSION}"), || {
                KEY_RUNS.fetch_add(1, Ordering::Relaxed);
                Ok(audio_analysis::analyze_key(pcm, rate))
            })
            .expect("derived stage cache");
        a.key = value.estimate.as_ref().map(|k| k.label.clone());
        a.camelot = value.estimate.as_ref().map(|k| k.camelot.clone());
        a.key_confidence = value.estimate.as_ref().map(|k| k.confidence);
        a.key_candidates = value.candidates.clone();
        a.analysis_stages.key = Some(StageStamp {
            version: KEY_VERSION,
            dependency: fp,
            outcome: if value.estimate.is_some() {
                StageOutcome::Ready
            } else {
                StageOutcome::Unavailable
            },
            reason: None,
        });
    }
}

fn sanitize(x: f32) -> f32 {
    if x.is_finite() {
        x.clamp(-8.0, 8.0)
    } else {
        0.0
    }
}

/// Reusable offline analysis of an existing PCM allocation. No file or provider
/// reads, no recording/export, and no transport mutation.
pub(crate) fn analyze_pcm(
    pcm: &[[f32; 2]],
    sample_rate: u32,
    info: &AudioSourceInfo,
    recording_allowed: bool,
) -> TrackAnalysis {
    analyze_pcm_cached(pcm, sample_rate, info, recording_allowed, None)
}
fn analyze_pcm_with_fingerprint(
    pcm: &[[f32; 2]],
    sample_rate: u32,
    info: &AudioSourceInfo,
    recording_allowed: bool,
    fingerprint: String,
) -> TrackAnalysis {
    let (rhythm, waveform, key) = std::thread::scope(|scope| {
        let rhythm = scope.spawn(|| {
            RHYTHM_MEMO
                .get(format!("{fingerprint}:rhythm-v{RHYTHM_VERSION}"), || {
                    RHYTHM_RUNS.fetch_add(1, Ordering::Relaxed);
                    Ok(audio_analysis::analyze_rhythm(pcm, sample_rate))
                })
                .expect("derived stage cache")
        });
        let waveform = scope.spawn(|| {
            WAVEFORM_MEMO
                .get(format!("{fingerprint}:wave-v{WAVEFORM_VERSION}"), || {
                    WAVEFORM_RUNS.fetch_add(1, Ordering::Relaxed);
                    Ok(audio_analysis::analyze_waveform(pcm, sample_rate, 4096))
                })
                .expect("derived stage cache")
                .as_ref()
                .clone()
        });
        let key = scope.spawn(|| {
            KEY_MEMO
                .get(format!("{fingerprint}:key-v{KEY_VERSION}"), || {
                    KEY_RUNS.fetch_add(1, Ordering::Relaxed);
                    Ok(audio_analysis::analyze_key(pcm, sample_rate))
                })
                .expect("derived stage cache")
                .as_ref()
                .clone()
        });
        (
            rhythm.join().expect("rhythm worker"),
            waveform.join().expect("waveform worker"),
            key.join().expect("key worker"),
        )
    });
    let (tempo, phase) = rhythm.as_ref().clone();
    let mut analysis = TrackAnalysis {
        neural_rhythm: None,
        rhythm_error: None,
        manual_grid_anchor_index: None,
        id: info.id.clone(),
        path: info.location.clone(),
        title: info.title.clone(),
        artist: info.artist.clone(),
        duration_seconds: pcm.len() as f64 / sample_rate as f64,
        sample_rate,
        channels: 2,
        recording_allowed,
        analysis_version: audio_analysis::ANALYSIS_VERSION,
        bpm: tempo.bpm,
        bpm_confidence: tempo.confidence,
        bpm_ambiguous: tempo.ambiguous,
        bpm_candidates: tempo.candidates,
        bpm_source: tempo.bpm.map(|_| "analysis".into()),
        beatgrid_offset_seconds: phase.as_ref().map(|p| p.offset_seconds),
        beatgrid_confidence: phase.as_ref().map(|p| p.confidence),
        beatgrid_source: phase.map(|_| "analysis".into()),
        key: key.estimate.as_ref().map(|k| k.label.clone()),
        key_confidence: key.estimate.as_ref().map(|k| k.confidence),
        camelot: key.estimate.map(|k| k.camelot),
        key_candidates: key.candidates,
        waveform: waveform.peaks,
        waveform_bands: Some(waveform.bands),
        beat_model: None,

        cue_points: Vec::new(),
        source_fingerprint: None,
        source_release: info.release_identity.clone(),
        audio_measurements: Some(audio_analysis::measure_audio(pcm, sample_rate)),
        analysis_stages: Default::default(),
    };
    analysis.source_fingerprint = Some(fingerprint.clone());
    analysis.analysis_stages.rhythm = Some(StageStamp::ready(RHYTHM_VERSION, fingerprint.clone()));
    analysis.analysis_stages.waveform =
        Some(StageStamp::ready(WAVEFORM_VERSION, fingerprint.clone()));
    analysis.analysis_stages.key = Some(StageStamp {
        version: KEY_VERSION,
        dependency: fingerprint,
        outcome: if analysis.key.is_some() {
            StageOutcome::Ready
        } else {
            StageOutcome::Unavailable
        },
        reason: None,
    });
    analysis
}

fn neural_tempo_ambiguous(rhythm: &audio_analysis::NeuralRhythm) -> bool {
    if rhythm
        .bar_evidence
        .iter()
        .any(|e| !e.pulse_repairs.is_empty() || e.half_phase_corrected)
    {
        return true;
    }
    if rhythm.beats_seconds.len() < 65 {
        return false;
    }
    let mut rates: Vec<f64> = rhythm
        .beats_seconds
        .windows(33)
        .map(|w| 1920.0 / (w[32] - w[0]))
        .collect();
    rates.sort_by(f64::total_cmp);
    let low = rates[(rates.len() - 1) * 2 / 100];
    let high = rates[(rates.len() - 1) * 98 / 100];
    // A sustained large tempo spread can represent intentional changes or an
    // octave interpretation switch. It needs passage-level review in either case.
    low > 0.0 && high / low > 1.6
}

fn apply_neural_rhythm(analysis: &mut TrackAnalysis, pcm: &[[f32; 2]], rate: u32) {
    // Unit fixtures exercise deterministic DSP; actual inference has a separate
    // full-model integration test. Application builds always use the model.
    let dep = neural_dependency(analysis);
    if stage_current(&analysis.analysis_stages.neural, NEURAL_VERSION, &dep)
        && analysis
            .analysis_stages
            .neural
            .as_ref()
            .is_some_and(|s| s.outcome != StageOutcome::Ready || analysis.neural_rhythm.is_some())
    {
        return;
    }
    let result = NEURAL_MEMO.get(dep.clone(), || {
        NEURAL_RUNS.fetch_add(1, Ordering::Relaxed);
        if cfg!(test) {
            Err("Fixture build omits inference".into())
        } else {
            audio_analysis::analyze_neural_rhythm(pcm, rate)
        }
    });
    analysis.analysis_stages.neural = Some(StageStamp {
        version: NEURAL_VERSION,
        dependency: dep,
        outcome: if result.is_ok() {
            StageOutcome::Ready
        } else {
            StageOutcome::Failed
        },
        reason: result.as_ref().err().cloned(),
    });
    match result {
        Ok(rhythm) => {
            let rhythm = rhythm.as_ref().clone();
            if analysis.beatgrid_source.as_deref() == Some("manual") {
                let phase_changed = analysis.neural_rhythm.as_ref().is_some_and(|old| {
                    old.bar_evidence.iter().any(|e| e.half_phase_corrected)
                        != rhythm.bar_evidence.iter().any(|e| e.half_phase_corrected)
                });
                let anchor_time = if phase_changed {
                    analysis.beatgrid_offset_seconds
                } else {
                    analysis
                        .neural_rhythm
                        .as_ref()
                        .and_then(|g| {
                            analysis
                                .manual_grid_anchor_index
                                .and_then(|i| g.beats_seconds.get(i))
                        })
                        .copied()
                }
                .or(analysis.beatgrid_offset_seconds)
                .unwrap_or(0.0);
                analysis.manual_grid_anchor_index = Some(crate::grid::nearest_index(
                    &rhythm.beats_seconds,
                    anchor_time,
                ));
            }
            if analysis.bpm_source.as_deref() != Some("manual")
                && analysis.bpm_source.as_deref() != Some("tag")
            {
                analysis.bpm = Some(rhythm.bpm);
                analysis.bpm_source = Some("beat_this".into());
                analysis.bpm_confidence = rhythm.beat_confidence;
                analysis.bpm_ambiguous = neural_tempo_ambiguous(&rhythm);
            }
            if analysis.beatgrid_source.as_deref() != Some("manual") {
                analysis.beatgrid_offset_seconds = rhythm
                    .downbeats_seconds
                    .first()
                    .copied()
                    .or_else(|| rhythm.beats_seconds.first().copied());
                analysis.beatgrid_source = Some("beat_this".into());
                analysis.beatgrid_confidence = Some(rhythm.beat_confidence);
            }
            analysis.rhythm_error = None;
            analysis.neural_rhythm = Some(rhythm);
        }
        Err(error) => analysis.rhythm_error = Some(error),
    }
}

pub(crate) fn demo_track(deck: usize) -> Arc<TrackData> {
    let rate = 48_000;
    let bpm = if deck == 0 { 120.0 } else { 124.0 };
    let seconds = 32.0;
    let mut pcm = Vec::with_capacity((rate as f64 * seconds) as usize);
    let freq = if deck == 0 { 110.0 } else { 164.81 };
    for i in 0..(rate as f64 * seconds) as usize {
        let t = i as f64 / rate as f64;
        let beat_time = t % (60.0 / bpm);
        let kick = ((2.0
            * std::f64::consts::PI
            * (55.0 * beat_time + 15.0 * (1.0 - (-beat_time * 35.0).exp())))
        .sin()
            * (-beat_time * 22.0).exp()) as f32;
        let bass = (2.0 * std::f64::consts::PI * freq * t).sin() as f32 * 0.12;
        let hat_time = t % (30.0 / bpm);
        let noise = (((i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) & 32767) as f32
            / 16384.0
            - 1.0)
            * (-hat_time as f32 * 100.0).exp()
            * 0.04;
        pcm.push([kick * 0.38 + bass + noise, kick * 0.38 + bass - noise]);
    }
    let waveform = audio_analysis::analyze_waveform(&pcm, rate, 4096);
    let analysis = TrackAnalysis {
        neural_rhythm: None,
        rhythm_error: None,
        manual_grid_anchor_index: None,
        id: format!("demo:{deck}"),
        path: format!("demo:{deck}"),
        title: format!("Demo {} — {bpm:.0} BPM", deck + 1),
        artist: Some("Original synthesized test loop".into()),
        duration_seconds: seconds,
        sample_rate: rate,
        channels: 2,
        recording_allowed: true,
        bpm: Some(bpm),
        bpm_confidence: 1.0,
        analysis_version: audio_analysis::ANALYSIS_VERSION,
        bpm_candidates: Vec::new(),
        bpm_ambiguous: false,
        bpm_source: Some("synthetic".into()),
        beatgrid_offset_seconds: Some(0.0),
        beatgrid_confidence: Some(1.0),
        beatgrid_source: Some("synthetic".into()),
        key_candidates: Vec::new(),
        key: None,
        key_confidence: None,
        camelot: None,
        waveform: waveform.peaks,
        waveform_bands: Some(waveform.bands),
        beat_model: None,

        cue_points: Vec::new(),
        source_fingerprint: None,
        source_release: None,
        audio_measurements: None,
        analysis_stages: Default::default(),
    };
    Arc::new(TrackData {
        analysis_pending: false,
        samples: pcm,
        sample_rate: rate,
        analysis,
    })
}

pub(crate) fn sample_track(slot: usize) -> Arc<TrackData> {
    let rate = 48000_u32;
    let sound = slot % 8;
    let length = if sound == 7 { rate * 2 } else { rate };
    let mut samples = Vec::with_capacity(length as usize);
    for i in 0..length {
        let t = i as f64 / rate as f64;
        let noise = ((i.wrapping_mul(1664525).wrapping_add(1013904223) >> 16) & 32767) as f64
            / 16384.0
            - 1.0;
        let value = match sound {
            0 => {
                (2.0 * std::f64::consts::PI * (50.0 * t + 3.0 * (1.0 - (-t * 35.0).exp()))).sin()
                    * (-t * 12.0).exp()
                    * 0.75
            }
            1 => {
                noise * (-t * 18.0).exp() * 0.55
                    + (2.0 * std::f64::consts::PI * 180.0 * t).sin() * (-t * 25.0).exp() * 0.2
            }
            2 => noise * (-t * 70.0).exp() * 0.35,
            3 => {
                noise
                    * (-t * 20.0).exp()
                    * (0.5 + 0.5 * (2.0 * std::f64::consts::PI * 110.0 * t).cos())
                    * 0.45
            }
            4 => (2.0 * std::f64::consts::PI * 110.0 * t).sin() * (-t * 5.0).exp() * 0.5,
            5 => (2.0 * std::f64::consts::PI * 330.0 * t).sin() * (-t * 25.0).exp() * 0.45,
            6 => (2.0 * std::f64::consts::PI * 1400.0 * t).sin() * (-t * 80.0).exp() * 0.4,
            _ => noise * (-t * 3.0).exp() * 0.4,
        };
        samples.push([value as f32; 2]);
    }
    let names = [
        "Kick",
        "Snare",
        "Closed hat",
        "Clap",
        "Bass",
        "Percussion",
        "Rim",
        "Crash",
    ];
    Arc::new(TrackData {
        analysis_pending: false,
        samples,
        sample_rate: rate,
        analysis: TrackAnalysis {
            neural_rhythm: None,
            rhythm_error: None,
            manual_grid_anchor_index: None,
            id: format!("sample:{slot}"),
            path: format!("sample:{slot}"),
            title: names[sound].into(),
            artist: None,
            duration_seconds: length as f64 / rate as f64,
            sample_rate: rate,
            channels: 2,
            recording_allowed: true,
            bpm: None,
            bpm_confidence: 0.0,
            analysis_version: audio_analysis::ANALYSIS_VERSION,
            bpm_candidates: Vec::new(),
            bpm_ambiguous: false,
            bpm_source: None,
            beatgrid_offset_seconds: None,
            beatgrid_confidence: None,
            beatgrid_source: None,
            key_candidates: Vec::new(),
            waveform_bands: None,
            key: None,
            key_confidence: None,
            camelot: None,
            waveform: Vec::new(),
            beat_model: None,

            cue_points: Vec::new(),
            source_fingerprint: None,
            source_release: None,
            audio_measurements: None,
            analysis_stages: Default::default(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wav_fixture() -> Vec<u8> {
        let rate = 8000_u32;
        let n = rate * 12;
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&(36 + n * 2).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16_u32.to_le_bytes());
        file.extend_from_slice(&1_u16.to_le_bytes());
        file.extend_from_slice(&1_u16.to_le_bytes());
        file.extend_from_slice(&rate.to_le_bytes());
        file.extend_from_slice(&(rate * 2).to_le_bytes());
        file.extend_from_slice(&2_u16.to_le_bytes());
        file.extend_from_slice(&16_u16.to_le_bytes());
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(n * 2).to_le_bytes());
        for i in 0..n {
            let phase = i % (rate / 2);
            let v = if phase < 200 {
                ((i as f64 * 0.37).sin() * 20000.0) as i16
            } else {
                0
            };
            file.extend_from_slice(&v.to_le_bytes());
        }
        file
    }
    #[test]
    fn decode_real_wav_and_detect_pulses() {
        let path = std::env::temp_dir().join(format!("deck-engine-{}.wav", std::process::id()));
        std::fs::write(&path, wav_fixture()).unwrap();
        let decoded = decode_file(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(decoded.samples.len(), 96_000);
        assert_eq!(decoded.sample_rate, 8000);
        assert_eq!(decoded.samples[20][0], decoded.samples[20][1]);
        for (i, frame) in decoded.samples.iter().enumerate() {
            let value = if i % 4000 < 200 {
                ((i as f64 * 0.37).sin() * 20000.0) as i16
            } else {
                0
            } as f32
                / 32768.0;
            assert_eq!(*frame, [value, value]);
        }
        assert_eq!(decoded.analysis.waveform.len(), 4096);
        assert!((decoded.analysis.bpm.unwrap() - 120.0).abs() < 2.0);
    }
    #[test]
    fn reused_decode_buffer_preserves_stereo_and_partial_final_packet() {
        let path =
            std::env::temp_dir().join(format!("deck-stereo-reuse-{}.wav", std::process::id()));
        let frames = 12_347_u32;
        let expected: Vec<_> = (0..frames)
            .map(|i| [(i.wrapping_mul(37) as i16), (i.wrapping_mul(113) as i16)])
            .collect();
        let mut bytes = wav_fixture()[..44].to_vec();
        bytes[4..8].copy_from_slice(&(36 + frames * 4).to_le_bytes());
        bytes[22..24].copy_from_slice(&2_u16.to_le_bytes());
        bytes[28..32].copy_from_slice(&32_000_u32.to_le_bytes());
        bytes[32..34].copy_from_slice(&4_u16.to_le_bytes());
        bytes[40..44].copy_from_slice(&(frames * 4).to_le_bytes());
        for frame in &expected {
            for sample in frame {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        std::fs::write(&path, bytes).unwrap();
        let decoded = decode_file_playable(&path, None).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(decoded.samples.len(), expected.len());
        for (actual, expected) in decoded.samples.iter().zip(expected) {
            assert_eq!(*actual, expected.map(|v| f32::from(v) / 32768.0));
        }
    }
    #[test]
    fn metadata_import_and_playable_load_defer_analysis_and_validate_cache() {
        let path = std::env::temp_dir().join(format!("deck-playable-{}.wav", std::process::id()));
        let mut bytes = wav_fixture();
        std::fs::write(&path, &bytes).unwrap();
        let metadata = probe_file(&path).unwrap();
        assert_eq!(metadata.duration_seconds, 12.0);
        assert_eq!(metadata.sample_rate, 8000);
        assert!(metadata.waveform.is_empty());
        assert!(metadata.source_fingerprint.is_none());
        assert_eq!(metadata.analysis_version, 0);
        let fresh = decode_file_playable(&path, Some(metadata)).unwrap();
        assert_eq!(fresh.samples.len(), 96_000);
        assert!(fresh.analysis.waveform.is_empty());
        assert!(fresh.analysis.analysis_stages.neural.is_none());

        let engine = crate::EngineHandle::new().unwrap();
        let loaded = engine.load_track_cached(0, &path, None).unwrap();
        assert_eq!(loaded.analysis_version, 0);
        assert!(engine
            .command(crate::EngineCommand::Play { deck: 0 })
            .unwrap_err()
            .contains("analysis"));
        assert!(engine
            .command(crate::EngineCommand::CuePress {
                deck: 0,
                pressed: true
            })
            .is_err());
        assert!(engine
            .command(crate::EngineCommand::TriggerHotCue { deck: 0, index: 1 })
            .is_err());
        engine
            .command(crate::EngineCommand::Seek {
                deck: 0,
                seconds: 2.0,
            })
            .unwrap();
        engine
            .command(crate::EngineCommand::SetHotCue { deck: 0, index: 1 })
            .unwrap();
        engine
            .command(crate::EngineCommand::SetBpm {
                deck: 0,
                bpm: 128.0,
            })
            .unwrap();
        engine
            .command(crate::EngineCommand::SetBeatgrid {
                deck: 0,
                offset_seconds: 0.12,
                anchor_index: None,
            })
            .unwrap();
        engine
            .wait_for_fence(engine.fence().unwrap(), std::time::Duration::from_secs(2))
            .unwrap();
        let before = engine.snapshot().decks[0].clone();
        let mut analyzed = engine.complete_loaded_analysis(0).unwrap();
        let after = engine.snapshot().decks[0].clone();
        assert_eq!(before.position_seconds, after.position_seconds);
        assert_eq!(before.playing, after.playing);
        assert_eq!(before.hot_cues, after.hot_cues);
        assert_eq!(after.bpm, Some(128.0));
        assert_eq!(after.beatgrid_offset_seconds, 0.12);
        assert!(analysis_current(&analyzed));
        // A fresh source also receives its statistical tempo after the worker
        // finishes, even if neural inference is unavailable in this test build.
        let fresh_engine = crate::EngineHandle::new().unwrap();
        fresh_engine.load_track_cached(0, &path, None).unwrap();
        let completed = fresh_engine.complete_loaded_analysis(0).unwrap();
        fresh_engine
            .wait_for_fence(
                fresh_engine.fence().unwrap(),
                std::time::Duration::from_secs(2),
            )
            .unwrap();
        assert_eq!(fresh_engine.snapshot().decks[0].bpm, completed.bpm);
        analyzed.waveform[0] = 0.123;
        let cached = decode_file_playable(&path, Some(analyzed.clone())).unwrap();
        assert_eq!(cached.analysis.waveform[0], 0.123);
        assert!(analysis_current(&cached.analysis));
        bytes[44..46].copy_from_slice(&1234_i16.to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        let replaced = decode_file_playable(&path, Some(analyzed)).unwrap();
        assert!(replaced.analysis.waveform.is_empty());
        assert!(!analysis_current(&replaced.analysis));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn malformed_id3_is_skipped_for_metadata_and_playback_without_changing_the_file() {
        let path =
            std::env::temp_dir().join(format!("deck-damaged-id3-{}.wav", std::process::id()));
        let mut bytes =
            b"ID3\x04\x00\x00\x00\x00\x00\x0cTIT2\x00\x00\x00\x02\x00\x00\xff\x00".to_vec();
        // Consecutive malformed tags occur in downloaded packs too.
        let second = bytes.clone();
        bytes.extend_from_slice(&second);
        bytes.extend_from_slice(&wav_fixture());
        std::fs::write(&path, &bytes).unwrap();
        let metadata = probe_file(&path).unwrap();
        assert_eq!(metadata.duration_seconds, 12.0);
        assert!(metadata.waveform.is_empty());
        let track = decode_file_playable(&path, None).unwrap();
        assert_eq!(track.samples.len(), 96_000);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, []).unwrap();
        assert!(probe_file(&path).unwrap_err().contains("empty"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn embedded_front_cover_is_read_without_audio_analysis() {
        let path = std::env::temp_dir().join(format!("deck-cover-{}.wav", std::process::id()));
        let image = b"embedded-cover-fixture";
        let mut payload = b"\x00image/png\x00\x03\x00".to_vec();
        payload.extend_from_slice(image);
        let mut frame = b"APIC".to_vec();
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&[0, 0]);
        frame.extend_from_slice(&payload);
        let len = frame.len() as u32;
        let mut bytes = b"ID3\x03\x00\x00".to_vec();
        bytes.extend_from_slice(&[
            ((len >> 21) & 127) as u8,
            ((len >> 14) & 127) as u8,
            ((len >> 7) & 127) as u8,
            (len & 127) as u8,
        ]);
        bytes.extend_from_slice(&frame);
        bytes.extend_from_slice(&wav_fixture());
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(embedded_artwork(&path).unwrap().unwrap(), image);
        assert!(probe_file(&path).unwrap().source_fingerprint.is_none());
        std::fs::remove_file(path).unwrap();
    }

    struct FragmentedSource(std::io::Cursor<Vec<u8>>);
    impl std::io::Read for FragmentedSource {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let count = buffer.len().min(7);
            std::io::Read::read(&mut self.0, &mut buffer[..count])
        }
    }
    impl std::io::Seek for FragmentedSource {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            std::io::Seek::seek(&mut self.0, pos)
        }
    }
    impl MediaSource for FragmentedSource {
        fn is_seekable(&self) -> bool {
            true
        }
        fn byte_len(&self) -> Option<u64> {
            Some(self.0.get_ref().len() as u64)
        }
    }
    #[test]
    fn provider_short_reads_feed_native_transport_without_a_file() {
        let source = FragmentedSource(std::io::Cursor::new(wav_fixture()));
        let info = AudioSourceInfo {
            id: "fixture:owned-track".into(),
            location: "fixture:owned-track".into(),
            title: "Provider test".into(),
            artist: Some("Original test audio".into()),
            extension: Some("wav".into()),
            release_identity: None,
        };
        let engine = crate::EngineHandle::new().unwrap();
        let analysis = engine.load_source(0, Box::new(source), info).unwrap();
        assert_eq!(analysis.id, "fixture:owned-track");
        assert_eq!(analysis.title, "Provider test");
        assert_eq!(analysis.duration_seconds, 12.);
        assert_eq!(analysis.waveform.len(), 4096);
        engine
            .command(crate::EngineCommand::Seek {
                deck: 0,
                seconds: 4.,
            })
            .unwrap();
        for _ in 0..100 {
            if engine.snapshot().decks[0].position_seconds == 4. {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(engine.snapshot().decks[0].position_seconds, 4.);
        assert_eq!(
            engine.snapshot().decks[0].track.as_ref().unwrap().path,
            "fixture:owned-track"
        );
    }
}

#[cfg(test)]
mod tag_provenance_tests {
    use super::*;
    #[test]
    fn id3_tempo_is_preserved_over_statistical_analysis() {
        let value = b"\x00133.75";
        let mut tag = Vec::new();
        tag.extend_from_slice(b"TBPM");
        tag.extend_from_slice(&(value.len() as u32).to_be_bytes());
        tag.extend_from_slice(&[0, 0]);
        tag.extend_from_slice(value);
        let size = tag.len() as u32;
        let mut file = Vec::new();
        file.extend_from_slice(b"ID3\x03\x00\x00");
        file.extend_from_slice(&[
            ((size >> 21) & 127) as u8,
            ((size >> 14) & 127) as u8,
            ((size >> 7) & 127) as u8,
            (size & 127) as u8,
        ]);
        file.extend(tag);
        // Original silence: MPEG1 LayerIII, 128kbps/44.1kHz, empty side info/data.
        for _ in 0..8 {
            file.extend_from_slice(&[0xff, 0xfb, 0x90, 0x00]);
            file.extend(std::iter::repeat_n(0_u8, 413));
        }
        let track = decode_source(
            Box::new(std::io::Cursor::new(file)),
            AudioSourceInfo {
                id: "fixture:tagged".into(),
                location: "memory:tagged".into(),
                title: "Tagged silence".into(),
                artist: None,
                extension: Some("mp3".into()),
                release_identity: None,
            },
        )
        .unwrap();
        assert_eq!(track.analysis.bpm, Some(133.75));
        assert_eq!(track.analysis.bpm_source.as_deref(), Some("tag"));
        assert!(track
            .samples
            .iter()
            .all(|f| f[0].abs() < 1e-6 && f[1].abs() < 1e-6));
    }
}

#[cfg(test)]
mod cached_phase_upgrade_tests {
    use super::*;
    fn fixture() -> (Vec<[f32; 2]>, AudioSourceInfo, TrackAnalysis) {
        let rate = 8000;
        let bpm = 127.3;
        let offset = 0.173;
        let pcm = (0..rate * 12)
            .map(|i| {
                let t = i as f64 / rate as f64;
                if t < offset {
                    return [0.0; 2];
                }
                let phase = (t - offset).rem_euclid(60.0 / bpm);
                let value = (2.0 * std::f64::consts::PI * 75.0 * phase).sin()
                    * (-phase * 55.0).exp()
                    * 0.55;
                [value as f32; 2]
            })
            .collect::<Vec<_>>();
        let info = AudioSourceInfo {
            id: "phase-cache".into(),
            location: "ram:phase-cache".into(),
            title: "Phase cache fixture".into(),
            artist: None,
            extension: None,
            release_identity: None,
        };
        let mut cached = demo_track(0).analysis.clone();
        cached.id = info.id.clone();
        cached.path = info.location.clone();
        cached.sample_rate = rate;
        cached.duration_seconds = 12.0;
        cached.analysis_version = 2;
        cached.bpm = Some(bpm);
        cached.bpm_source = Some("analysis".into());
        cached.key = Some("cached key".into());
        cached.waveform = vec![0.432; 4096];
        cached.beatgrid_offset_seconds = None;
        cached.beatgrid_confidence = None;
        cached.beatgrid_source = None;
        (pcm, info, cached)
    }
    #[test]
    fn invalidated_release_cache_recomputes_waveform_and_preserves_explicit_overrides() {
        let (pcm, info, mut cached) = fixture();
        cached.analysis_version = 0;
        cached.bpm = Some(130.0);
        cached.bpm_source = Some("manual".into());
        cached.beatgrid_offset_seconds = Some(0.297);
        cached.beatgrid_source = Some("manual".into());
        let t = prepare_pcm_cached(pcm, 8000, info, false, Some(cached)).unwrap();
        assert_eq!(t.analysis.bpm, Some(130.0));
        assert_eq!(t.analysis.beatgrid_offset_seconds, Some(0.297));
        assert_eq!(t.analysis.beatgrid_source.as_deref(), Some("manual"));
        assert_ne!(t.analysis.waveform, vec![0.432; 4096]);
        assert!(!t.analysis.recording_allowed);
    }
    #[test]
    fn sustained_octave_switch_is_reported_separately_from_model_activation_scores() {
        let mut r = audio_analysis::NeuralRhythm {
            beats_seconds: vec![],
            downbeats_seconds: vec![],
            bpm: 120.0,
            beat_confidence: 0.99,
            downbeat_confidence: 0.99,
            model: "fixture".into(),
            analysis_seconds: 0.0,
            bar_evidence: Vec::new(),
            beats_per_bar: None,
        };
        let mut t = 0.0;
        for _ in 0..180 {
            r.beats_seconds.push(t);
            t += 0.5;
        }
        assert!(!neural_tempo_ambiguous(&r));
        for _ in 0..90 {
            r.beats_seconds.push(t);
            t += 0.25;
        }
        assert!(neural_tempo_ambiguous(&r));
    }
    #[test]
    fn legacy_without_content_identity_is_recomputed_once() {
        let (pcm, info, cached) = fixture();
        let first =
            prepare_pcm_cached(pcm.clone(), 8000, info.clone(), false, Some(cached)).unwrap();
        assert!(first.analysis.source_fingerprint.is_some());
        assert_ne!(first.analysis.key.as_deref(), Some("cached key"));
        assert_ne!(first.analysis.waveform, vec![0.432; 4096]);
        let second =
            prepare_pcm_cached(pcm, 8000, info, false, Some(first.analysis.clone())).unwrap();
        assert_eq!(first.analysis.waveform, second.analysis.waveform);
        assert_eq!(
            first.analysis.analysis_stages,
            second.analysis.analysis_stages
        );
        assert!((second.analysis.beatgrid_offset_seconds.unwrap() - 0.173).abs() < 0.012);
    }
    #[test]
    fn verified_stage_hit_skips_analysis_and_partial_upgrade_keeps_other_stages_and_manual_grid() {
        let (pcm, info, _) = fixture();
        let first = prepare_pcm_cached(pcm.clone(), 8000, info.clone(), false, None).unwrap();
        let mut cached = first.analysis.clone();
        cached.key = Some("retained key".into());
        cached.waveform[0] = 0.321;
        cached.bpm = Some(130.);
        cached.bpm_source = Some("manual".into());
        cached.beatgrid_source = Some("manual".into());
        cached.beatgrid_offset_seconds = Some(0.297);
        let hit = prepare_pcm_cached(pcm.clone(), 8000, info.clone(), false, Some(cached.clone()))
            .unwrap();
        assert_eq!(hit.analysis.key.as_deref(), Some("retained key"));
        assert_eq!(hit.analysis.waveform[0], 0.321);
        assert_eq!(
            hit.analysis.analysis_stages.neural,
            cached.analysis_stages.neural
        );
        cached.analysis_stages.waveform = None;
        cached.waveform.clear();
        let upgrade = prepare_pcm_cached(pcm, 8000, info, false, Some(cached.clone())).unwrap();
        assert_eq!(upgrade.analysis.waveform.len(), 4096);
        assert_eq!(upgrade.analysis.key, cached.key);
        assert_eq!(
            upgrade.analysis.analysis_stages.key,
            cached.analysis_stages.key
        );
        assert_eq!(
            upgrade.analysis.analysis_stages.neural,
            cached.analysis_stages.neural
        );
        assert_eq!(upgrade.analysis.bpm, Some(130.));
        assert_eq!(upgrade.analysis.beatgrid_offset_seconds, Some(0.297));
    }
    #[test]
    fn concurrent_page_and_deck_share_derived_stages_and_terminal_failures() {
        let (pcm, info, _) = fixture();
        let samples = Arc::new(pcm);
        let barrier = Arc::new(std::sync::Barrier::new(4));
        let jobs: Vec<_> = (0..4)
            .map(|i| {
                let (samples, barrier, mut info) = (samples.clone(), barrier.clone(), info.clone());
                info.location = format!("ram:alias-{i}");
                std::thread::spawn(move || {
                    barrier.wait();
                    analyze_pcm_cached(&samples, 8000, &info, false, None)
                })
            })
            .collect();
        let records: Vec<_> = jobs.into_iter().map(|j| j.join().unwrap()).collect();
        for r in &records[1..] {
            assert_eq!(r.source_fingerprint, records[0].source_fingerprint);
            assert_eq!(r.analysis_stages, records[0].analysis_stages);
            assert_eq!(r.waveform, records[0].waveform);
        }
        let mut partial = records[0].clone();
        analyze_beats(&samples, 8000, &mut partial, &|| false).unwrap_err();
        assert!(beat_stage_current(&partial));
        let cached = partial.analysis_stages.clone();
        let _ = analyze_beats(&samples, 8000, &mut partial, &|| false);
        assert_eq!(partial.analysis_stages, cached);
        retry_neural(&mut partial);
        assert!(!beat_stage_current(&partial));
    }
    fn legacy_spotify_fixture() -> (Vec<[f32; 2]>, AudioSourceInfo, TrackAnalysis) {
        let (pcm, mut info, mut a) = fixture();
        let uri = "spotify:track:5ImCVtO1gvcD1ttdG5SrQT";
        info.id = uri.into();
        info.location = uri.into();
        info.release_identity = Some(uri.into());
        a.id = uri.into();
        a.path = uri.into();
        a.recording_allowed = false;
        a.analysis_version = audio_analysis::ANALYSIS_VERSION;
        a.analysis_stages = Default::default();
        a.neural_rhythm = Some(audio_analysis::NeuralRhythm {
            beats_seconds: (0..24).map(|i| i as f64 * 0.5).collect(),
            downbeats_seconds: (0..6).map(|i| i as f64 * 2.).collect(),
            bpm: 120., beat_confidence: 0.9, downbeat_confidence: 0.9,
            model: "Beat This! final0 FP32 + DBN 3/4/5/7 55-300 BPM + acoustic bar/pulse/phase evidence + bounded logit timing / beat-this 1.1.0 / RTen 0.24.0".into(),
            analysis_seconds: 987.654, bar_evidence: vec![], beats_per_bar: Some(4),
        });
        a.bpm = Some(120.);
        a.bpm_source = Some("beat_this".into());
        a.beatgrid_offset_seconds = Some(0.);
        a.beatgrid_source = Some("beat_this".into());
        (pcm, info, a)
    }
    #[test]
    fn persisted_analysis_reuses_every_stage_in_a_fresh_process() {
        const CHILD_CACHE: &str = "PULSEDECK_TEST_PERSISTED_ANALYSIS";
        let (pcm, info, legacy) = legacy_spotify_fixture();
        if let Some(path) = std::env::var_os(CHILD_CACHE) {
            // A fresh process has empty StageMemos. Only the durable record can
            // prevent expensive work; an in-process memo must not mask a miss.
            assert_eq!(analysis_cache_counts(), [0; 4]);
            let saved: TrackAnalysis =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            assert!(
                beat_stage_current(&saved),
                "serialized analysis invalidated neural cache"
            );
            let mut restored = analyze_pcm_cached(&pcm, 8000, &info, false, Some(saved.clone()));
            assert_eq!(
                take_analysis_diagnostics().unwrap().cache_decision,
                "content_match"
            );
            analyze_beats(&pcm, 8000, &mut restored, &|| false).unwrap();
            assert_eq!(analysis_cache_counts(), [0; 4]);
            assert_eq!(restored.analysis_stages, saved.analysis_stages);
            assert_eq!(restored.neural_rhythm.unwrap().analysis_seconds, 987.654);
            return;
        }
        let mut saved = analyze_pcm_cached(&pcm, 8000, &info, false, Some(legacy));
        // Real analysis values whose least significant bit can change when
        // parsed without serde_json's accurate float roundtrip support.
        saved.bpm = Some(120.00100300902709);
        saved.beatgrid_offset_seconds = Some(0.04245912656188011);
        let path = std::env::temp_dir().join(format!(
            "pulsedeck-cache-restart-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "decode::cached_phase_upgrade_tests::persisted_analysis_reuses_every_stage_in_a_fresh_process", "--nocapture"])
            .env(CHILD_CACHE, &path)
            .output().unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(
            result.status.success(),
            "fresh-process reuse failed:\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    #[test]
    fn immutable_spotify_legacy_migration_preserves_origin_and_skips_inference_on_subsequent_hits()
    {
        let (pcm, info, mut legacy) = legacy_spotify_fixture();
        legacy.bpm = Some(121.);
        legacy.bpm_source = Some("manual".into());
        legacy.beatgrid_offset_seconds = Some(0.5);
        legacy.beatgrid_source = Some("manual".into());
        legacy.manual_grid_anchor_index = Some(1);
        let first = analyze_pcm_cached(&pcm, 8000, &info, false, Some(legacy.clone()));
        let timing = take_analysis_diagnostics().unwrap();
        assert_eq!(timing.cache_decision, "legacy_spotify_uri_adopted");
        assert_eq!(timing.fingerprint_passes, 1);
        assert!(first
            .source_fingerprint
            .as_ref()
            .unwrap()
            .starts_with("decoded-sha256-v1:"));
        assert_eq!(first.waveform, legacy.waveform);
        assert_eq!(first.key, legacy.key);
        assert_eq!(
            first.neural_rhythm.as_ref().unwrap().analysis_seconds,
            987.654
        );
        assert_eq!(first.bpm, Some(121.));
        assert_eq!(first.manual_grid_anchor_index, Some(1));
        assert_eq!(first.beatgrid_offset_seconds, Some(0.5));
        assert_eq!(
            first
                .analysis_stages
                .neural
                .as_ref()
                .unwrap()
                .reason
                .as_deref(),
            Some(LEGACY_SPOTIFY_ORIGIN)
        );
        let second = analyze_pcm_cached(&pcm, 8000, &info, false, Some(first.clone()));
        assert_eq!(
            take_analysis_diagnostics().unwrap().cache_decision,
            "content_match"
        );
        assert_eq!(second.analysis_stages, first.analysis_stages);
        assert_eq!(
            second.neural_rhythm.as_ref().unwrap().analysis_seconds,
            987.654
        );
        assert_eq!(second.waveform, legacy.waveform);
        let mut changed = pcm;
        changed[4000][0] += 0.2;
        let replaced = analyze_pcm_cached(&changed, 8000, &info, false, Some(second));
        assert_eq!(
            take_analysis_diagnostics().unwrap().cache_decision,
            "content_mismatch"
        );
        assert_ne!(replaced.source_fingerprint, first.source_fingerprint);
        assert_ne!(replaced.waveform, legacy.waveform);
        assert!(replaced.neural_rhythm.is_none()); // Test build omits inference.
        assert_ne!(
            replaced.analysis_stages.neural.unwrap().reason.as_deref(),
            Some(LEGACY_SPOTIFY_ORIGIN)
        );
        assert_eq!(replaced.bpm, Some(121.));
    }
    #[test]
    fn legacy_adoption_rejects_relinked_releases_obsolete_models_and_invalid_events() {
        let (pcm, mut info, legacy) = legacy_spotify_fixture();
        info.release_identity = Some("spotify:track:3qOYnxC8CGUMz0Dg81Ov8S".into());
        assert!(!legacy_spotify_reusable(
            &legacy,
            8000,
            pcm.len(),
            &info,
            false
        ));
        info.release_identity = Some(info.location.clone());
        let mut bad = legacy.clone();
        bad.neural_rhythm.as_mut().unwrap().model = "obsolete".into();
        assert!(!legacy_spotify_reusable(
            &bad,
            8000,
            pcm.len(),
            &info,
            false
        ));
        bad = legacy.clone();
        bad.neural_rhythm.as_mut().unwrap().beats_seconds[1] = f64::NAN;
        assert!(!legacy_spotify_reusable(
            &bad,
            8000,
            pcm.len(),
            &info,
            false
        ));
        bad = legacy.clone();
        bad.analysis_stages.neural = Some(StageStamp::ready(99, "explicitly-stale".into()));
        assert!(!legacy_spotify_reusable(
            &bad,
            8000,
            pcm.len(),
            &info,
            false
        ));
        bad = legacy.clone();
        bad.source_fingerprint = Some("different-content".into());
        assert!(!legacy_spotify_reusable(
            &bad,
            8000,
            pcm.len(),
            &info,
            false
        ));
        bad = legacy;
        bad.source_release = Some("spotify:track:3qOYnxC8CGUMz0Dg81Ov8S".into());
        assert!(!legacy_spotify_reusable(
            &bad,
            8000,
            pcm.len(),
            &info,
            false
        ));
    }
    #[test]
    fn same_length_content_replacement_and_changed_release_invalidate_exactly_once() {
        let (mut pcm, mut info, _) = fixture();
        info.release_identity = Some("spotify:track:releaseA".into());
        let first = prepare_pcm_cached(pcm.clone(), 8000, info.clone(), false, None).unwrap();
        pcm[4000][0] += 0.2;
        let changed = prepare_pcm_cached(
            pcm.clone(),
            8000,
            info.clone(),
            false,
            Some(first.analysis.clone()),
        )
        .unwrap();
        assert_ne!(
            changed.analysis.source_fingerprint,
            first.analysis.source_fingerprint
        );
        let stable = prepare_pcm_cached(
            pcm.clone(),
            8000,
            info.clone(),
            false,
            Some(changed.analysis.clone()),
        )
        .unwrap();
        assert_eq!(
            stable.analysis.analysis_stages,
            changed.analysis.analysis_stages
        );
        info.release_identity = Some("spotify:track:releaseB".into());
        let relink = prepare_pcm_cached(
            pcm.clone(),
            8000,
            info.clone(),
            false,
            Some(stable.analysis.clone()),
        )
        .unwrap();
        assert_ne!(
            relink.analysis.source_fingerprint,
            stable.analysis.source_fingerprint
        );
        let repeated =
            prepare_pcm_cached(pcm, 8000, info, false, Some(relink.analysis.clone())).unwrap();
        assert_eq!(
            repeated.analysis.analysis_stages,
            relink.analysis.analysis_stages
        );
    }
    #[test]
    fn manual_grid_survives_cache_upgrade() {
        let (pcm, info, mut cached) = fixture();
        cached.beatgrid_offset_seconds = Some(0.297);
        cached.beatgrid_confidence = Some(1.0);
        cached.beatgrid_source = Some("manual".into());
        let track = prepare_pcm_cached(pcm, 8000, info, false, Some(cached)).unwrap();
        assert_eq!(
            track.analysis.analysis_version,
            audio_analysis::ANALYSIS_VERSION
        );
        assert_eq!(track.analysis.beatgrid_offset_seconds, Some(0.297));
        assert_eq!(track.analysis.beatgrid_source.as_deref(), Some("manual"));
    }
}

#[cfg(test)]
mod beat_model_tests {
    use super::*;
    use audio_analysis::{BeatModelStage, ModelGrid};

    fn stage(bpm: f64, offset: f64, beats: usize) -> BeatModelStage {
        BeatModelStage {
            version: audio_analysis::BEAT_MODEL_VERSION,
            beats_seconds: (0..beats)
                .map(|k| (offset + k as f64 * 60.0 / bpm) as f32)
                .collect(),
            downbeats_seconds: vec![offset as f32],
            grid: Some(ModelGrid {
                bpm,
                downbeat_offset_seconds: offset,
                on_grid: 0.9,
                bar_agreement: 0.8,
            }),
        }
    }

    fn analysis(bpm: f64, source: &str) -> TrackAnalysis {
        let mut a = demo_track(0).analysis.clone();
        a.bpm = Some(bpm);
        a.bpm_source = Some(source.into());
        a.bpm_confidence = 0.6;
        a.beatgrid_offset_seconds = Some(0.1);
        a.beatgrid_source = Some("analysis".into());
        a
    }

    #[test]
    fn model_grid_replaces_dsp_and_flags_half_double_disagreement() {
        let mut a = analysis(70.99, "analysis");
        apply_beat_model(&mut a, stage(142.03, 0.11, 300));
        assert_eq!(a.bpm, Some(142.03));
        assert_eq!(a.bpm_source.as_deref(), Some("model"));
        assert!(a.bpm_ambiguous);
        assert_eq!(a.bpm_candidates[0].bpm, 142.03);
        assert!(a.bpm_candidates.iter().any(|c| c.bpm == 70.99));
        assert_eq!(a.beatgrid_offset_seconds, Some(0.11));
        assert_eq!(a.beatgrid_source.as_deref(), Some("model"));
        assert_eq!(a.beatgrid_confidence, Some(0.8));
        assert!(a.beat_model.is_some());
    }

    #[test]
    fn agreeing_tag_is_refined_without_ambiguity() {
        let mut a = analysis(124.0, "tag");
        apply_beat_model(&mut a, stage(123.97, 0.4, 300));
        assert_eq!(a.bpm, Some(123.97));
        assert!(!a.bpm_ambiguous);
        assert_eq!(a.bpm_candidates.len(), 1);
    }

    #[test]
    fn manual_values_are_kept_and_grid_follows_only_a_compatible_bpm() {
        let mut half = analysis(71.0, "manual");
        apply_beat_model(&mut half, stage(142.03, 0.11, 300));
        assert_eq!(
            (half.bpm, half.bpm_source.as_deref()),
            (Some(71.0), Some("manual"))
        );
        assert_eq!(half.beatgrid_offset_seconds, Some(0.11));

        let mut other = analysis(100.0, "manual");
        apply_beat_model(&mut other, stage(142.03, 0.11, 300));
        assert_eq!(other.beatgrid_offset_seconds, Some(0.1));
        assert_eq!(other.beatgrid_source.as_deref(), Some("analysis"));

        let mut grid = analysis(120.0, "analysis");
        grid.beatgrid_source = Some("manual".into());
        apply_beat_model(&mut grid, stage(120.0, 0.3, 300));
        assert_eq!(grid.bpm_source.as_deref(), Some("model"));
        assert_eq!(grid.beatgrid_offset_seconds, Some(0.1));
    }

    #[test]
    fn unusable_grid_is_recorded_without_replacing_dsp() {
        let mut a = analysis(120.0, "analysis");
        apply_beat_model(&mut a, stage(120.0, 0.3, 10));
        assert_eq!(a.bpm_source.as_deref(), Some("analysis"));
        assert_eq!(a.beatgrid_offset_seconds, Some(0.1));
        assert_eq!(
            a.beat_model.as_ref().map(|s| s.beats_seconds.len()),
            Some(10)
        );
    }
    #[test]
    fn derived_stage_keeps_native_timing_and_manual_provenance() {
        let mut a = analysis(127.0, "manual");
        a.beatgrid_source = Some("manual".into());
        a.manual_grid_anchor_index = Some(3);
        a.neural_rhythm = Some(audio_analysis::NeuralRhythm {
            beats_seconds: vec![0.17, 0.68, 1.23, 1.81, 2.45],
            downbeats_seconds: vec![0.17, 2.45],
            bpm: 120.0,
            beat_confidence: 0.8,
            downbeat_confidence: 0.7,
            model: "fixture".into(),
            analysis_seconds: 0.0,
            bar_evidence: Vec::new(),
            beats_per_bar: Some(4),
        });
        let before = a.neural_rhythm.as_ref().unwrap().beats_seconds.clone();
        apply_beat_model(&mut a, stage(130.0, 0.4, 300));
        assert_eq!(a.bpm, Some(127.0));
        assert_eq!(a.bpm_source.as_deref(), Some("manual"));
        assert_eq!(a.beatgrid_offset_seconds, Some(0.1));
        assert_eq!(a.manual_grid_anchor_index, Some(3));
        assert_eq!(a.neural_rhythm.unwrap().beats_seconds, before);
    }
}
