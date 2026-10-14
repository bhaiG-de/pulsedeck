use crate::spectral::{downsample, Spectrum};
use rustfft::{num_complex::Complex, FftPlanner};
use serde::{Deserialize, Serialize};
const RATE: u32 = 8000;
const HOP: usize = 80;
const PHASE_HOP: usize = 40;
const FFT: usize = 512;
const MIN_BPM: f64 = 40.0;
const MAX_BPM: f64 = 240.0;
const STEP: f64 = 0.25;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TempoCandidate {
    pub bpm: f64,
    pub score: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TempoEstimate {
    pub bpm: Option<f64>,
    pub confidence: f32,
    pub ambiguous: bool,
    pub candidates: Vec<TempoCandidate>,
}
impl TempoEstimate {
    fn empty() -> Self {
        Self {
            bpm: None,
            confidence: 0.0,
            ambiguous: false,
            candidates: Vec::new(),
        }
    }
}
/// Positive log-spectral flux, separate bass/mid/treble novelty, distributed
/// segment agreement and a harmonically weighted autocorrelation comb. Metrical
/// alternatives remain visible instead of silently forcing all music into one octave.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BeatPhaseEstimate {
    pub offset_seconds: f64,
    pub confidence: f32,
}
struct NoveltySegment {
    start_seconds: f64,
    onset: Vec<f64>,
    phase_onset: Vec<f64>,
}
fn segments(samples: &[[f32; 2]], rate: u32) -> Vec<NoveltySegment> {
    if !(8000..=384000).contains(&rate) || samples.len() < rate as usize * 5 {
        return Vec::new();
    }
    let duration = samples.len() / rate as usize;
    let segment_frames = (24 * rate as usize).min(samples.len());
    let count = if duration > 55 {
        3
    } else if duration > 30 {
        2
    } else {
        1
    };
    let mut result = Vec::new();
    for index in 0..count {
        let start = if count == 1 {
            0
        } else {
            (samples.len() - segment_frames) * (index * 2 + 1) / (count * 2)
        };
        let end = (start + segment_frames).min(samples.len());
        let signal = downsample(samples, start, end, RATE, rate);
        if let Some(onset) = novelty(&signal) {
            result.push(NoveltySegment {
                start_seconds: start as f64 / rate as f64,
                onset,
                phase_onset: phase_novelty(&signal),
            });
        }
    }
    result
}
fn tempo_from_segments(segments: &[NoveltySegment]) -> TempoEstimate {
    let curves = segments
        .iter()
        .map(|s| {
            let corr = autocorrelation(&s.onset, 600);
            (0..=((MAX_BPM - MIN_BPM) / STEP) as usize)
                .map(|i| score(MIN_BPM + i as f64 * STEP, &corr))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    summarize(&curves)
}
pub fn estimate_tempo(samples: &[[f32; 2]], rate: u32) -> TempoEstimate {
    tempo_from_segments(&segments(samples, rate))
}
/// Analyze BPM and phase from the same bounded spectral-novelty excerpts.
pub fn analyze_rhythm(
    samples: &[[f32; 2]],
    rate: u32,
) -> (TempoEstimate, Option<BeatPhaseEstimate>) {
    let segments = segments(samples, rate);
    let tempo = tempo_from_segments(&segments);
    let phase = phase_from_segments(&segments, tempo.bpm);
    (tempo, phase)
}
/// Estimate a periodic beat position against an explicitly supplied tempo. None
/// means absent or unstable beat evidence, never an assumed phase of zero. This
/// finds a beat, not the true musical downbeat or a four-beat measure boundary.
pub fn estimate_phase(
    samples: &[[f32; 2]],
    rate: u32,
    bpm: Option<f64>,
) -> Option<BeatPhaseEstimate> {
    phase_from_segments(&segments(samples, rate), bpm)
}
/// Attack envelopes for beat phase. One FFT splits the excerpt into zero-phase
/// low/mid/high bands and forms each band's analytic signal. Its squared
/// magnitude is an energy envelope without carrier ripple: a kick's 40-80 Hz
/// body is one smooth envelope, not a train of per-cycle energy steps that a
/// 5 ms RMS window would report as rises across the whole body. The positive
/// log-energy step between adjacent frames is timestamped at their shared
/// boundary, so nothing delays the envelope and no compensation is needed. Log
/// steps weight where an attack starts rather than where the loud body sits.
fn phase_novelty(signal: &[f32]) -> Vec<f64> {
    let frames = signal.len() / PHASE_HOP;
    let mut result = vec![0.0; frames];
    if frames < 8 {
        return result;
    }
    // Padding keeps band-filter ringing from wrapping between excerpt ends.
    let size = (signal.len() + RATE as usize / 4).next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let forward = planner.plan_fft_forward(size);
    let inverse = planner.plan_fft_inverse(size);
    let mean = signal.iter().map(|x| *x as f64).sum::<f64>() / signal.len() as f64;
    let mut spectrum = signal
        .iter()
        .map(|x| Complex::new(*x as f64 - mean, 0.0))
        .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
        .take(size)
        .collect::<Vec<_>>();
    forward.process(&mut spectrum);
    // Complementary second-order Butterworth magnitudes: low + mid + high = 1.
    let lowpass = |freq: f64, cutoff: f64| 1.0 / (1.0 + (freq / cutoff).powi(4)).sqrt();
    let mut band = vec![Complex::new(0.0, 0.0); size];
    for (index, weight) in [0.82, 0.12, 0.06].into_iter().enumerate() {
        for (bin, value) in band.iter_mut().enumerate() {
            let freq = bin as f64 * RATE as f64 / size as f64;
            let low = lowpass(freq, 250.0);
            let upper = lowpass(freq, 2500.0);
            let gain = [low, upper - low, 1.0 - upper][index];
            // Analytic signal: no DC, doubled positive and no negative frequencies.
            *value = if bin == 0 || bin >= size / 2 {
                Complex::new(0.0, 0.0)
            } else {
                spectrum[bin] * (2.0 * gain / size as f64)
            };
        }
        inverse.process(&mut band);
        let energy = (0..frames)
            .map(|frame| {
                band[frame * PHASE_HOP..(frame + 1) * PHASE_HOP]
                    .iter()
                    .map(|v| v.norm_sqr())
                    .sum::<f64>()
                    / PHASE_HOP as f64
            })
            .collect::<Vec<_>>();
        let floor = energy.iter().sum::<f64>() / frames as f64 * 0.001;
        if floor < 1e-14 {
            continue;
        }
        let mut rises = vec![0.0; frames];
        for frame in 1..frames {
            rises[frame] =
                ((energy[frame] + floor).ln() - (energy[frame - 1] + floor).ln()).max(0.0);
        }
        let rms = (rises.iter().map(|x| x * x).sum::<f64>() / frames as f64).sqrt();
        if rms < 1e-9 {
            continue;
        }
        for (value, rise) in result.iter_mut().zip(&rises) {
            *value += weight * (rise / rms).min(12.0);
        }
    }
    for value in result.iter_mut().take(4) {
        *value = 0.0;
    }
    // Remove the dense background of small log steps (sustained notes, noise,
    // reverb) with a symmetric 0.5 s local mean, so only distinct attacks fold.
    let mut prefix = vec![0.0; frames + 1];
    for (i, value) in result.iter().enumerate() {
        prefix[i + 1] = prefix[i] + value;
    }
    (0..frames)
        .map(|i| {
            let low = i.saturating_sub(50);
            let high = (i + 51).min(frames);
            let local = (prefix[high] - prefix[low]) / (high - low) as f64;
            (result[i] - local).max(0.0)
        })
        .collect()
}
fn phase_from_segments(segments: &[NoveltySegment], bpm: Option<f64>) -> Option<BeatPhaseEstimate> {
    let bpm = bpm.filter(|v| v.is_finite() && (30.0..=300.0).contains(v))?;
    let period = 60.0 / bpm;
    const BINS: usize = 256;
    let sigma = (0.015 / period * BINS as f64).clamp(1.5, 12.0);
    let radius = (sigma * 3.0).ceil() as isize;
    let mut histograms = Vec::new();
    let mut weights = Vec::new();
    for segment in segments {
        let corr = autocorrelation(&segment.phase_onset, 800);
        let periodicity = at(&corr, 12000.0 / bpm);
        if periodicity < 0.16 {
            continue;
        }
        let total = segment.phase_onset.iter().sum::<f64>();
        if total < 1e-8 {
            continue;
        }
        let mut folded = [0.0_f64; BINS];
        for (i, onset) in segment.phase_onset.iter().enumerate() {
            let time = segment.start_seconds + (i * PHASE_HOP) as f64 / RATE as f64;
            let position = time.rem_euclid(period) / period * BINS as f64;
            let index = position as usize;
            let fraction = position - index as f64;
            folded[index % BINS] += onset / total * (1.0 - fraction);
            folded[(index + 1) % BINS] += onset / total * fraction;
        }
        let mut smoothed = [0.0_f64; BINS];
        for (index, value) in smoothed.iter_mut().enumerate() {
            let mut norm = 0.0;
            for delta in -radius..=radius {
                let weight = (-0.5 * (delta as f64 / sigma).powi(2)).exp();
                *value +=
                    folded[(index as isize + delta).rem_euclid(BINS as isize) as usize] * weight;
                norm += weight;
            }
            *value /= norm;
        }
        histograms.push(smoothed);
        weights.push(periodicity);
    }
    if histograms.is_empty() {
        return None;
    }
    let total_weight = weights.iter().sum::<f64>();
    let mut aggregate = [0.0_f64; BINS];
    for (histogram, weight) in histograms.iter().zip(&weights) {
        for index in 0..BINS {
            aggregate[index] += histogram[index] * weight / total_weight;
        }
    }
    let peak = aggregate
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))?;
    let mut index = peak.0;
    let maximum = *peak.1;
    // When two equally supported subdivisions fit one tempo period, prefer the
    // first detected beat in that period; never invent a downbeat assignment.
    for candidate in 0..index {
        if aggregate[candidate] >= maximum * 0.97
            && aggregate[candidate] >= aggregate[(candidate + BINS - 1) % BINS]
            && aggregate[candidate] >= aggregate[(candidate + 1) % BINS]
        {
            index = candidate;
            break;
        }
    }
    // An excerpt agrees when it supports a beat within 25 ms of the chosen
    // phase. Attack envelopes are sharp enough that BPM rounding alone (0.02
    // BPM at 112 BPM drifts 27 ms over 150 s) would otherwise read as conflict.
    // For the same reason concentration is judged inside each 24 s excerpt,
    // where that drift is negligible, rather than on the drift-smeared aggregate.
    let tolerance = (0.025 / period * BINS as f64).round() as isize;
    let mut agreeing = 0;
    let mut concentration = 0.0;
    for (histogram, weight) in histograms.iter().zip(&weights) {
        let local = histogram.iter().copied().fold(0.0_f64, f64::max);
        let near = (-tolerance..=tolerance)
            .map(|delta| histogram[(index as isize + delta).rem_euclid(BINS as isize) as usize])
            .fold(0.0_f64, f64::max);
        if near >= local * 0.65 {
            agreeing += 1;
        }
        // Each histogram sums to one, so its mean bin is 1 / BINS.
        concentration += ((near * BINS as f64 - 1.0) / 6.0).clamp(0.0, 1.0) * weight / total_weight;
    }
    let agreement = agreeing as f64 / histograms.len() as f64;
    let periodicity = weights.iter().sum::<f64>() / weights.len() as f64;
    let confidence = (concentration * periodicity.clamp(0.0, 1.0)).sqrt() * (0.5 + 0.5 * agreement);
    // A strict majority must agree: two excerpts that disagree are no evidence
    // of one uniform grid.
    if confidence < 0.22 || agreement <= 0.5 {
        return None;
    }
    let delta = parabolic(
        aggregate[(index + BINS - 1) % BINS],
        aggregate[index],
        aggregate[(index + 1) % BINS],
    );
    let phase = ((index as f64 + delta) / BINS as f64 * period).rem_euclid(period);
    Some(BeatPhaseEstimate {
        offset_seconds: phase,
        confidence: confidence as f32,
    })
}

fn summarize(curves: &[Vec<f64>]) -> TempoEstimate {
    if curves.is_empty() {
        return TempoEstimate::empty();
    }
    let count = curves[0].len();
    let aggregate = (0..count)
        .map(|i| curves.iter().map(|c| c[i]).sum::<f64>() / curves.len() as f64)
        .collect::<Vec<_>>();
    let mut peaks = Vec::new();
    for i in 1..count - 1 {
        if aggregate[i] >= aggregate[i - 1] && aggregate[i] > aggregate[i + 1] {
            let delta = parabolic(aggregate[i - 1], aggregate[i], aggregate[i + 1]);
            let bpm = MIN_BPM + (i as f64 + delta) * STEP;
            peaks.push((bpm, aggregate[i]));
        }
    }
    peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
    // Integer-lag sampling can slightly favor a two-beat period. When its
    // double-tempo fundamental has virtually equal support, prefer that shorter
    // supported beat period; keep both candidates and explicitly flag ambiguity.
    if let Some(&(slower, maximum)) = peaks.first() {
        if let Some(index) = peaks.iter().position(|(bpm, value)| {
            *bpm > slower * 1.8 && (*bpm / slower - 2.0).abs() < 0.06 && *value >= maximum * 0.96
        }) {
            peaks.swap(0, index);
        }
    }
    let Some(&(best, best_score)) = peaks.first() else {
        return TempoEstimate::empty();
    };
    if best_score < 0.16 {
        return TempoEstimate::empty();
    }
    let mut candidates = Vec::new();
    for (bpm, value) in &peaks {
        if candidates
            .iter()
            .all(|c: &TempoCandidate| (c.bpm - bpm).abs() > 1.5)
        {
            candidates.push(TempoCandidate {
                bpm: (bpm * 100.0).round() / 100.0,
                score: value.clamp(0.0, 1.0) as f32,
            });
            if candidates.len() == 4 {
                break;
            }
        }
    }
    // A local winner can represent another metrical level even when the
    // aggregate tempo is strongly present. Assess support at the aggregate
    // winner itself, rather than requiring each segment's single maximum to
    // land on an enumerated half/double ratio.
    let mut agreement = 0.0;
    let center = ((best - MIN_BPM) / STEP).round() as usize;
    let radius = (best * 0.015 / STEP).ceil() as usize;
    for curve in curves {
        let strongest = curve.iter().copied().fold(0.0_f64, f64::max);
        let low = center.saturating_sub(radius);
        let high = (center + radius + 1).min(curve.len());
        let supported = curve[low..high].iter().copied().fold(0.0_f64, f64::max);
        agreement += (supported / strongest.max(1e-12)).clamp(0.0, 1.0);
    }
    agreement /= curves.len() as f64;
    let strongest_alternative = peaks
        .iter()
        .skip(1)
        .filter(|(bpm, _)| (bpm - best).abs() > 2.0)
        .map(|(_, score)| *score)
        .next()
        .unwrap_or(0.0);
    let ambiguous = strongest_alternative > best_score * 0.87 || agreement < 0.80;
    let confidence = ((best_score - 0.10) / 0.65).clamp(0.0, 1.0)
        * (0.65 + 0.35 * agreement)
        * if ambiguous { 0.8 } else { 1.0 };
    TempoEstimate {
        bpm: if confidence >= 0.16 {
            Some((best * 100.0).round() / 100.0)
        } else {
            None
        },
        confidence: confidence as f32,
        ambiguous,
        candidates,
    }
}
fn novelty(signal: &[f32]) -> Option<Vec<f64>> {
    if signal.len() < RATE as usize * 4 {
        return None;
    }
    let mut fft = Spectrum::new(FFT);
    let mut previous = vec![0.0_f32; FFT / 2 + 1];
    let mut bands: [Vec<f64>; 3] = std::array::from_fn(|_| Vec::new());
    let mut counts = [0_usize; 3];
    for i in 1..FFT / 2 {
        let freq = i as f32 * RATE as f32 / FFT as f32;
        let band = if freq < 250.0 {
            0
        } else if freq < 2500.0 {
            1
        } else {
            2
        };
        counts[band] += 1;
    }
    for start in (0..signal.len().saturating_sub(FFT)).step_by(HOP) {
        fft.process(&signal[start..start + FFT]);
        let logs = fft
            .magnitudes
            .iter()
            .map(|m| (1.0 + *m * 20.0).ln())
            .collect::<Vec<_>>();
        let mut flux = [0.0_f64; 3];
        for i in 1..FFT / 2 {
            let reference = previous[i - 1].max(previous[i]).max(previous[i + 1]);
            let difference = (logs[i] - reference).max(0.0) as f64;
            let freq = i as f32 * RATE as f32 / FFT as f32;
            let band = if freq < 250.0 {
                0
            } else if freq < 2500.0 {
                1
            } else {
                2
            };
            flux[band] += difference;
        }
        for band in 0..3 {
            bands[band].push(flux[band] / counts[band].max(1) as f64);
        }
        previous = logs;
    }
    let length = bands[0].len();
    let mut onset = vec![0.0; length];
    let mut active = 0;
    for (band, envelope) in bands.iter().enumerate() {
        let rms = (envelope.iter().map(|x| x * x).sum::<f64>() / length as f64).sqrt();
        if rms < 1e-5 {
            continue;
        }
        active += 1;
        let weight = [0.6, 0.28, 0.12][band];
        for i in 0..length {
            onset[i] += weight * (envelope[i] / rms).min(12.0);
        }
    }
    if active == 0 {
        return None;
    }
    // Suppress isolated first-frame flux and slow gain pumping.
    for x in onset.iter_mut().take(3) {
        *x = 0.0;
    }
    let mut prefix = vec![0.0; length + 1];
    for i in 0..length {
        prefix[i + 1] = prefix[i] + onset[i];
    }
    for (i, value) in onset.iter_mut().enumerate() {
        let low = i.saturating_sub(50);
        let high = (i + 51).min(length);
        let local = (prefix[high] - prefix[low]) / (high - low) as f64;
        *value = (*value - local * 0.65).max(0.0);
    }
    let unsmoothed = onset.clone();
    for i in 1..length - 1 {
        onset[i] = unsmoothed[i - 1] * 0.25 + unsmoothed[i] * 0.5 + unsmoothed[i + 1] * 0.25;
    }
    let energy = onset.iter().map(|x| x * x).sum::<f64>();
    if energy < 1e-6 {
        None
    } else {
        Some(onset)
    }
}
fn autocorrelation(onset: &[f64], max_lag: usize) -> Vec<f64> {
    let max_lag = max_lag.min(onset.len() / 2);
    let mean = onset.iter().sum::<f64>() / onset.len() as f64;
    let centered = onset.iter().map(|x| x - mean).collect::<Vec<_>>();
    let mut result = vec![0.0; max_lag + 1];
    for (lag, value) in result.iter_mut().enumerate().skip(1) {
        let mut cross = 0.0;
        let mut left = 0.0;
        let mut right = 0.0;
        for i in lag..centered.len() {
            cross += centered[i] * centered[i - lag];
            left += centered[i] * centered[i];
            right += centered[i - lag] * centered[i - lag];
        }
        *value = (cross / (left * right).sqrt().max(1e-12)).max(0.0);
    }
    result
}
fn at(values: &[f64], position: f64) -> f64 {
    let i = position as usize;
    if i + 1 >= values.len() {
        return 0.0;
    }
    let t = position - i as f64;
    let p0 = values[i.saturating_sub(1)];
    let p1 = values[i];
    let p2 = values[i + 1];
    let p3 = values[(i + 2).min(values.len() - 1)];
    // Cubic interpolation preserves fractional lag curvature instead of
    // creating a triangular score maximum at every integer correlation lag.
    (0.5 * (2.0 * p1
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t))
        .clamp(0.0, 1.0)
}

fn score(bpm: f64, corr: &[f64]) -> f64 {
    let lag = 6000.0 / bpm;
    let mut result = 0.0;
    let mut weight = 0.0;
    for (multiple, gain) in [(1.0, 1.0), (2.0, 0.5), (3.0, 0.25), (4.0, 0.125)] {
        if lag * multiple < (corr.len() - 1) as f64 {
            result += at(corr, lag * multiple) * gain;
            weight += gain;
        }
    }
    let prior = (-0.5 * (bpm / 120.0).log2().powi(2) / 1.4_f64.powi(2)).exp();
    result / weight.max(1.0) * prior
}
fn parabolic(a: f64, b: f64, c: f64) -> f64 {
    let denominator = a - 2.0 * b + c;
    if denominator.abs() < 1e-10 {
        0.0
    } else {
        (0.5 * (a - c) / denominator).clamp(-1.0, 1.0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rhythm(bpm: f64, rate: u32, seconds: u32, subdivisions: bool) -> Vec<[f32; 2]> {
        let mut seed = 73_u32;
        (0..rate as usize * seconds as usize)
            .map(|i| {
                let time = i as f64 / rate as f64;
                let phase = time.rem_euclid(60.0 / bpm);
                let kick =
                    (2.0 * std::f64::consts::PI * 70.0 * phase).sin() * (-phase * 50.0).exp() * 0.8;
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = seed as f64 / u32::MAX as f64 * 2.0 - 1.0;
                let hat = if subdivisions {
                    noise * (-time.rem_euclid(30.0 / bpm) * 120.0).exp() * 0.25
                } else {
                    0.0
                };
                [(kick + hat) as f32; 2]
            })
            .collect()
    }
    #[test]
    fn fractional_tempos_with_subdivision_and_multiple_sample_rates() {
        for (bpm, rate) in [(93.7, 8000), (127.3, 44100), (174.5, 48000), (65.2, 22050)] {
            let estimate = estimate_tempo(&rhythm(bpm, rate, 16, bpm > 70.0), rate);
            assert!(
                (estimate.bpm.unwrap() - bpm).abs() < 0.3,
                "{bpm}: {estimate:?}"
            );
            assert!(estimate.confidence > 0.35);
        }
    }
    #[test]
    fn silence_dc_and_unpitched_noise_do_not_invent_a_tempo() {
        for samples in [vec![[0.0; 2]; 48000], vec![[0.3; 2]; 48000]] {
            assert_eq!(estimate_tempo(&samples, 8000).bpm, None);
        }
        let mut seed = 24_u32;
        let noise = (0..80000)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                [seed as f32 / u32::MAX as f32 * 0.2; 2]
            })
            .collect::<Vec<_>>();
        assert_eq!(estimate_tempo(&noise, 8000).bpm, None);
    }
    #[test]
    fn intro_silence_and_antiphase_do_not_hide_the_later_rhythm() {
        let mut samples = vec![[0.0; 2]; 8000 * 35];
        samples.extend(
            rhythm(121.5, 8000, 40, false)
                .into_iter()
                .map(|s| [s[0], -s[1]]),
        );
        let tempo = estimate_tempo(&samples, 8000);
        assert!((tempo.bpm.unwrap() - 121.5).abs() < 0.8, "{tempo:?}");
    }
}

#[cfg(test)]
mod metrical_tests {
    use super::*;
    #[test]
    fn quarter_and_eighth_pulses_surface_the_half_double_choice() {
        let rate = 8000;
        let bpm = 65.2;
        let pcm = (0..rate * 16)
            .map(|i| {
                let t = i as f64 / rate as f64;
                let phase = t.rem_euclid(60.0 / bpm);
                let sub = t.rem_euclid(30.0 / bpm);
                let kick =
                    (2.0 * std::f64::consts::PI * 75.0 * phase).sin() * (-phase * 45.0).exp() * 0.5;
                let hat =
                    (2.0 * std::f64::consts::PI * 2700.0 * sub).sin() * (-sub * 120.0).exp() * 0.6;
                [(kick + hat) as f32; 2]
            })
            .collect::<Vec<_>>();
        let e = estimate_tempo(&pcm, rate);
        assert!(
            e.candidates.iter().any(|c| (c.bpm - bpm).abs() < 0.8),
            "{e:?}"
        );
        assert!(
            e.candidates.iter().any(|c| (c.bpm - bpm * 2.0).abs() < 1.0),
            "{e:?}"
        );
        let best = e.bpm.unwrap();
        assert!(
            (best - bpm).abs() < 0.8 || (best - bpm * 2.0).abs() < 1.0,
            "{e:?}"
        );
    }
}

#[cfg(test)]
mod local_competition_regressions {
    use super::*;
    #[test]
    fn supported_aggregate_survives_stronger_three_two_local_competitors() {
        // A common fractional beat remains present in all three sections, while
        // local tempo curves favor different half/compound/double accent levels.
        // No single local maximum consistently equals the global tempo.
        let beat = 127.8;
        let curves = [
            (beat * 2.0 / 3.0, 0.58),
            (beat * 1.5, 0.62),
            (beat / 2.0, 0.60),
        ]
        .iter()
        .map(|(accent, strength)| {
            (0..=((MAX_BPM - MIN_BPM) / STEP) as usize)
                .map(|i| {
                    let bpm = MIN_BPM + i as f64 * STEP;
                    let bump = |target: f64, amplitude: f64| {
                        amplitude * (-0.5 * ((bpm - target) / 0.65).powi(2)).exp()
                    };
                    bump(beat, 0.46) + bump(*accent, *strength)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
        let estimate = summarize(&curves);
        assert!((estimate.bpm.unwrap() - beat).abs() < 0.1, "{estimate:?}");
        assert!(estimate.ambiguous);
        assert!(
            estimate.confidence >= 0.25 && estimate.confidence < 0.7,
            "{estimate:?}"
        );
        assert!(estimate
            .candidates
            .iter()
            .any(|c| (c.bpm - beat * 2.0 / 3.0).abs() < 0.2));
    }
    #[test]
    fn changing_accents_in_real_pcm_keep_a_supported_tempo_visible() {
        let rate = 8000;
        let beat = 127.8;
        let mut pcm = Vec::new();
        for section in 0..3 {
            let competing = beat
                * if section == 0 {
                    2.0 / 3.0
                } else if section == 1 {
                    1.5
                } else {
                    0.5
                };
            for i in 0..rate * 28 {
                let t = i as f64 / rate as f64;
                let main = t.rem_euclid(60.0 / beat);
                let accent = t.rem_euclid(60.0 / competing);
                let sample =
                    (2.0 * std::f64::consts::PI * 75.0 * main).sin() * (-main * 55.0).exp() * 0.45
                        + (2.0 * std::f64::consts::PI * 170.0 * accent).sin()
                            * (-accent * 55.0).exp()
                            * 0.60;
                pcm.push([sample as f32; 2]);
            }
        }
        let result = estimate_tempo(&pcm, rate);
        assert!(result.bpm.is_some(), "{result:?}");
        assert!(result.confidence >= 0.16);
        assert!(
            result.candidates.iter().any(|c| (c.bpm - beat).abs() < 1.0),
            "{result:?}"
        );
    }
    #[test]
    fn weak_periodicity_is_still_withheld() {
        let curve = (0..=((MAX_BPM - MIN_BPM) / STEP) as usize)
            .map(|i| 0.1 * (-0.5 * ((MIN_BPM + i as f64 * STEP - 127.8) / 0.65).powi(2)).exp())
            .collect::<Vec<_>>();
        assert_eq!(summarize(&[curve]).bpm, None);
    }
}

#[cfg(test)]
mod phase_tests {
    use super::*;
    fn pulses(bpm: f64, offset: f64, seconds: u32, rate: u32, loud_hats: bool) -> Vec<[f32; 2]> {
        let period = 60.0 / bpm;
        (0..rate as usize * seconds as usize)
            .map(|i| {
                let t = i as f64 / rate as f64;
                if t < offset {
                    return [0.0; 2];
                }
                let beat = (t - offset).rem_euclid(period);
                let kick =
                    (2.0 * std::f64::consts::PI * 75.0 * beat).sin() * (-beat * 55.0).exp() * 0.55;
                let hat_time = (t - offset - period * 0.5).rem_euclid(period);
                let hat = if loud_hats {
                    (2.0 * std::f64::consts::PI * 2850.0 * hat_time).sin()
                        * (-hat_time * 140.0).exp()
                        * 1.2
                } else {
                    0.0
                };
                [(kick + hat) as f32, -(kick + hat) as f32]
            })
            .collect()
    }
    fn error(actual: f64, expected: f64, bpm: f64) -> f64 {
        let period = 60.0 / bpm;
        ((actual - expected + period * 0.5).rem_euclid(period) - period * 0.5).abs()
    }
    #[test]
    fn shifted_fractional_pulses_keep_nonzero_sample_time_phase() {
        for (bpm, offset, rate) in [
            (93.7, 0.137, 8000),
            (127.3, 0.213, 22050),
            (174.5, 0.041, 48000),
        ] {
            let phase =
                estimate_phase(&pulses(bpm, offset, 16, rate, false), rate, Some(bpm)).unwrap();
            assert!(
                error(phase.offset_seconds, offset, bpm) < 0.008,
                "{bpm}: {phase:?}"
            );
            assert!(phase.confidence > 0.35);
        }
    }
    #[test]
    fn loud_offbeat_hi_hats_do_not_replace_the_bass_beat() {
        let bpm = 127.3;
        let offset = 0.173;
        let phase = estimate_phase(&pulses(bpm, offset, 18, 8000, true), 8000, Some(bpm)).unwrap();
        assert!(
            error(phase.offset_seconds, offset, bpm) < 0.008,
            "{phase:?}"
        );
    }
    #[test]
    fn distributed_excerpts_preserve_absolute_phase_in_long_track() {
        let bpm = 127.3;
        let offset = 0.137;
        let phase = estimate_phase(&pulses(bpm, offset, 85, 8000, false), 8000, Some(bpm)).unwrap();
        assert!(
            error(phase.offset_seconds, offset, bpm) < 0.008,
            "{phase:?}"
        );
    }
    #[test]
    fn silence_noise_and_unknown_tempo_do_not_fabricate_a_grid() {
        let silence = vec![[0.0; 2]; 80000];
        assert!(estimate_phase(&silence, 8000, Some(127.3)).is_none());
        assert!(estimate_phase(&pulses(127.3, 0.173, 12, 8000, false), 8000, None).is_none());
        let mut seed = 89_u32;
        let noise = (0..80000)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                [seed as f32 / u32::MAX as f32 * 0.3; 2]
            })
            .collect::<Vec<_>>();
        assert!(estimate_phase(&noise, 8000, Some(127.3)).is_none());
    }
}

#[cfg(test)]
mod phase_stability_tests {
    use super::*;
    #[test]
    fn inconsistent_phase_across_sections_has_no_uniform_grid() {
        let rate = 8000;
        let bpm = 127.3;
        let mut pcm = Vec::new();
        for offset in [0.07, 0.25, 0.43] {
            for i in 0..rate * 28 {
                let t = i as f64 / rate as f64;
                let phase = (t - offset).rem_euclid(60.0 / bpm);
                let value = (2.0 * std::f64::consts::PI * 75.0 * phase).sin()
                    * (-phase * 55.0).exp()
                    * 0.55;
                pcm.push([value as f32; 2]);
            }
        }
        assert!(estimate_phase(&pcm, rate, Some(bpm)).is_none());
    }
}

#[cfg(test)]
mod realistic_kick_phase_tests {
    use super::*;
    use std::f64::consts::PI;
    /// A 3 ms click over a sub whose low-band energy builds for tens of
    /// milliseconds and then sustains, like the kicks measured on commercial
    /// house/EDM. A 5 ms RMS envelope saw each 40-60 Hz body cycle as a rise
    /// and put the grid 80-100 ms after the click; isolated fast-decay
    /// fixtures never exposed that.
    fn layered_kick(u: f64, sub_hz: f64, attack: f64, hold: f64, release: f64) -> f64 {
        let envelope = if u < attack {
            0.5 - 0.5 * (PI * u / attack).cos()
        } else if u < attack + hold {
            1.0
        } else {
            (-(u - attack - hold) / release).exp()
        };
        let glide = (90.0 - sub_hz) * 0.015 * (1.0 - (-u / 0.015).exp());
        let sub = (2.0 * PI * (sub_hz * u + glide)).sin() * envelope * 0.6;
        let click = ((2.0 * PI * 1800.0 * u).sin() * 0.5 + (2.0 * PI * 400.0 * u).sin())
            * (-u / 0.003).exp()
            * 0.35;
        sub + click
    }
    /// Pitch-swept kick with a 2 ms attack and a long 50 Hz tail.
    fn swept_kick(u: f64) -> f64 {
        let sweep = (150.0 - 50.0) / 30.0 * (1.0 - (-30.0 * u).exp());
        (2.0 * PI * (50.0 * u + sweep)).sin() * (u / 0.002).min(1.0) * (-u * 9.0).exp() * 0.7
    }
    /// Kicks on every beat from `offset`; optionally a sustained bass that the
    /// kick ducks (sidechain) and noise hats on the offbeat.
    fn track(
        bpm: f64,
        offset: f64,
        seconds: f64,
        rate: u32,
        kick: impl Fn(f64) -> f64,
        mix: bool,
    ) -> Vec<[f32; 2]> {
        let period = 60.0 / bpm;
        let mut seed = 17_u32;
        (0..(seconds * rate as f64) as usize)
            .map(|i| {
                let t = i as f64 / rate as f64;
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                if t < offset {
                    return [0.0; 2];
                }
                let u = (t - offset).rem_euclid(period);
                let mut value = kick(u);
                if mix {
                    let duck = (1.0 - (-u / 0.06).exp()).powi(2);
                    value += (2.0 * PI * 55.0 * t).sin() * 0.25 * duck;
                    let hat = (u - period * 0.5).rem_euclid(period);
                    let noise = seed as f64 / u32::MAX as f64 * 2.0 - 1.0;
                    value += noise * (-hat * 90.0).exp() * 0.15;
                }
                [value as f32; 2]
            })
            .collect()
    }
    fn error(actual: f64, expected: f64, bpm: f64) -> f64 {
        let period = 60.0 / bpm;
        ((actual - expected + period * 0.5).rem_euclid(period) - period * 0.5).abs()
    }
    #[test]
    fn layered_kicks_place_the_grid_on_the_click_not_the_sub_body() {
        for (bpm, offset, rate, sub_hz, attack, hold, release) in [
            (112.0, 0.137, 44100, 50.0, 0.040, 0.10, 0.08),
            (125.0, 0.211, 48000, 45.0, 0.030, 0.12, 0.08),
        ] {
            let pcm = track(
                bpm,
                offset,
                30.0,
                rate,
                |u| layered_kick(u, sub_hz, attack, hold, release),
                false,
            );
            let phase = estimate_phase(&pcm, rate, Some(bpm)).unwrap();
            assert!(
                error(phase.offset_seconds, offset, bpm) < 0.008,
                "{bpm}: {phase:?}"
            );
            assert!(phase.confidence > 0.35, "{bpm}: {phase:?}");
        }
    }
    #[test]
    fn sidechained_bass_and_offbeat_hats_keep_the_kick_phase() {
        for (bpm, offset, rate) in [(112.0, 0.137, 44100), (127.3, 0.05, 48000)] {
            let pcm = track(bpm, offset, 40.0, rate, swept_kick, true);
            let phase = estimate_phase(&pcm, rate, Some(bpm)).unwrap();
            assert!(
                error(phase.offset_seconds, offset, bpm) < 0.008,
                "{bpm}: {phase:?}"
            );
        }
    }
    #[test]
    fn bpm_rounding_drift_between_excerpts_keeps_a_grid_centred_on_the_kicks() {
        // A 0.05 BPM tempo error moves the kicks 47 ms between the first and
        // last excerpt. That is not conflicting phase: the grid at the supplied
        // BPM should sit on the kicks mid-track and split the unavoidable drift.
        let (bpm, offset, rate) = (124.0, 0.173, 8000);
        let pcm = track(
            bpm,
            offset,
            200.0,
            rate,
            |u| layered_kick(u, 50.0, 0.03, 0.08, 0.06),
            false,
        );
        let rounded = bpm + 0.05;
        let phase = estimate_phase(&pcm, rate, Some(rounded)).unwrap();
        let (period, rounded_period) = (60.0 / bpm, 60.0 / rounded);
        for (time, limit) in [(100.0, 0.008), (41.0, 0.030), (159.0, 0.030)] {
            let kick = offset + ((time - offset) / period).round() * period;
            let line = phase.offset_seconds
                + ((kick - phase.offset_seconds) / rounded_period).round() * rounded_period;
            assert!(
                (line - kick).abs() < limit,
                "{time}: {:.1} ms",
                (line - kick) * 1e3
            );
        }
    }
}
