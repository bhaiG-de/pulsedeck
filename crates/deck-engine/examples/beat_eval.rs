//! Score the beat model against the v3 DSP grid on hand-checked tracks.
//!
//! Annotations come from `tools/export_beat_annotations.sh`: a JSON list of
//! `{path, title, bpm, downbeat_seconds, genre}` taken from manual grids.
//!
//! ```sh
//! cargo run --release -p deck-engine --example beat_eval -- annotations.json
//! cargo run --release -p deck-engine --example beat_eval -- --report a.mp3 b.mp3
//! ```
//!
//! Metrics per track, for each estimate (BPM E, offset O) against the
//! annotation (BPM A, downbeat D), over the whole track:
//! - tempo: E within 2% of A; half/double counts E within 2% of A/2 or 2A
//! - beats: share of annotated beats within 35 ms of an estimated grid line
//! - bars: share of annotated downbeats within 35 ms of an estimated bar line
//!   (every fourth grid line from O); beat 1 counts as right at 90% or more
use std::{path::Path, time::Instant};

use deck_engine::{analyze_file, analyze_file_with_beat_model};
use serde_json::Value;

const TOLERANCE: f64 = 0.035;

struct Score {
    tempo: bool,
    octave: bool,
    beats: f64,
    bars: f64,
}

#[derive(Default)]
struct Totals {
    n: usize,
    tempo: usize,
    octave: usize,
    beat_one: usize,
    beats: f64,
    bars: f64,
}

impl Totals {
    fn add(&mut self, s: &Score) {
        self.n += 1;
        self.tempo += s.tempo as usize;
        self.octave += s.octave as usize;
        self.beat_one += (s.bars >= 0.9) as usize;
        self.beats += s.beats;
        self.bars += s.bars;
    }
    fn row(&self, label: &str) -> String {
        let pct = |x: usize| 100.0 * x as f64 / self.n.max(1) as f64;
        format!(
            "| {label} | {} | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {:.0}% |",
            self.n,
            pct(self.tempo),
            pct(self.octave),
            100.0 * self.beats / self.n.max(1) as f64,
            100.0 * self.bars / self.n.max(1) as f64,
            pct(self.beat_one),
        )
    }
}

fn score(
    est_bpm: Option<f64>,
    est_offset: Option<f64>,
    bpm: f64,
    downbeat: f64,
    duration: f64,
) -> Score {
    let Some(e) = est_bpm.filter(|b| b.is_finite() && *b > 0.0) else {
        return Score {
            tempo: false,
            octave: false,
            beats: 0.0,
            bars: 0.0,
        };
    };
    let o = est_offset.unwrap_or(0.0);
    let close = |x: f64| (e / x - 1.0).abs() <= 0.02;
    let near = |t: f64, origin: f64, period: f64| {
        let r = (t - origin).rem_euclid(period);
        r.min(period - r) <= TOLERANCE
    };
    let (beat, est_beat) = (60.0 / bpm, 60.0 / e);
    // Annotated beat indices k (time D + k·beat) inside the track; downbeats
    // are the indices divisible by four.
    let first = ((0.5 - downbeat) / beat).ceil() as i64;
    let indices: Vec<i64> = (first..)
        .take_while(|&k| downbeat + k as f64 * beat < duration - 0.5)
        .collect();
    let share = |downbeats_only: bool, period: f64| {
        let times = indices
            .iter()
            .filter(|&&k| !downbeats_only || k.rem_euclid(4) == 0)
            .map(|&k| downbeat + k as f64 * beat);
        let (hits, n) = times.fold((0, 0), |(h, n), t| (h + near(t, o, period) as usize, n + 1));
        hits as f64 / n.max(1) as f64
    };
    Score {
        tempo: close(bpm),
        octave: close(bpm) || close(bpm / 2.0) || close(bpm * 2.0),
        beats: share(false, est_beat),
        bars: share(true, 4.0 * est_beat),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--report") {
        return report(&args[1..]);
    }
    let path = args
        .first()
        .expect("usage: beat_eval annotations.json | --report files...");
    let items: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read annotations"))
            .expect("annotations are a JSON list");
    let mut groups: Vec<(String, Totals, Totals)> = Vec::new();
    let mut all = (Totals::default(), Totals::default());
    let (mut model_seconds, mut audio_seconds) = (0.0, 0.0);
    println!("| Track | Genre | Annotated BPM | DSP BPM | Model BPM | DSP bars | Model bars |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    for item in &items {
        let file = item["path"].as_str().unwrap_or_default();
        let (Some(bpm), Some(downbeat)) = (item["bpm"].as_f64(), item["downbeat_seconds"].as_f64())
        else {
            eprintln!("skipping incomplete annotation for {file}");
            continue;
        };
        let genre = item["genre"].as_str().unwrap_or("unlabelled").to_string();
        let dsp = match analyze_file(Path::new(file)) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("skipping {file}: {e}");
                continue;
            }
        };
        let started = Instant::now();
        let (model, warning) =
            analyze_file_with_beat_model(Path::new(file), &|| false).expect("decode");
        model_seconds += started.elapsed().as_secs_f64();
        audio_seconds += dsp.duration_seconds;
        if let Some(w) = warning {
            eprintln!("{file}: beat model failed: {w}");
        }
        let grid = model.beat_model.as_ref().and_then(|s| s.usable_grid());
        let d = score(
            dsp.bpm,
            dsp.beatgrid_offset_seconds,
            bpm,
            downbeat,
            dsp.duration_seconds,
        );
        let m = score(
            grid.map(|g| g.bpm),
            grid.map(|g| g.downbeat_offset_seconds),
            bpm,
            downbeat,
            dsp.duration_seconds,
        );
        println!(
            "| {} | {genre} | {bpm:.2} | {} | {} | {:.0}% | {:.0}% |",
            item["title"].as_str().unwrap_or(file),
            dsp.bpm.map_or("none".into(), |b| format!("{b:.2}")),
            grid.map_or("none".into(), |g| format!("{:.2}", g.bpm)),
            100.0 * d.bars,
            100.0 * m.bars,
        );
        let slot = match groups.iter().position(|g| g.0 == genre) {
            Some(i) => i,
            None => {
                groups.push((genre.clone(), Totals::default(), Totals::default()));
                groups.len() - 1
            }
        };
        groups[slot].1.add(&d);
        groups[slot].2.add(&m);
        all.0.add(&d);
        all.1.add(&m);
    }
    println!("\n| Group | Tracks | Tempo ±2% | Incl. half/double | Beats on grid | Bars on grid | Beat 1 right |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    for (genre, dsp, model) in &groups {
        println!("{}", dsp.row(&format!("{genre}, DSP v3")));
        println!("{}", model.row(&format!("{genre}, model")));
    }
    println!("{}", all.0.row("all, DSP v3"));
    println!("{}", all.1.row("all, model"));
    if audio_seconds > 0.0 {
        println!(
            "\nAnalysis with the model: {:.2} s per minute of audio (decode + DSP + model).",
            model_seconds / (audio_seconds / 60.0)
        );
    }
}

/// Side-by-side tempo and timing estimates without annotations.
fn report(files: &[String]) {
    println!("| File | DSP BPM | DSP offset | Model BPM | Model beat 1 | On grid | Bar agreement | Seconds |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    for file in files {
        let started = Instant::now();
        match analyze_file_with_beat_model(Path::new(file), &|| false) {
            Ok((model, warning)) => {
                let dsp = analyze_file(Path::new(file)).ok();
                let grid = model.beat_model.as_ref().and_then(|s| s.grid);
                let name = Path::new(file)
                    .file_name()
                    .map_or(file.clone(), |n| n.to_string_lossy().into());
                let opt =
                    |x: Option<f64>, p: usize| x.map_or("none".into(), |v| format!("{v:.p$}"));
                println!(
                    "| {name} | {} | {} | {} | {} | {} | {} | {:.1} |",
                    opt(dsp.as_ref().and_then(|a| a.bpm), 2),
                    opt(dsp.as_ref().and_then(|a| a.beatgrid_offset_seconds), 3),
                    opt(grid.map(|g| g.bpm), 2),
                    opt(grid.map(|g| g.downbeat_offset_seconds), 3),
                    grid.map_or("none".into(), |g| format!("{:.0}%", 100.0 * g.on_grid)),
                    grid.map_or("none".into(), |g| format!(
                        "{:.0}%",
                        100.0 * g.bar_agreement
                    )),
                    started.elapsed().as_secs_f64(),
                );
                if let Some(w) = warning {
                    eprintln!("{file}: beat model failed: {w}");
                }
            }
            Err(e) => eprintln!("{file}: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoring_separates_tempo_octave_and_bar_phase() {
        let exact = score(
            Some(124.0),
            Some(0.5),
            124.0,
            0.5 + 4.0 * 60.0 / 124.0,
            120.0,
        );
        assert!(exact.tempo && exact.octave);
        assert!(exact.beats > 0.99 && exact.bars > 0.99);
        // Right beats, wrong bar phase: beat 1 is one beat late.
        let shifted = score(Some(124.0), Some(0.5 + 60.0 / 124.0), 124.0, 0.5, 120.0);
        assert!(shifted.beats > 0.99 && shifted.bars < 0.01);
        let double = score(Some(248.0), Some(0.5), 124.0, 0.5, 120.0);
        assert!(!double.tempo && double.octave);
        let none = score(None, None, 124.0, 0.5, 120.0);
        assert_eq!((none.beats, none.bars), (0.0, 0.0));
    }
}
