use crate::model::TrackAnalysis;
use std::{fs::File, path::Path, sync::Arc};
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
}

/// Public display identity for a provider-owned audio source. Never put tokens here.
#[derive(Clone, Debug)]
pub struct AudioSourceInfo {
    pub id: String,
    pub location: String,
    pub title: String,
    pub artist: Option<String>,
    pub extension: Option<String>,
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
    let track = decode_file(path)?;
    if cancel() {
        return Err("Beat analysis cancelled".into());
    }
    let mut analysis = track.analysis.clone();
    match analyze_beats_and_sections(&track.samples, track.sample_rate, &mut analysis, cancel) {
        Ok(()) => Ok((analysis, None)),
        Err(_) if cancel() => Err("Beat analysis cancelled".into()),
        Err(e) => Ok((analysis, Some(e))),
    }
}

/// Run the beat model, merge its grid into `analysis`, then cut sections on
/// the resulting grid from the same spectrogram.
pub fn analyze_beats_and_sections(
    samples: &[[f32; 2]],
    sample_rate: u32,
    analysis: &mut TrackAnalysis,
    cancel: &dyn Fn() -> bool,
) -> Result<(), String> {
    let (stage, spect) =
        audio_analysis::track_beats_with_spectrogram(samples, sample_rate, cancel)?;
    apply_beat_model(analysis, stage);
    analysis.sections = analysis
        .bpm
        .zip(analysis.beatgrid_offset_seconds)
        .filter(|_| grid_knows_beat_one(analysis))
        .and_then(|(bpm, offset)| sections_on(&spect, analysis, bpm, offset));
    Ok(())
}

/// Re-cut sections on a different grid (after a manual edit) without the model.
pub fn refresh_sections(
    samples: &[[f32; 2]],
    sample_rate: u32,
    analysis: &mut TrackAnalysis,
    bpm: f64,
    offset_seconds: f64,
) -> Result<(), String> {
    let spect = audio_analysis::spectrogram(samples, sample_rate)?;
    analysis.sections = sections_on(&spect, analysis, bpm, offset_seconds);
    Ok(())
}

fn sections_on(
    spect: &audio_analysis::Spectrogram,
    analysis: &TrackAnalysis,
    bpm: f64,
    offset_seconds: f64,
) -> Option<audio_analysis::SectionStage> {
    let vocabulary = analysis
        .sections
        .as_ref()
        .and_then(|s| s.vocabulary_override);
    audio_analysis::detect_sections(
        spect,
        analysis.duration_seconds,
        bpm,
        offset_seconds,
        vocabulary,
    )
}

/// Only model and manual grids place their origin on beat 1; the DSP phase
/// is a beat, not a bar, so bars cut from it can start mid-phrase.
fn grid_knows_beat_one(analysis: &TrackAnalysis) -> bool {
    matches!(
        analysis.beatgrid_source.as_deref(),
        Some("model" | "manual")
    )
}

/// Merge a beat-model stage into an analysis. A usable model grid replaces the
/// DSP or tag estimate unless the BPM or grid was set manually. The previous BPM
/// stays visible as a candidate, and a half/double or tag disagreement marks
/// the BPM ambiguous. The stage is always recorded so it is not re-run.
pub fn apply_beat_model(analysis: &mut TrackAnalysis, stage: audio_analysis::BeatModelStage) {
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
        },
        cached,
    )
}

/// Decode provider reads on the calling worker, never in the render callback.
pub(crate) fn decode_source(
    source: Box<dyn MediaSource>,
    info: AudioSourceInfo,
) -> Result<Arc<TrackData>, String> {
    decode_source_cached(source, info, None)
}

fn decode_source_cached(
    source: Box<dyn MediaSource>,
    info: AudioSourceInfo,
    cached: Option<TrackAnalysis>,
) -> Result<Arc<TrackData>, String> {
    let mut hint = Hint::new();
    if let Some(extension) = info.extension.as_deref() {
        hint.with_extension(extension);
    }
    let stream = MediaSourceStream::new(source, Default::default());
    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("Unsupported audio container: {e}"))?;
    let mut format = probed.format;
    let selected = format.default_track().ok_or("No audio track found")?;
    let track_id = selected.id;
    let mut sample_rate = selected.codec_params.sample_rate.unwrap_or(48_000);
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
        let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        samples.copy_interleaved_ref(decoded);
        for frame in samples.samples().chunks_exact(channels) {
            let left = sanitize(frame[0]);
            let right = sanitize(if channels > 1 { frame[1] } else { frame[0] });
            pcm.push([left, right]);
        }
        // Bound memory used by untrusted/accidentally huge media to ~1.4 GiB of PCM.
        if pcm.len() > 180_000_000 {
            return Err("Track exceeds the in-memory deck limit".into());
        }
    }
    if pcm.is_empty() {
        return Err("Track contains no decoded audio".into());
    }
    let mut track = prepare_pcm_cached(
        pcm,
        sample_rate,
        AudioSourceInfo {
            title,
            artist,
            ..info
        },
        true,
        cached,
    )?;
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
            let phase =
                audio_analysis::estimate_phase(&track.samples, track.sample_rate, Some(bpm));
            track.analysis.beatgrid_offset_seconds = phase.as_ref().map(|p| p.offset_seconds);
            track.analysis.beatgrid_confidence = phase.as_ref().map(|p| p.confidence);
            track.analysis.beatgrid_source = phase.map(|_| "tag".into());
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
    let mut analysis = cached
        .filter(|a| {
            a.path == info.location
                && (2..=audio_analysis::ANALYSIS_VERSION).contains(&a.analysis_version)
                && a.sample_rate == sample_rate
                && a.channels == 2
                && (a.duration_seconds - pcm.len() as f64 / sample_rate as f64).abs() < 0.1
                && a.waveform.len() == 4096
                && a.waveform_bands.as_ref().is_some_and(|b| {
                    b.low.len() == 4096 && b.mid.len() == 4096 && b.high.len() == 4096
                })
        })
        .unwrap_or_else(|| analyze_pcm(&pcm, sample_rate, &info, recording_allowed));
    if analysis.analysis_version == 2
        && !(analysis.beatgrid_source.as_deref() == Some("manual")
            && analysis
                .beatgrid_offset_seconds
                .is_some_and(|x| x.is_finite()))
    {
        let phase = audio_analysis::estimate_phase(&pcm, sample_rate, analysis.bpm);
        analysis.beatgrid_offset_seconds = phase.as_ref().map(|p| p.offset_seconds);
        analysis.beatgrid_confidence = phase.as_ref().map(|p| p.confidence);
        analysis.beatgrid_source = phase.map(|_| {
            if analysis.bpm_source.as_deref() == Some("tag") {
                "tag".into()
            } else {
                "analysis".into()
            }
        });
    }
    analysis.analysis_version = audio_analysis::ANALYSIS_VERSION;
    analysis.recording_allowed = recording_allowed;
    analysis.title = info.title;
    analysis.artist = info.artist;
    Ok(Arc::new(TrackData {
        samples: pcm,
        sample_rate,
        analysis,
    }))
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
    let (tempo, phase) = audio_analysis::analyze_rhythm(pcm, sample_rate);
    let waveform = audio_analysis::analyze_waveform(pcm, sample_rate, 4096);
    let key = audio_analysis::analyze_key(pcm, sample_rate);
    TrackAnalysis {
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
        sections: None,
        auto_cues: None,
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
        sections: None,
        auto_cues: None,
    };
    Arc::new(TrackData {
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
        samples,
        sample_rate: rate,
        analysis: TrackAnalysis {
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
            sections: None,
            auto_cues: None,
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
        assert_eq!(decoded.analysis.waveform.len(), 4096);
        assert!((decoded.analysis.bpm.unwrap() - 120.0).abs() < 2.0);
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
    fn version_two_reuses_bpm_key_waveforms_and_only_adds_pcm_phase() {
        let (pcm, info, cached) = fixture();
        let bands = cached.waveform_bands.clone();
        let track = prepare_pcm_cached(pcm, 8000, info, false, Some(cached)).unwrap();
        assert_eq!(track.analysis.analysis_version, 3);
        assert_eq!(track.analysis.bpm, Some(127.3));
        assert_eq!(track.analysis.key.as_deref(), Some("cached key"));
        assert_eq!(track.analysis.waveform, vec![0.432; 4096]);
        assert_eq!(track.analysis.waveform_bands, bands);
        assert!((track.analysis.beatgrid_offset_seconds.unwrap() - 0.173).abs() < 0.008);
        assert_eq!(track.analysis.beatgrid_source.as_deref(), Some("analysis"));
        assert!(!track.analysis.recording_allowed);
    }
    #[test]
    fn manual_grid_survives_cache_upgrade() {
        let (pcm, info, mut cached) = fixture();
        cached.beatgrid_offset_seconds = Some(0.297);
        cached.beatgrid_confidence = Some(1.0);
        cached.beatgrid_source = Some("manual".into());
        let track = prepare_pcm_cached(pcm, 8000, info, false, Some(cached)).unwrap();
        assert_eq!(track.analysis.analysis_version, 3);
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
}
