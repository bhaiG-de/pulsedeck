//! Offline preparation benchmark; never starts audio or writes the user's library.
use deck_engine::{analysis_cache_counts, EngineHandle, TrackAnalysis};
use std::{path::Path, time::Instant};
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let manifest = args.first().is_some_and(|s| s == "--manifest");
    let paths: Vec<String> = if manifest {
        let rows: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(&args[1]).unwrap()).unwrap();
        rows.into_iter()
            .map(|r| r["path"].as_str().unwrap().to_owned())
            .collect()
    } else {
        vec![args
            .first()
            .expect("audio path or --manifest cohort.json")
            .clone()]
    };
    let cached: Option<TrackAnalysis> = if manifest {
        None
    } else {
        args.get(1)
            .filter(|s| !s.starts_with("--"))
            .map(|p| serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap())
    };
    let repeats = args
        .iter()
        .position(|s| s == "--repeat")
        .map(|i| args[i + 1].parse::<usize>().unwrap())
        .unwrap_or(1);
    let engine = EngineHandle::new().unwrap();
    for path in &paths {
        let path = Path::new(path);
        for _ in 0..repeats {
            let counts = analysis_cache_counts();
            let started = Instant::now();
            let metadata = deck_engine::probe_file(path).unwrap();
            let probe_ms = started.elapsed().as_secs_f64() * 1000.;
            let started = Instant::now();
            let mut analysis = engine.load_track_cached(0, path, cached.clone()).unwrap();
            let decode_ms = started.elapsed().as_secs_f64() * 1000.;
            if (manifest || args.iter().any(|s| s == "--analyze"))
                && !deck_engine::playback_analysis_ready(&analysis)
            {
                analysis = engine.complete_loaded_analysis(0).unwrap();
            }
            let elapsed = started.elapsed().as_secs_f64() * 1000.;
            println!(
                "{}",
                serde_json::json!({"path":path,"probe_ms":probe_ms,"load_ms":elapsed,"decode_ms":decode_ms,"duration_seconds":metadata.duration_seconds,"ready":deck_engine::playback_analysis_ready(&analysis),"timings":deck_engine::take_analysis_diagnostics(),"analysis_runs":std::array::from_fn::<_,4,_>(|i|analysis_cache_counts()[i]-counts[i])})
            );
            if let Some(i) = args.iter().position(|s| s == "--out") {
                std::fs::write(&args[i + 1], serde_json::to_vec(&analysis).unwrap()).unwrap();
            }
        }
    }
}
