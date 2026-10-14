//! Beat This! feasibility spike: run the exported small model from Rust with
//! tract and compare every stage against the Python reference outputs.
//! Results and method: research/beat-model-spike.md. Setup: ../README.md.
//!
//! Environment switches:
//! - `SPIKE_ROOT`: folder holding `models/` and `ref/` (default: the spike folder)
//! - `SPIKE_MODEL`: ONNX file name inside `models/`
//! - `SPIKE_THREADS`: tract matrix-multiply threads (default 1)
//! - `SPIKE_F16=1`: run the model in float16
//! - `SPIKE_GAPLESS=1`: trim MP3 encoder delay (the app does not)
//! - `SPIKE_RS_CHUNK`: resampler chunk size (default 4096)
//! - `SPIKE_ONLY`: run one reference track, e.g. `t2`
//! - `SPIKE_MODE=infer-only` with `SPIKE_CHUNKS`: time and size one chunk

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use rustfft::{num_complex::Complex32, FftPlanner};
use serde_json::{json, Value};
use tract_onnx::prelude::*;

const SR: u32 = 22050;
const NFFT: usize = 1024;
const HOP: usize = 441;
const NMELS: usize = 128;
const NBINS: usize = NFFT / 2 + 1;
const CHUNK: usize = 1500;
const BORDER: usize = 6;
const FPS: f64 = 50.0;

type Model = Arc<TypedRunnableModel>;

fn main() {
    let root = std::env::var_os("SPIKE_ROOT").map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
    });
    let reference = root.join("ref");
    let t = Instant::now();
    let mut typed = tract_onnx::onnx()
        .model_for_path(root.join("models").join(std::env::var("SPIKE_MODEL").unwrap_or("beat_this_small0.onnx".into())))
        .unwrap()
        .with_input_fact(0, f32::fact([1, CHUNK, NMELS]).into())
        .unwrap()
        .into_typed()
        .unwrap();
    if half() {
        typed = tract_core::transform::get_transform("f32_to_f16")
            .unwrap()
            .unwrap()
            .transform_into(typed)
            .unwrap();
    }
    let model: Model = typed.into_optimized().unwrap().into_runnable().unwrap();
    let load_ms = t.elapsed().as_secs_f64() * 1e3;
    let threads: usize = env_or("SPIKE_THREADS", 1);
    if threads > 1 {
        tract_linalg::multithread::set_default_executor(
            tract_linalg::multithread::Executor::multithread(threads),
        );
    }
    if std::env::var("SPIKE_MODE").as_deref() == Ok("infer-only") {
        // Peak memory of one chunk, measured externally with /usr/bin/time -l.
        let t = Instant::now();
        let n: usize = env_or("SPIKE_CHUNKS", 1);
        for _ in 0..n {
            let input = Tensor::from_shape(&[1, CHUNK, NMELS], &vec![0f32; CHUNK * NMELS]).unwrap();
            model.run(tvec!(to_model(input).into())).unwrap();
        }
        println!("{{\"threads\": {threads}, \"chunks\": {n}, \"ms_per_chunk\": {:.1}}}",
            t.elapsed().as_secs_f64() * 1e3 / n as f64);
        return;
    }
    let mel = Mel::new();
    let fb_err = mel.compare_filterbank(&reference.join("melfb.f32"));
    let index: Vec<Value> =
        serde_json::from_str(&fs::read_to_string(reference.join("index.json")).unwrap()).unwrap();
    let only = std::env::var("SPIKE_ONLY").ok();
    let mut rows = Vec::new();
    for item in index {
        let name = item["name"].as_str().unwrap();
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let path = item["path"].as_str().unwrap();
        let py: Value =
            serde_json::from_str(&fs::read_to_string(reference.join(format!("{name}.json"))).unwrap())
                .unwrap();
        rows.push(run_track(&model, &mel, &reference, name, path, &py));
    }
    let out = json!({ "model_load_ms": load_ms, "melfb_max_abs_err": fb_err, "tracks": rows });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

fn run_track(model: &Model, mel: &Mel, reference: &Path, name: &str, path: &str, py: &Value) -> Value {
    // Stage checks on the Python reference inputs.
    let py_sig = read_f32(&reference.join(format!("{name}.sig.f32")));
    let py_spect = read_f32(&reference.join(format!("{name}.spect.f32")));
    let py_beat = read_f32(&reference.join(format!("{name}.beat.f32")));
    let py_down = read_f32(&reference.join(format!("{name}.downbeat.f32")));
    let spect_from_py_sig = mel.compute(&py_sig);
    let spect_err = diff(&spect_from_py_sig, &py_spect);
    let (b_from_py, d_from_py) = infer(model, &py_spect);
    let logit_err = diff(&b_from_py, &py_beat).max(diff(&d_from_py, &py_down));

    // Full Rust path, timed: decode, mono, resample, mel, model, peaks.
    let t0 = Instant::now();
    let (stereo, rate) = decode(Path::new(path));
    let t1 = Instant::now();
    let mono: Vec<f32> = stereo.iter().map(|f| 0.5 * (f[0] + f[1])).collect();
    let sig = resample(&mono, rate);
    let t2 = Instant::now();
    let spect = mel.compute(&sig);
    let t3 = Instant::now();
    let (beat_logits, down_logits) = infer(model, &spect);
    let t4 = Instant::now();
    let (beats, downbeats) = postprocess(&beat_logits, &down_logits);
    let t5 = Instant::now();

    let py_beats: Vec<f64> = py["beats"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let py_downs: Vec<f64> =
        py["downbeats"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let grid = fit_grid(&beats, &downbeats);
    let t6 = Instant::now();
    let (dsp_tempo, dsp_phase) = audio_analysis::analyze_rhythm(&stereo, rate);
    let dsp_ms = t6.elapsed().as_secs_f64() * 1e3;
    let duration = stereo.len() as f64 / rate as f64;
    let dsp_bpm = dsp_tempo.bpm;
    let phase_gap_ms = match (&grid, &dsp_phase) {
        (Some(g), Some(p)) => {
            // Compare at the DSP's beat period after resolving half/double.
            let period = 60.0 / dsp_bpm.unwrap_or(g.bpm);
            let d = (g.offset - p.offset_seconds).rem_euclid(period);
            Some(d.min(period - d) * 1e3)
        }
        _ => None,
    };
    json!({
        "name": name,
        "file": Path::new(path).file_name().unwrap().to_string_lossy(),
        "source_rate": rate,
        "duration_s": round(duration, 1),
        "stage_check": {
            "rust_mel_vs_python_max_abs": spect_err,
            "tract_vs_torch_max_logit_diff": logit_err,
            "frames_rust": spect.len() / NMELS,
            "frames_python": py["frames"],
        },
        "end_to_end_vs_python": {
            "beats": [beats.len(), py_beats.len()],
            "beat_f_20ms": f_measure(&beats, &py_beats, 0.020),
            "beat_f_70ms": f_measure(&beats, &py_beats, 0.070),
            "downbeat_f_20ms": f_measure(&downbeats, &py_downs, 0.020),
            "median_beat_shift_ms": median_shift_ms(&beats, &py_beats),
        },
        "timing_ms": {
            "decode": ms(t0, t1), "resample": ms(t1, t2), "mel": ms(t2, t3),
            "model": ms(t3, t4), "peaks": ms(t4, t5),
            "total_excluding_decode": ms(t1, t5),
            "existing_dsp_rhythm": dsp_ms,
        },
        "model_grid": grid.as_ref().map(|g| json!({
            "bpm": round(g.bpm, 2), "beat1_offset_s": round(g.offset, 3),
            "beats_on_grid_35ms": round(g.on_grid, 3), "median_grid_error_ms": round(g.median_err_ms, 1),
            "downbeat_phase_agreement": round(g.bar_agreement, 3),
        })),
        "existing_dsp": json!({
            "bpm": dsp_bpm.map(|b| round(b, 2)), "ambiguous": dsp_tempo.ambiguous,
            "confidence": round(dsp_tempo.confidence as f64, 3),
            "phase_offset_s": dsp_phase.as_ref().map(|p| round(p.offset_seconds, 3)),
            "phase_gap_to_model_ms": phase_gap_ms.map(|x| round(x, 1)),
        }),
    })
}

// ---------- audio front end ----------

fn decode(path: &Path) -> (Vec<[f32; 2]>, u32) {
    use symphonia::core::{
        audio::SampleBuffer, codecs::DecoderOptions, errors::Error, formats::FormatOptions,
        io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
    };
    let file = fs::File::open(path).unwrap();
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    // Default matches the app's decoder (decode.rs); SPIKE_GAPLESS=1 trims
    // encoder delay/padding the way most other decoders do.
    let options = FormatOptions {
        enable_gapless: std::env::var("SPIKE_GAPLESS").as_deref() == Ok("1"),
        ..Default::default()
    };
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &options, &MetadataOptions::default())
        .unwrap();
    let mut format = probed.format;
    let track = format.default_track().unwrap();
    let id = track.id;
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).unwrap();
    let mut rate = track.codec_params.sample_rate.unwrap_or(44100);
    let mut out = Vec::new();
    let mut buf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        if packet.track_id() != id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(Error::DecodeError(_)) => continue,
            Err(_) => break,
        };
        let spec = *decoded.spec();
        rate = spec.rate;
        let ch = spec.channels.count();
        if buf.as_ref().is_none_or(|b| b.capacity() < decoded.capacity() * ch) {
            buf = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
        }
        let b = buf.as_mut().unwrap();
        b.copy_interleaved_ref(decoded);
        for f in b.samples().chunks(ch) {
            out.push([f[0], if ch > 1 { f[1] } else { f[0] }]);
        }
    }
    (out, rate)
}

fn resample(mono: &[f32], rate: u32) -> Vec<f32> {
    if rate == SR {
        return mono.to_vec();
    }
    let chunk: usize = env_or("SPIKE_RS_CHUNK", 4096);
    let mut r = Fft::<f32>::new_custom(rate as usize, SR as usize, chunk, 1, 1, rubato::WindowFunction::BlackmanHarris2, FixedSync::Input).unwrap();
    let input = InterleavedSlice::new(mono, 1, mono.len()).unwrap();
    r.process_all(&input, mono.len(), None).unwrap().take_data()
}

struct Mel {
    window: Vec<f32>,
    /// Per mel band: first STFT bin and its weights (torchaudio slaney scale, no area norm).
    bands: Vec<(usize, Vec<f32>)>,
    dense: Vec<f32>,
    fft: Arc<dyn rustfft::Fft<f32>>,
}

impl Mel {
    fn new() -> Self {
        let window = (0..NFFT)
            .map(|n| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / NFFT as f64).cos()) as f32)
            .collect();
        // torchaudio.functional.melscale_fbanks(513, 30, 11000, 128, 22050, None, "slaney")
        let (m_min, m_max) = (hz_to_mel(30.0), hz_to_mel(11000.0));
        let f_pts: Vec<f64> = (0..NMELS + 2)
            .map(|i| mel_to_hz(m_min + (m_max - m_min) * i as f64 / (NMELS + 1) as f64))
            .collect();
        let all_freqs: Vec<f64> = (0..NBINS).map(|k| (SR / 2) as f64 * k as f64 / (NBINS - 1) as f64).collect();
        let mut dense = vec![0f32; NBINS * NMELS];
        let mut bands = Vec::with_capacity(NMELS);
        for m in 0..NMELS {
            let mut first = None;
            let mut weights = Vec::new();
            for (k, &f) in all_freqs.iter().enumerate() {
                let down = (f - f_pts[m]) / (f_pts[m + 1] - f_pts[m]);
                let up = (f_pts[m + 2] - f) / (f_pts[m + 2] - f_pts[m + 1]);
                let w = down.min(up).max(0.0) as f32;
                dense[k * NMELS + m] = w;
                if w > 0.0 {
                    first.get_or_insert(k);
                    weights.push(w);
                } else if first.is_some() {
                    break;
                }
            }
            bands.push((first.unwrap_or(0), weights));
        }
        let fft = FftPlanner::new().plan_fft_forward(NFFT);
        Self { window, bands, dense, fft }
    }

    fn compare_filterbank(&self, path: &Path) -> f32 {
        diff(&self.dense, &read_f32(path))
    }

    /// Log-mel frames (row-major frames × 128) matching beat_this LogMelSpect:
    /// centered reflect-padded STFT, periodic Hann, magnitude / sqrt(n_fft),
    /// mel projection, then ln(1 + 1000 x).
    fn compute(&self, x: &[f32]) -> Vec<f32> {
        let len = x.len();
        let pad = NFFT / 2;
        let reflect = |i: isize| -> f32 {
            let n = len as isize;
            let mut j = i - pad as isize;
            if j < 0 {
                j = -j;
            }
            if j >= n {
                j = 2 * (n - 1) - j;
            }
            x[j.clamp(0, n - 1) as usize]
        };
        let frames = 1 + len / HOP;
        let scale = 1.0 / (NFFT as f32).sqrt();
        let mut out = vec![0f32; frames * NMELS];
        let mut buf = vec![Complex32::new(0.0, 0.0); NFFT];
        let mut scratch = vec![Complex32::new(0.0, 0.0); self.fft.get_inplace_scratch_len()];
        let mut mag = vec![0f32; NBINS];
        for t in 0..frames {
            let start = t * HOP;
            for n in 0..NFFT {
                let s = if start + n >= pad && start + n - pad < len {
                    x[start + n - pad]
                } else {
                    reflect((start + n) as isize)
                };
                buf[n] = Complex32::new(s * self.window[n], 0.0);
            }
            self.fft.process_with_scratch(&mut buf, &mut scratch);
            for k in 0..NBINS {
                mag[k] = buf[k].norm() * scale;
            }
            let row = &mut out[t * NMELS..(t + 1) * NMELS];
            for (m, (first, weights)) in self.bands.iter().enumerate() {
                let e: f32 = weights.iter().zip(&mag[*first..]).map(|(w, v)| w * v).sum();
                row[m] = (1.0 + 1000.0 * e).ln();
            }
        }
        out
    }
}

fn hz_to_mel(f: f64) -> f64 {
    let (f_sp, min_log_hz) = (200.0 / 3.0, 1000.0);
    let logstep = 6.4f64.ln() / 27.0;
    if f >= min_log_hz { min_log_hz / f_sp + (f / min_log_hz).ln() / logstep } else { f / f_sp }
}

fn mel_to_hz(m: f64) -> f64 {
    let (f_sp, min_log_hz) = (200.0 / 3.0, 1000.0);
    let min_log_mel = min_log_hz / f_sp;
    let logstep = 6.4f64.ln() / 27.0;
    if m >= min_log_mel { min_log_hz * (logstep * (m - min_log_mel)).exp() } else { f_sp * m }
}

// ---------- model ----------

/// beat_this split_predict_aggregate: 1500-frame chunks, 6-frame borders,
/// "keep_first" overlap, last chunk aligned to the end of the piece.
fn infer(model: &Model, spect: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let len = spect.len() / NMELS;
    let step = (CHUNK - 2 * BORDER) as isize;
    let mut starts: Vec<isize> = (0..)
        .map(|i| -(BORDER as isize) + i * step)
        .take_while(|&s| s < len as isize - BORDER as isize)
        .collect();
    if len > CHUNK - 2 * BORDER {
        *starts.last_mut().unwrap() = len as isize - (CHUNK - BORDER) as isize;
    }
    let mut beat = vec![-1000f32; len];
    let mut down = vec![-1000f32; len];
    for &start in starts.iter().rev() {
        let mut chunk = vec![0f32; CHUNK * NMELS];
        for i in 0..CHUNK {
            let src = start + i as isize;
            if src >= 0 && (src as usize) < len {
                let s = src as usize;
                chunk[i * NMELS..(i + 1) * NMELS].copy_from_slice(&spect[s * NMELS..(s + 1) * NMELS]);
            }
        }
        let input = Tensor::from_shape(&[1, CHUNK, NMELS], &chunk).unwrap();
        let out = model.run(tvec!(to_model(input).into())).unwrap();
        let (b, d) = (out[0].cast_to::<f32>().unwrap(), out[1].cast_to::<f32>().unwrap());
        let b = b.to_plain_array_view::<f32>().unwrap();
        let d = d.to_plain_array_view::<f32>().unwrap();
        for i in BORDER..CHUNK - BORDER {
            let dst = start + i as isize;
            if dst >= 0 && (dst as usize) < len {
                beat[dst as usize] = b[[0, i]];
                down[dst as usize] = d[[0, i]];
            }
        }
    }
    (beat, down)
}

/// beat_this "minimal" postprocessing.
fn postprocess(beat: &[f32], down: &[f32]) -> (Vec<f64>, Vec<f64>) {
    let peaks = |x: &[f32]| -> Vec<f64> {
        let frames: Vec<usize> = (0..x.len())
            .filter(|&i| {
                let lo = i.saturating_sub(3);
                let hi = (i + 4).min(x.len());
                x[i] > 0.0 && x[lo..hi].iter().all(|&v| v <= x[i])
            })
            .collect();
        dedupe(&frames).into_iter().map(|f| f / FPS).collect()
    };
    let beats = peaks(beat);
    let mut downs: Vec<f64> = peaks(down)
        .into_iter()
        .map(|d| {
            *beats
                .iter()
                .min_by(|a, b| (*a - d).abs().total_cmp(&(*b - d).abs()))
                .unwrap_or(&d)
        })
        .collect();
    downs.sort_by(f64::total_cmp);
    downs.dedup();
    (beats, downs)
}

fn dedupe(frames: &[usize]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut it = frames.iter();
    let Some(&first) = it.next() else { return out };
    let (mut p, mut c) = (first as f64, 1.0);
    for &f in it {
        let f = f as f64;
        if f - p <= 1.0 {
            c += 1.0;
            p += (f - p) / c;
        } else {
            out.push(p);
            p = f;
            c = 1.0;
        }
    }
    out.push(p);
    out
}

// ---------- constant grid fit ----------

struct Grid {
    bpm: f64,
    offset: f64,
    on_grid: f64,
    median_err_ms: f64,
    bar_agreement: f64,
}

/// Fit the app's constant grid (BPM + offset) to the model's beats and put the
/// offset on the dominant downbeat phase.
fn fit_grid(beats: &[f64], downbeats: &[f64]) -> Option<Grid> {
    if beats.len() < 8 {
        return None;
    }
    let mut ibi: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    ibi.sort_by(f64::total_cmp);
    let bpm0 = 60.0 / ibi[ibi.len() / 2];
    // Search the BPM (±4% of the median interval, 0.005 BPM steps) whose best
    // phase puts the most beats within 35 ms of a grid line. Frame-quantized
    // intervals make the median alone too coarse for a whole-track grid.
    let phase_for = |period: f64| -> (f64, usize) {
        let w = std::f64::consts::TAU / period;
        let (c, s) = beats.iter().fold((0.0, 0.0), |a, &t| (a.0 + (w * t).cos(), a.1 + (w * t).sin()));
        let origin = s.atan2(c) / w;
        let hits = beats
            .iter()
            .filter(|&&t| {
                let r = (t - origin).rem_euclid(period);
                r.min(period - r) <= 0.035
            })
            .count();
        (origin, hits)
    };
    let steps = (bpm0 * 0.08 / 0.005) as i64;
    let (mut period, mut origin) = (0..=steps)
        .map(|i| 60.0 / (bpm0 * 0.96 + i as f64 * 0.005))
        .map(|p| (p, phase_for(p)))
        .max_by_key(|(_, (_, hits))| *hits)
        .map(|(p, (o, _))| (p, o))
        .unwrap();
    // Least-squares refinement over the inliers of the winning grid.
    let pts: Vec<(f64, f64)> = beats
        .iter()
        .map(|&t| (((t - origin) / period).round(), t))
        .filter(|&(k, t)| (t - (origin + k * period)).abs() <= 0.035)
        .collect();
    if pts.len() >= 8 {
        let n = pts.len() as f64;
        let (sk, st) = pts.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
        let (mk, mt) = (sk / n, st / n);
        let num: f64 = pts.iter().map(|p| (p.0 - mk) * (p.1 - mt)).sum();
        let den: f64 = pts.iter().map(|p| (p.0 - mk).powi(2)).sum();
        period = num / den;
        origin = mt - period * mk;
    }
    let mut errs: Vec<f64> = beats
        .iter()
        .map(|&t| {
            let k = ((t - origin) / period).round();
            (t - (origin + k * period)).abs()
        })
        .collect();
    let on_grid = errs.iter().filter(|&&e| e <= 0.035).count() as f64 / errs.len() as f64;
    errs.sort_by(f64::total_cmp);
    let median_err_ms = errs[errs.len() / 2] * 1e3;
    let mut votes = [0usize; 4];
    for &d in downbeats {
        let k = ((d - origin) / period).round() as i64;
        votes[k.rem_euclid(4) as usize] += 1;
    }
    let (phase, best) = votes.iter().enumerate().max_by_key(|v| *v.1).unwrap();
    let bar = 4.0 * period;
    let offset = (origin + phase as f64 * period).rem_euclid(bar);
    Some(Grid {
        bpm: 60.0 / period,
        offset,
        on_grid,
        median_err_ms,
        bar_agreement: *best as f64 / downbeats.len().max(1) as f64,
    })
}

// ---------- comparisons ----------

fn f_measure(est: &[f64], refs: &[f64], tol: f64) -> f64 {
    if est.is_empty() || refs.is_empty() {
        return 0.0;
    }
    let mut used = vec![false; refs.len()];
    let mut hits = 0;
    for &e in est {
        if let Some((i, _)) = refs
            .iter()
            .enumerate()
            .filter(|(i, r)| !used[*i] && (*r - e).abs() <= tol)
            .min_by(|a, b| (a.1 - e).abs().total_cmp(&(b.1 - e).abs()))
        {
            used[i] = true;
            hits += 1;
        }
    }
    let p = hits as f64 / est.len() as f64;
    let r = hits as f64 / refs.len() as f64;
    if p + r == 0.0 { 0.0 } else { round(2.0 * p * r / (p + r), 4) }
}

fn median_shift_ms(est: &[f64], refs: &[f64]) -> f64 {
    let mut shifts: Vec<f64> = est
        .iter()
        .filter_map(|&e| {
            refs.iter()
                .map(|r| e - r)
                .min_by(|a, b| a.abs().total_cmp(&b.abs()))
                .filter(|d| d.abs() < 0.1)
        })
        .collect();
    if shifts.is_empty() {
        return f64::NAN;
    }
    shifts.sort_by(f64::total_cmp);
    round(shifts[shifts.len() / 2] * 1e3, 1)
}

fn diff(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::INFINITY;
    }
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

fn read_f32(path: &Path) -> Vec<f32> {
    fs::read(path)
        .unwrap()
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn ms(a: Instant, b: Instant) -> f64 {
    round((b - a).as_secs_f64() * 1e3, 1)
}

fn round(x: f64, digits: i32) -> f64 {
    let p = 10f64.powi(digits);
    (x * p).round() / p
}

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn half() -> bool {
    std::env::var("SPIKE_F16").as_deref() == Ok("1")
}

fn to_model(t: Tensor) -> Tensor {
    if half() { t.cast_to::<f16>().unwrap().into_owned() } else { t }
}
