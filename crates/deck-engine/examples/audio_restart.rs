//! Muted CoreAudio regression: repeated output restart must keep exactly one deck clock.
use deck_engine::{AudioRouting, EngineCommand, EngineHandle};
use std::{
    thread,
    time::{Duration, Instant},
};
fn main() -> Result<(), String> {
    let engine = EngineHandle::new()?;
    let name = engine
        .devices()?
        .into_iter()
        .find(|d| {
            d.name.to_lowercase().contains("macbook") && d.name.to_lowercase().contains("speaker")
        })
        .ok_or("MacBook speaker output unavailable")?
        .name;
    engine.load_demo(0)?;
    engine.command(EngineCommand::SetMasterGain { gain: 0. })?;
    let mut attempts = Vec::new();
    for pass in 0..5 {
        let status = engine.start_audio(Some(name.clone()), Some(512), AudioRouting::default())?;
        thread::sleep(Duration::from_millis(100));
        if pass == 0 {
            engine.command(EngineCommand::Play { deck: 0 })?;
            thread::sleep(Duration::from_millis(50));
        }
        if engine
            .start_audio(
                Some("PulseDeck missing test output".into()),
                Some(512),
                AudioRouting::default(),
            )
            .is_ok()
        {
            return Err("Unavailable output was accepted".into());
        }
        if engine
            .start_audio(Some(name.clone()), Some(0), AudioRouting::default())
            .is_ok()
        {
            return Err("Zero-sized buffer was accepted".into());
        }
        let before = engine.snapshot().decks[0].position_seconds;
        let started = Instant::now();
        thread::sleep(Duration::from_millis(400));
        let elapsed = started.elapsed().as_secs_f64();
        let snapshot = engine.snapshot();
        let advance = snapshot.decks[0].position_seconds - before;
        if !snapshot.running
            || snapshot.output_channels != 2
            || snapshot.underruns != 0
            || !(0.75..1.25).contains(&(advance / elapsed))
        {
            return Err(format!("Restart {pass} failed: advance={advance}, wall={elapsed}, running={}, channels={}, errors={}",snapshot.running,snapshot.output_channels,snapshot.underruns));
        }
        attempts.push(serde_json::json!({"pass":pass,"wall_seconds":elapsed,"deck_advance_seconds":advance,"clock_ratio":advance/elapsed,"status":status}));
    }
    engine.stop_audio();
    thread::sleep(Duration::from_millis(30));
    let stopped = engine.snapshot().decks[0].position_seconds;
    thread::sleep(Duration::from_millis(250));
    let after = engine.snapshot();
    if after.running || (after.decks[0].position_seconds - stopped).abs() > 0.002 {
        return Err(
            "Stopped output still advances the deck: an old callback survived teardown".into(),
        );
    }
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"test":"macbook_stereo_restart_single_clock","master_muted":true,"restarts":attempts,"stopped_position":stopped,"position_after_stop":after.decks[0].position_seconds,"stream_errors":after.underruns})).map_err(|e|e.to_string())?);
    Ok(())
}
