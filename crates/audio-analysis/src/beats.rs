//! Neural beat and downbeat tracking with the Beat This! small model (JKU,
//! MIT code and weights; see models/README.md). The model runs on CPU through
//! tract on background threads, one track at a time, never in a render callback.
//!
//! Pipeline, matching the upstream reference (verified in research/beat-model-spike.md):
//! mono mix → 22,050 Hz → 128-band log-mel at 50 fps → 1500-frame chunks →
//! frame logits → local-maximum peaks → beat and downbeat times. The beats are
//! then fitted to a constant grid whose offset sits on the dominant downbeat.

use std::{
    io::Cursor,
    sync::{Arc, Mutex, OnceLock},
};

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler, WindowFunction};
use rustfft::{num_complex::Complex32, FftPlanner};
use serde::{Deserialize, Serialize};
use tract_linalg::multithread::{multithread_tract_scope, Executor};
use tract_onnx::prelude::*;

/// Version of the stored beat-model stage. Bump when the model, its
/// preprocessing, the grid fit or the sections cut in the same pass change in a
/// way that should re-run analysis. 2: song sections.
pub const BEAT_MODEL_VERSION: u32 = 2;

const MODEL: &[u8] = include_bytes!("../models/beat-this-small0.onnx");
const SR: u32 = 22_050;
const NFFT: usize = 1024;
const HOP: usize = 441;
const NMELS: usize = 128;
const NBINS: usize = NFFT / 2 + 1;
const CHUNK: usize = 1500;
const BORDER: usize = 6;
const FPS: f64 = 50.0;
const THREADS: usize = 4;
/// Beats closer than this to a grid line count as on the grid.
const GRID_TOLERANCE: f64 = 0.035;

/// Output of the neural beat stage, persisted with a track's analysis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeatModelStage {
    pub version: u32,
    /// Beat times in seconds on the analysed PCM timeline.
    pub beats_seconds: Vec<f32>,
    /// Downbeat ("beat 1") times in seconds; each is also a beat.
    pub downbeats_seconds: Vec<f32>,
    /// Constant-grid fit of the beats, when one exists.
    pub grid: Option<ModelGrid>,
}

/// A constant tempo grid fitted to the model's beats.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelGrid {
    pub bpm: f64,
    /// A downbeat time reduced modulo one bar (four beats), in seconds.
    pub downbeat_offset_seconds: f64,
    /// Fraction of beats within 35 ms of a grid line.
    pub on_grid: f32,
    /// Fraction of downbeats that fall on the chosen bar phase.
    pub bar_agreement: f32,
}

impl BeatModelStage {
    /// The grid, when it is trustworthy enough to replace the DSP estimate.
    pub fn usable_grid(&self) -> Option<ModelGrid> {
        self.grid.filter(|g| {
            self.beats_seconds.len() >= 16
                && g.on_grid >= 0.5
                && g.bpm.is_finite()
                && (40.0..=250.0).contains(&g.bpm)
        })
    }
}

type Model = Arc<TypedRunnableModel>;

fn model() -> Result<&'static Model, String> {
    static MODEL_PLAN: OnceLock<Result<Model, String>> = OnceLock::new();
    MODEL_PLAN
        .get_or_init(|| {
            tract_onnx::onnx()
                .model_for_read(&mut Cursor::new(MODEL))
                .and_then(|m| m.with_input_fact(0, f32::fact([1, CHUNK, NMELS]).into()))
                .and_then(|m| m.into_optimized())
                .and_then(|m| m.into_runnable())
                .map_err(|e| format!("Beat model could not be prepared: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Shared low-priority worker pool for the model's matrix multiplies.
fn executor() -> Executor {
    static POOL: OnceLock<Executor> = OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(THREADS)
            .thread_name(|i| format!("pulsedeck-beat-{i}"))
            .start_handler(|_| lower_thread_priority())
            .build()
            .map(|pool| Executor::MultiThread(Arc::new(pool)))
            .unwrap_or(Executor::SingleThread)
    })
    .clone()
}

#[cfg(target_os = "macos")]
fn lower_thread_priority() {
    // Utility QoS keeps analysis away from interactive and audio work.
    // SAFETY: plain syscall on the current thread with a valid QoS class.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0);
    }
}

#[cfg(not(target_os = "macos"))]
fn lower_thread_priority() {}

/// Run the beat model over decoded stereo PCM. Blocks the calling worker for
/// roughly two seconds per minute of audio. `cancel` is polled between chunks.
pub fn track_beats(
    samples: &[[f32; 2]],
    sample_rate: u32,
    cancel: &dyn Fn() -> bool,
) -> Result<BeatModelStage, String> {
    track_beats_with_spectrogram(samples, sample_rate, cancel).map(|(stage, _)| stage)
}

/// [`track_beats`], also returning the log-mel spectrogram it computed so
/// section detection can reuse it.
pub fn track_beats_with_spectrogram(
    samples: &[[f32; 2]],
    sample_rate: u32,
    cancel: &dyn Fn() -> bool,
) -> Result<(BeatModelStage, Spectrogram), String> {
    let model = model()?;
    let spect = spectrogram(samples, sample_rate)?;
    if cancel() {
        return Err("Beat analysis cancelled".into());
    }
    // One inference at a time bounds peak memory (about 250 MB per run).
    static INFERENCE: Mutex<()> = Mutex::new(());
    let _turn = INFERENCE.lock().unwrap_or_else(|e| e.into_inner());
    let (beat, downbeat) =
        multithread_tract_scope(executor(), || infer(model, &spect.frames, cancel))?;
    let (beats, downbeats) = peaks(&beat, &downbeat);
    let stage = BeatModelStage {
        version: BEAT_MODEL_VERSION,
        grid: fit_grid(&beats, &downbeats),
        beats_seconds: beats.iter().map(|&t| t as f32).collect(),
        downbeats_seconds: downbeats.iter().map(|&t| t as f32).collect(),
    };
    Ok((stage, spect))
}

/// The beat model's 128-band log-mel spectrogram at 50 frames per second.
pub struct Spectrogram {
    /// Frames × 128, row-major.
    pub(crate) frames: Vec<f32>,
}

impl Spectrogram {
    pub const FRAMES_PER_SECOND: f64 = FPS;
    pub const BANDS: usize = NMELS;
    pub fn frame_count(&self) -> usize {
        self.frames.len() / NMELS
    }
    pub fn frame(&self, index: usize) -> &[f32] {
        &self.frames[index * NMELS..(index + 1) * NMELS]
    }
    /// Centre frequency in Hz of each mel band.
    pub fn band_centres() -> Vec<f64> {
        let (lo, hi) = (hz_to_mel(30.0), hz_to_mel(11_000.0));
        (1..=NMELS)
            .map(|i| mel_to_hz(lo + (hi - lo) * i as f64 / (NMELS + 1) as f64))
            .collect()
    }
}

/// Compute the log-mel spectrogram of decoded stereo PCM (mono mix at 22,050 Hz).
pub fn spectrogram(samples: &[[f32; 2]], sample_rate: u32) -> Result<Spectrogram, String> {
    if samples.is_empty() || !(8_000..=192_000).contains(&sample_rate) {
        return Err("Beat model needs decoded audio at 8-192 kHz".into());
    }
    let mono: Vec<f32> = samples.iter().map(|f| 0.5 * (f[0] + f[1])).collect();
    Ok(Spectrogram {
        frames: log_mel(&resample(&mono, sample_rate)?),
    })
}

fn resample(mono: &[f32], rate: u32) -> Result<Vec<f32>, String> {
    if rate == SR {
        return Ok(mono.to_vec());
    }
    // One FFT sub-block keeps the anti-aliasing cutoff close to 11 kHz; the
    // model's top mel bands (hi-hats) are sensitive to a softer filter.
    let mut resampler = Fft::<f32>::new_custom(
        rate as usize,
        SR as usize,
        4096,
        1,
        1,
        WindowFunction::BlackmanHarris2,
        FixedSync::Input,
    )
    .map_err(|e| format!("Beat model resampler: {e}"))?;
    let input = InterleavedSlice::new(mono, 1, mono.len()).map_err(|e| e.to_string())?;
    Ok(resampler
        .process_all(&input, mono.len(), None)
        .map_err(|e| format!("Beat model resampling: {e}"))?
        .take_data())
}

struct MelBank {
    window: Vec<f32>,
    /// Per band: first STFT bin and the band's non-zero weights.
    bands: Vec<(usize, Vec<f32>)>,
}

fn mel_bank() -> &'static MelBank {
    static BANK: OnceLock<MelBank> = OnceLock::new();
    BANK.get_or_init(|| {
        let window = (0..NFFT)
            .map(|n| (0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / NFFT as f64).cos()) as f32)
            .collect();
        // torchaudio melscale_fbanks(513, 30, 11000, 128, 22050, norm=None, "slaney")
        let (lo, hi) = (hz_to_mel(30.0), hz_to_mel(11_000.0));
        let points: Vec<f64> = (0..NMELS + 2)
            .map(|i| mel_to_hz(lo + (hi - lo) * i as f64 / (NMELS + 1) as f64))
            .collect();
        let bands = (0..NMELS)
            .map(|m| {
                let mut first = None;
                let mut weights = Vec::new();
                for k in 0..NBINS {
                    let f = (SR / 2) as f64 * k as f64 / (NBINS - 1) as f64;
                    let down = (f - points[m]) / (points[m + 1] - points[m]);
                    let up = (points[m + 2] - f) / (points[m + 2] - points[m + 1]);
                    let w = down.min(up).max(0.0) as f32;
                    if w > 0.0 {
                        first.get_or_insert(k);
                        weights.push(w);
                    } else if first.is_some() {
                        break;
                    }
                }
                (first.unwrap_or(0), weights)
            })
            .collect();
        MelBank { window, bands }
    })
}

fn hz_to_mel(f: f64) -> f64 {
    let logstep = 6.4f64.ln() / 27.0;
    if f >= 1000.0 {
        15.0 + (f / 1000.0).ln() / logstep
    } else {
        f * 3.0 / 200.0
    }
}

fn mel_to_hz(m: f64) -> f64 {
    let logstep = 6.4f64.ln() / 27.0;
    if m >= 15.0 {
        1000.0 * (logstep * (m - 15.0)).exp()
    } else {
        m * 200.0 / 3.0
    }
}

/// beat_this LogMelSpect: centred reflect-padded STFT with a periodic Hann
/// window, magnitude / sqrt(n_fft), mel projection, ln(1 + 1000 x).
/// Returns frames × 128, row-major.
fn log_mel(x: &[f32]) -> Vec<f32> {
    let bank = mel_bank();
    let len = x.len();
    let pad = NFFT / 2;
    let sample = |i: usize| -> f32 {
        // Index into the conceptually padded signal.
        let n = len as isize;
        let mut j = i as isize - pad as isize;
        if j < 0 {
            j = -j;
        }
        if j >= n {
            j = 2 * (n - 1) - j;
        }
        x[j.clamp(0, n - 1) as usize]
    };
    let frames = 1 + len / HOP;
    let fft = FftPlanner::new().plan_fft_forward(NFFT);
    let scale = 1.0 / (NFFT as f32).sqrt();
    let mut out = vec![0f32; frames * NMELS];
    let mut buf = vec![Complex32::new(0.0, 0.0); NFFT];
    let mut scratch = vec![Complex32::new(0.0, 0.0); fft.get_inplace_scratch_len()];
    let mut magnitude = [0f32; NBINS];
    for t in 0..frames {
        let start = t * HOP;
        for (n, value) in buf.iter_mut().enumerate() {
            *value = Complex32::new(sample(start + n) * bank.window[n], 0.0);
        }
        fft.process_with_scratch(&mut buf, &mut scratch);
        for (m, v) in magnitude.iter_mut().zip(&buf) {
            *m = v.norm() * scale;
        }
        let row = &mut out[t * NMELS..(t + 1) * NMELS];
        for (value, (first, weights)) in row.iter_mut().zip(&bank.bands) {
            let energy: f32 = weights
                .iter()
                .zip(&magnitude[*first..])
                .map(|(w, v)| w * v)
                .sum();
            *value = (1.0 + 1000.0 * energy).ln();
        }
    }
    out
}

/// Chunk start frames, as in beat_this `split_piece` with `avoid_short_end`.
fn chunk_starts(len: usize) -> Vec<isize> {
    let step = (CHUNK - 2 * BORDER) as isize;
    let mut starts: Vec<isize> = (0..)
        .map(|i| i * step - BORDER as isize)
        .take_while(|&s| s < len as isize - BORDER as isize)
        .collect();
    if starts.is_empty() {
        starts.push(-(BORDER as isize));
    }
    if len > CHUNK - 2 * BORDER {
        *starts.last_mut().expect("at least one chunk") = len as isize - (CHUNK - BORDER) as isize;
    }
    starts
}

/// beat_this `split_predict_aggregate` with keep-first overlap. Short pieces
/// are zero-padded to the model's fixed 1500-frame input.
fn infer(
    model: &Model,
    spect: &[f32],
    cancel: &dyn Fn() -> bool,
) -> Result<(Vec<f32>, Vec<f32>), String> {
    let len = spect.len() / NMELS;
    let mut beat = vec![-1000f32; len];
    let mut downbeat = vec![-1000f32; len];
    let mut chunk = vec![0f32; CHUNK * NMELS];
    for &start in chunk_starts(len).iter().rev() {
        if cancel() {
            return Err("Beat analysis cancelled".into());
        }
        chunk.fill(0.0);
        for i in 0..CHUNK {
            let src = start + i as isize;
            if (0..len as isize).contains(&src) {
                let s = src as usize;
                chunk[i * NMELS..(i + 1) * NMELS]
                    .copy_from_slice(&spect[s * NMELS..(s + 1) * NMELS]);
            }
        }
        let input = Tensor::from_shape(&[1, CHUNK, NMELS], &chunk).map_err(|e| e.to_string())?;
        let out = model
            .run(tvec!(input.into()))
            .map_err(|e| format!("Beat model inference failed: {e}"))?;
        let b = out[0]
            .to_plain_array_view::<f32>()
            .map_err(|e| e.to_string())?;
        let d = out[1]
            .to_plain_array_view::<f32>()
            .map_err(|e| e.to_string())?;
        for i in BORDER..CHUNK - BORDER {
            let dst = start + i as isize;
            if (0..len as isize).contains(&dst) {
                beat[dst as usize] = b[[0, i]];
                downbeat[dst as usize] = d[[0, i]];
            }
        }
    }
    Ok((beat, downbeat))
}

/// beat_this "minimal" post-processing: local maxima within ±3 frames above
/// logit 0, adjacent peaks merged, downbeats snapped to the nearest beat.
fn peaks(beat: &[f32], downbeat: &[f32]) -> (Vec<f64>, Vec<f64>) {
    let times = |x: &[f32]| -> Vec<f64> {
        let frames: Vec<usize> = (0..x.len())
            .filter(|&i| {
                let window = &x[i.saturating_sub(3)..(i + 4).min(x.len())];
                x[i] > 0.0 && window.iter().all(|&v| v <= x[i])
            })
            .collect();
        merge_adjacent(&frames)
            .into_iter()
            .map(|f| f / FPS)
            .collect()
    };
    let beats = times(beat);
    let mut downbeats: Vec<f64> = times(downbeat)
        .into_iter()
        .map(|d| {
            beats
                .iter()
                .copied()
                .min_by(|a, b| (a - d).abs().total_cmp(&(b - d).abs()))
                .unwrap_or(d)
        })
        .collect();
    downbeats.sort_by(f64::total_cmp);
    downbeats.dedup();
    (beats, downbeats)
}

/// beat_this `deduplicate_peaks(width=1)`: runs of frames each within one
/// frame of the running mean collapse to that mean.
fn merge_adjacent(frames: &[usize]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut iter = frames.iter();
    let Some(&first) = iter.next() else {
        return out;
    };
    let (mut mean, mut count) = (first as f64, 1.0);
    for &f in iter {
        let f = f as f64;
        if f - mean <= 1.0 {
            count += 1.0;
            mean += (f - mean) / count;
        } else {
            out.push(mean);
            mean = f;
            count = 1.0;
        }
    }
    out.push(mean);
    out
}

/// Fit the app's constant grid to model beats. BPM is searched within ±4% of
/// the median beat interval for the phase that puts the most beats within
/// 35 ms of a line, then refined by least squares over those beats. The offset
/// lands on the bar phase most downbeats agree with.
pub fn fit_grid(beats: &[f64], downbeats: &[f64]) -> Option<ModelGrid> {
    if beats.len() < 8 {
        return None;
    }
    let mut intervals: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    intervals.sort_by(f64::total_cmp);
    let median = intervals[intervals.len() / 2];
    if !median.is_finite() || median <= 0.0 {
        return None;
    }
    let bpm0 = 60.0 / median;
    let best_phase = |period: f64| -> (f64, usize) {
        let w = std::f64::consts::TAU / period;
        let (c, s) = beats.iter().fold((0.0, 0.0), |a, &t| {
            (a.0 + (w * t).cos(), a.1 + (w * t).sin())
        });
        let origin = s.atan2(c) / w;
        let hits = beats
            .iter()
            .filter(|&&t| {
                let r = (t - origin).rem_euclid(period);
                r.min(period - r) <= GRID_TOLERANCE
            })
            .count();
        (origin, hits)
    };
    let steps = (bpm0 * 0.08 / 0.005) as usize;
    let (mut period, mut origin) = (0..=steps)
        .map(|i| 60.0 / (bpm0 * 0.96 + i as f64 * 0.005))
        .map(|p| (p, best_phase(p)))
        .max_by_key(|(_, (_, hits))| *hits)
        .map(|(p, (o, _))| (p, o))?;
    let inliers: Vec<(f64, f64)> = beats
        .iter()
        .map(|&t| (((t - origin) / period).round(), t))
        .filter(|&(k, t)| (t - (origin + k * period)).abs() <= GRID_TOLERANCE)
        .collect();
    if inliers.len() >= 8 {
        let n = inliers.len() as f64;
        let (sk, st) = inliers
            .iter()
            .fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
        let (mk, mt) = (sk / n, st / n);
        let num: f64 = inliers.iter().map(|p| (p.0 - mk) * (p.1 - mt)).sum();
        let den: f64 = inliers.iter().map(|p| (p.0 - mk).powi(2)).sum();
        if den > 0.0 && num / den > 0.0 {
            period = num / den;
            origin = mt - period * mk;
        }
    }
    let on_grid = beats
        .iter()
        .filter(|&&t| {
            let k = ((t - origin) / period).round();
            (t - (origin + k * period)).abs() <= GRID_TOLERANCE
        })
        .count() as f32
        / beats.len() as f32;
    let mut votes = [0usize; 4];
    for &d in downbeats {
        votes[(((d - origin) / period).round() as i64).rem_euclid(4) as usize] += 1;
    }
    let (phase, &agreeing) = votes
        .iter()
        .enumerate()
        .max_by_key(|(_, v)| **v)
        .expect("four phases");
    Some(ModelGrid {
        bpm: 60.0 / period,
        downbeat_offset_seconds: (origin + phase as f64 * period).rem_euclid(4.0 * period),
        on_grid,
        bar_agreement: if downbeats.is_empty() {
            0.0
        } else {
            agreeing as f32 / downbeats.len() as f32
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mel_bank_matches_torchaudio_slaney_filters() {
        let bank = mel_bank();
        let total: f64 = bank
            .bands
            .iter()
            .flat_map(|(_, w)| w)
            .map(|&w| w as f64)
            .sum();
        assert!((total - 502.205_627).abs() < 1e-3, "{total}");
        // (band, first bin, last bin, first weight) from torchaudio 2.9.
        for (band, first, last, weight) in [
            (0, 2, 3, 0.511_526_3),
            (1, 3, 4, 0.354_512_2),
            (64, 93, 97, 0.332_386_4),
            (127, 485, 510, 0.029_245_75),
        ] {
            let (start, weights) = &bank.bands[band];
            assert_eq!(
                (*start, start + weights.len() - 1),
                (first, last),
                "band {band}"
            );
            assert!(
                (weights[0] - weight).abs() < 1e-4,
                "band {band}: {}",
                weights[0]
            );
        }
    }

    #[test]
    fn log_mel_matches_beat_this_reference() {
        // 440 Hz + 3 kHz tones, one second at 22,050 Hz; values from beat_this LogMelSpect.
        let x: Vec<f32> = (0..22_050)
            .map(|n| {
                let t = n as f64 / 22_050.0;
                (0.5 * (std::f64::consts::TAU * 440.0 * t).sin()
                    + 0.25 * (std::f64::consts::TAU * 3000.0 * t).sin()) as f32
            })
            .collect();
        let s = log_mel(&x);
        assert_eq!(s.len(), 51 * NMELS);
        let total: f64 = s.iter().map(|&v| v as f64).sum();
        assert!((total - 6204.7239).abs() < 0.05, "{total}");
        let bands = [0, 20, 40, 60, 80, 100, 127];
        let frame0 = [5.7494, 6.05581, 3.64542, 3.37127, 7.08179, 4.37343, 4.10493];
        let frame10 = [0.12671, 2.13113, 0.062, 0.01999, 3.98264, 0.00192, 0.00034];
        for (i, &b) in bands.iter().enumerate() {
            assert!(
                (s[b] - frame0[i]).abs() < 2e-3,
                "frame 0 band {b}: {}",
                s[b]
            );
            let v = s[10 * NMELS + b];
            assert!((v - frame10[i]).abs() < 2e-3, "frame 10 band {b}: {v}");
        }
    }

    #[test]
    fn chunks_follow_upstream_split_piece() {
        assert_eq!(chunk_starts(100), [-6]);
        assert_eq!(chunk_starts(1488), [-6]);
        assert_eq!(chunk_starts(1489), [-6, -5]);
        assert_eq!(chunk_starts(1500), [-6, 6]);
        assert_eq!(chunk_starts(3000), [-6, 1482, 1506]);
        let long = chunk_starts(21_804);
        assert_eq!(long.len(), 15);
        assert_eq!(&long[..4], [-6, 1482, 2970, 4458]);
        assert_eq!(&long[13..], [19_338, 20_310]);
    }

    #[test]
    fn peaks_pick_local_maxima_and_snap_downbeats() {
        let mut beat = vec![-5f32; 200];
        let mut down = vec![-5f32; 200];
        for f in [10, 35, 60, 85, 110] {
            beat[f] = 3.0;
        }
        // A flat two-frame peak merges to its mean; a sub-threshold bump is ignored.
        beat[135] = 2.0;
        beat[136] = 2.0;
        beat[160] = -0.5;
        down[11] = 1.0;
        down[111] = 2.0;
        let (beats, downs) = peaks(&beat, &down);
        assert_eq!(beats, [0.2, 0.7, 1.2, 1.7, 2.2, 2.71]);
        assert_eq!(downs, [0.2, 2.2]);
    }

    #[test]
    fn grid_fit_recovers_tempo_and_bar_phase_despite_gaps() {
        let bpm = 123.4;
        let period = 60.0 / bpm;
        let first_downbeat = 0.731;
        let mut beats = Vec::new();
        let mut downbeats = Vec::new();
        for k in 0..600 {
            // Deterministic ±8 ms jitter, every 13th beat missing.
            if k % 13 == 5 {
                continue;
            }
            let jitter = ((k * 7919) % 17) as f64 / 16.0 * 0.016 - 0.008;
            let t = first_downbeat + (k as f64 - 2.0) * period + jitter;
            beats.push(t);
            if (k as i64 - 2).rem_euclid(4) == 0 {
                downbeats.push(t);
            }
        }
        // A misplaced downbeat should not move the bar phase.
        downbeats.push(beats[101]);
        downbeats.sort_by(f64::total_cmp);
        let grid = fit_grid(&beats, &downbeats).expect("grid");
        assert!((grid.bpm - bpm).abs() < 0.01, "{}", grid.bpm);
        let bar = 4.0 * period;
        let phase_error = (grid.downbeat_offset_seconds - first_downbeat).rem_euclid(bar);
        assert!(phase_error.min(bar - phase_error) < 0.004, "{grid:?}");
        assert!(grid.on_grid > 0.99 && grid.bar_agreement > 0.98, "{grid:?}");
        assert!(fit_grid(&beats[..5], &[]).is_none());
    }

    /// Kick on every beat, snare on two and four, closed hi-hats on eighths.
    fn drum_loop(bpm: f64, seconds: f64, rate: u32) -> Vec<[f32; 2]> {
        let beat = 60.0 / bpm;
        let mut seed = 0x2545_f491u32;
        (0..(seconds * rate as f64) as usize)
            .map(|i| {
                let t = i as f64 / rate as f64;
                let in_beat = t % beat;
                let beat_index = (t / beat) as i64;
                let kick = (std::f64::consts::TAU
                    * (50.0 * in_beat + 8.0 * (1.0 - (-in_beat * 30.0).exp())))
                .sin()
                    * (-in_beat * 18.0).exp();
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let noise = seed as f64 / u32::MAX as f64 * 2.0 - 1.0;
                let snare = if beat_index % 2 == 1 {
                    noise * (-in_beat * 25.0).exp() * 0.5
                } else {
                    0.0
                };
                let hat = noise * (-(t % (beat / 2.0)) * 120.0).exp() * 0.15;
                let v = (0.6 * kick + snare + hat) as f32;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn model_tracks_a_synthetic_drum_loop() {
        let audio = drum_loop(120.0, 40.0, 44_100);
        let stage = track_beats(&audio, 44_100, &|| false).expect("model runs");
        assert_eq!(stage.version, BEAT_MODEL_VERSION);
        let grid = stage.usable_grid().expect("usable grid");
        assert!((grid.bpm - 120.0).abs() < 0.5, "{grid:?}");
        assert!(
            stage.beats_seconds.len() >= 70,
            "{} beats",
            stage.beats_seconds.len()
        );
        // Every model beat should sit near a kick (multiples of 0.5 s).
        let near_kick = stage
            .beats_seconds
            .iter()
            .filter(|&&t| {
                let r = (t as f64).rem_euclid(0.5);
                r.min(0.5 - r) < 0.04
            })
            .count();
        assert!(near_kick as f32 / stage.beats_seconds.len() as f32 > 0.9);
    }

    #[test]
    fn cancellation_stops_before_inference() {
        let audio = drum_loop(120.0, 5.0, 44_100);
        let err = track_beats(&audio, 44_100, &|| true).unwrap_err();
        assert!(err.contains("cancelled"), "{err}");
    }
}
