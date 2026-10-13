//! Silent CoreAudio integration check. Run on a Mac with an output device.
use deck_engine::{AudioRouting, EngineCommand, EngineHandle};
use std::{thread, time::Duration};
fn main() -> Result<(), String> {
    let engine = EngineHandle::new()?;
    let devices = engine.devices()?;
    engine.load_demo(0)?;
    engine.load_demo(1)?;
    engine.command(EngineCommand::SetMasterGain { gain: 0.0 })?;
    let status = engine.start_audio(None, Some(256), AudioRouting::default())?;
    // Loaded decks are paused while the master smoothly reaches zero. No audible
    // material is emitted before the source decks start.
    thread::sleep(Duration::from_millis(150));
    let path = std::env::temp_dir().join(format!("deck-native-smoke-{}.wav", std::process::id()));
    let _ = std::fs::remove_file(&path);
    engine.start_recording(&path)?;
    engine.command(EngineCommand::Play { deck: 0 })?;
    engine.command(EngineCommand::Play { deck: 1 })?;
    thread::sleep(Duration::from_millis(450));
    let before_stop = engine.snapshot();
    if !before_stop.running
        || before_stop.rendered_frames == 0
        || before_stop.decks.iter().any(|d| d.position_seconds < 0.25)
        || before_stop.underruns != 0
    {
        return Err(format!(
            "Native render did not advance cleanly: {before_stop:?}"
        ));
    }
    let recording = engine.stop_recording()?;
    if recording.frames_written == 0 || recording.dropped_frames != 0 || recording.error.is_some() {
        return Err(format!("Recording failed: {recording:?}"));
    }
    let analysis = deck_engine::analyze_file(&path)?;
    if analysis.waveform.iter().any(|x| *x > 1e-6) {
        return Err("Silent smoke test unexpectedly emitted nonzero audio".into());
    }
    engine.stop_audio();
    thread::sleep(Duration::from_millis(30));
    let stopped = engine.snapshot();
    if (stopped.decks[0].position_seconds - before_stop.decks[0].position_seconds).abs() > 0.10 {
        return Err("Deck state changed unexpectedly at stop".into());
    }
    engine.start_audio(None, Some(256), AudioRouting::default())?;
    thread::sleep(Duration::from_millis(200));
    let restarted = engine.snapshot();
    engine.stop_audio();
    if restarted.decks[0].position_seconds <= stopped.decks[0].position_seconds + 0.1 {
        return Err("Restart did not preserve and resume the deck".into());
    }
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"test":"silent_native_coreaudio_smoke","device":status,"devices":devices,"frames_before_stop":before_stop.rendered_frames,"deck_positions_before_stop":before_stop.decks.iter().map(|d|d.position_seconds).collect::<Vec<_>>(),"deck_positions_after_restart":restarted.decks.iter().map(|d|d.position_seconds).collect::<Vec<_>>(),"deck_peaks":before_stop.decks.iter().map(|d|d.peak).collect::<Vec<_>>(),"stream_errors":restarted.underruns,"recording":recording,"captured_duration_seconds":analysis.duration_seconds,"master_was_silent":true,"flx4_tested":false})).map_err(|e|e.to_string())?);
    std::fs::remove_file(path).map_err(|e| e.to_string())?;
    Ok(())
}
