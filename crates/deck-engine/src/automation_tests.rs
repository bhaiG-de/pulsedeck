use crate::{decode, dsp::Mixer, ControlTarget, EngineCommand};
#[test]
fn native_ramps_follow_render_frames_and_manual_moves_take_over() {
    let mut mixer = Mixer::new();
    mixer.sample_rate = 1000;
    mixer.command(EngineCommand::Ramp {
        target: ControlTarget::Crossfader,
        value: 1.0,
        seconds: 0.1,
        clock: None,
        beats: None,
        quantize: None,
    });
    for _ in 0..51 {
        mixer.frame();
    }
    assert!((mixer.crossfader.target - 0.5).abs() < 0.001);
    assert!(mixer.active_ramps()[0]);
    for _ in 0..50 {
        mixer.frame();
    }
    assert_eq!(mixer.crossfader.target, 1.0);
    assert!(!mixer.active_ramps()[0]);
    mixer.command(EngineCommand::Ramp {
        target: ControlTarget::Crossfader,
        value: -1.0,
        seconds: 1.0,
        clock: None,
        beats: None,
        quantize: None,
    });
    mixer.command(EngineCommand::SetCrossfader { position: 0.2 });
    for _ in 0..10 {
        mixer.frame();
    }
    assert_eq!(mixer.crossfader.target, 0.2);
    assert!(!mixer.active_ramps()[0]);
}
#[test]
fn beat_ramps_cancel_on_pause_seek_or_source_replacement() {
    for event in ["pause", "seek", "source"] {
        let mut mixer = Mixer::new();
        mixer.load(0, decode::demo_track(0));
        mixer.command(EngineCommand::Play { deck: 0 });
        mixer.command(EngineCommand::Ramp {
            target: ControlTarget::Crossfader,
            value: 1.0,
            seconds: 1.0,
            clock: Some(0),
            beats: Some(4.0),
            quantize: Some(4.0),
        });
        mixer.frame();
        assert!(mixer.active_ramps()[0]);
        match event {
            "pause" => mixer.command(EngineCommand::Pause { deck: 0 }),
            "seek" => mixer.command(EngineCommand::Seek {
                deck: 0,
                seconds: 2.0,
            }),
            _ => mixer.load_revisions[0] += 1,
        }
        mixer.frame();
        assert!(!mixer.active_ramps()[0], "{event}");
    }
}

#[test]
fn deck_ramps_guard_the_target_even_when_clock_uses_other_deck() {
    let mut mixer = Mixer::new();
    mixer.load(0, decode::demo_track(0));
    mixer.load(1, decode::demo_track(1));
    mixer.command(EngineCommand::Play { deck: 0 });
    let command = EngineCommand::Ramp {
        target: ControlTarget::EqLow { deck: 1 },
        value: 0.0,
        seconds: 1.0,
        clock: Some(0),
        beats: Some(4.0),
        quantize: None,
    };
    assert_eq!(command.deck(), Some(1));
    mixer.command(command);
    mixer.frame();
    assert!(mixer.active_ramps().iter().any(|r| *r));
    mixer.load_revisions[1] += 1;
    mixer.frame();
    assert!(!mixer.active_ramps().iter().any(|r| *r));
}
