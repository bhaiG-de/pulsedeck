use super::*;
use control_protocol::{forecast::*, performance::*};

fn context() -> (crate::EngineSnapshot, Plan) {
    let m = mixer();
    let mut s = snapshot(&m);
    for d in &mut s.decks {
        d.playing = true;
        d.position_seconds = 10.;
        d.playback_rate = 1.;
        d.sync_enabled = false;
        d.normalization_enabled = false;
        d.normalization_gain_db = 0.;
        d.keylock = true;
        d.pitch_semitones = 0.;
        d.channel_fader = 1.;
        d.eq_low = 1.;
        d.eq_mid = 1.;
        d.eq_high = 1.;
        d.gain = 1.;
        d.filter = 0.;
        d.echo = 0.;
        d.reverb = 0.;
        d.track.as_mut().unwrap().waveform_bands = Some(audio_analysis::WaveformBands {
            low: vec![0.1; 240],
            mid: vec![0.05; 240],
            high: vec![0.025; 240],
        });
    }
    let mut p = artist_score(&m);
    p.generation = crate::performance::generation(&s);
    p.actions[0].commands = vec![EngineCommand::SetEq {
        deck: 0,
        low: 1.,
        mid: 1.,
        high: 1.,
    }];
    (s, p)
}
fn forecast(s: &crate::EngineSnapshot, p: &Plan) -> TransitionForecast {
    crate::performance::validate(p, s)
        .unwrap()
        .forecast
        .unwrap()
}
fn action(id: &str, seconds: f64, commands: Vec<EngineCommand>) -> Action {
    Action {
        id: id.into(),
        when: Timing::In {
            duration: Duration::Seconds { value: seconds },
        },
        after: vec![],
        commands,
        curves: vec![],
        alignment: None,
    }
}
fn curve(target: ControlTarget, points: &[(f64, f32)]) -> Curve {
    Curve {
        target,
        points: points
            .iter()
            .map(|(seconds, value)| CurvePoint {
                at: Duration::Seconds { value: *seconds },
                value: *value,
            })
            .collect(),
    }
}
#[test]
fn forecast_eof_is_a_boundary_not_a_slow_played_pulse() {
    let (mut s, p) = context();
    s.decks[0].position_seconds = 236.5;
    let f = forecast(&s, &p);
    assert!(f.windows.iter().any(|w| (w.seconds[1] - 3.5).abs() < 1e-6));
    for w in &f.windows {
        if let Some(bpm) = w.decks[0].playing_bpm {
            assert!((bpm - 120.).abs() < 0.001, "{w:?}");
        }
    }
    assert!(!f
        .findings
        .iter()
        .any(|i| i.code == "playing_pulse_difference"));
}
#[test]
fn forecast_normalization_change_is_not_source_structure() {
    let (mut s, mut p) = context();
    let measurements = serde_json::json!({"version":1,"sample_rate":1000,"frames":240000,"audible_start_frame":0,"audible_end_frame":240000,"integrated_lufs":-8.,"true_peak_dbtp":0.,"normalization_gain_db":-20.,"threshold_dbfs":-60.,"error":null});
    s.decks[0].normalization_gain_db = -20.;
    s.decks[0].track.as_mut().unwrap().audio_measurements =
        Some(serde_json::from_value(measurements).unwrap());
    p.actions = vec![action(
        "normalize",
        4.,
        vec![EngineCommand::SetNormalization {
            deck: 0,
            enabled: true,
        }],
    )];
    let f = forecast(&s, &p);
    assert!(!f
        .findings
        .iter()
        .any(|i| i.code == "source_low_withdrawal" || i.code == "source_low_arrival"));
    assert!(f
        .windows
        .iter()
        .any(|w| w.decks[0].source_rms.unwrap()[0] < 0.011));
}
#[test]
fn forecast_bounded_launch_withholds_exact_phase_but_eq_does_not_erase_clock() {
    let (mut s, mut p) = context();
    s.decks[1].playing = false;
    p.actions.push(Action {
        id: "launch".into(),
        when: Timing::After {
            action: p.actions[0].id.clone(),
            offset: None,
        },
        after: vec![],
        commands: vec![EngineCommand::Play { deck: 1 }],
        curves: vec![],
        alignment: None,
    });
    let f = forecast(&s, &p);
    assert!(f
        .windows
        .iter()
        .filter(|w| w.seconds[0] > 2.)
        .all(|w| w.decks[1].source_timing_uncertainty_seconds > 0.
            && w.fractional_phase_error_beats.is_none()
            && w.decks[1].playing_bpm.is_none()
            && w.decks[1].contribution_cue.is_none()
            && w.decks[1].low_eq_open_cue.is_none()));
    s.decks[1].playing = true;
    p.actions[1].commands = vec![EngineCommand::SetEq {
        deck: 1,
        low: 0.5,
        mid: 1.,
        high: 1.,
    }];
    let f = forecast(&s, &p);
    let late = f.windows.iter().find(|w| w.seconds[0] > 3.).unwrap();
    assert_eq!(late.decks[1].coverage, ForecastCoverage::Available);
    assert!(late.fractional_phase_error_beats.is_some());
}
#[test]
fn forecast_planned_loop_sync_filter_fx_are_explicitly_unproven() {
    let (s, base) = context();
    for (command, reason) in [
        (
            EngineCommand::SetLoop {
                deck: 1,
                beats: Some(4.),
            },
            "transport_or_rate_edit_unmodelled",
        ),
        (
            EngineCommand::SetSync {
                deck: 1,
                enabled: true,
            },
            "transport_or_rate_edit_unmodelled",
        ),
        (
            EngineCommand::SetFilter {
                deck: 1,
                amount: -0.5,
            },
            "filter_spectral_response_unmodelled",
        ),
        (
            EngineCommand::SetFx {
                deck: 1,
                echo: 0.5,
                reverb: 0.,
            },
            "fx_response_unmodelled",
        ),
    ] {
        let mut p = base.clone();
        p.actions = vec![action("creative", 1., vec![command])];
        let f = forecast(&s, &p);
        let d = &f.windows.iter().find(|w| w.seconds[0] > 2.).unwrap().decks[1];
        assert!(d.uncertainty.iter().any(|r| r == reason), "{d:?}");
        assert!(d.contribution_cue.is_none());
    }
}
#[test]
fn forecast_serial_curves_keep_their_inherited_start_and_known_tail() {
    let (s, mut p) = context();
    p.actions = vec![action("shape", 1., vec![])];
    p.actions[0].curves = vec![curve(
        ControlTarget::EqLow { deck: 1 },
        &[(4., 0.5), (8., 1.)],
    )];
    let f = forecast(&s, &p);
    let point = f
        .windows
        .iter()
        .find(|w| (w.seconds[0] - 5.).abs() < 1e-6)
        .unwrap();
    assert!((point.decks[1].eq[0][0] - 0.5).abs() < 0.001);
    let tail = f.windows.iter().find(|w| w.seconds[0] > 10.).unwrap();
    assert_eq!(tail.decks[1].coverage, ForecastCoverage::Available);
    assert!((tail.decks[1].eq[0][0] - 1.).abs() < 0.001);
}
#[test]
fn forecast_bounded_output_retains_late_closing_and_explicit_omissions() {
    let (s, mut p) = context();
    p.actions = (0..63)
        .map(|i| {
            action(
                &format!("{i:03}{}", "x".repeat(125)),
                1. + i as f64 * 0.2,
                vec![EngineCommand::SetEq {
                    deck: 0,
                    low: 1.,
                    mid: 1.,
                    high: 1.,
                }],
            )
        })
        .collect();
    p.actions.push(action(
        "final",
        170.,
        vec![EngineCommand::Ramp {
            target: ControlTarget::ChannelFader { deck: 0 },
            value: 0.,
            seconds: 9.,
            clock: None,
            beats: None,
            quantize: None,
        }],
    ));
    p.closing = Some(Closing {
        carrier: 0,
        endpoint_seconds: 180.,
        silence_from_seconds: Some(179.),
        session_id: None,
    });
    let f = forecast(&s, &p);
    assert!(f.windows.len() <= 24);
    assert!(f
        .windows
        .iter()
        .any(|w| w.seconds[0] >= 170. && w.actions.iter().any(|id| id == "final")));
    assert!(!f.omitted_ranges.is_empty());
    assert!(serde_json::to_vec(&f).unwrap().len() < 100_000);
    assert!(f.windows.last().unwrap().seconds[1] >= 179.9);
}
#[test]
fn forecast_trial06_cached_bass_handoff_exposes_withheld_arrival() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/trial06-bass-handoff.json")).unwrap();
    let (mut s, mut p) = context();
    for (deck, value) in fixture["sources"].as_array().unwrap().iter().enumerate() {
        let t = s.decks[deck].track.as_mut().unwrap();
        t.path = value["path"].as_str().unwrap().into();
        t.duration_seconds = value["duration_seconds"].as_f64().unwrap();
        t.source_fingerprint = Some(value["source_fingerprint"].as_str().unwrap().into());
        t.key = Some(value["key"].as_str().unwrap().into());
        t.camelot = Some(value["camelot"].as_str().unwrap().into());
        let n = value["raw_bins"].as_u64().unwrap() as usize;
        let start = value["start_index"].as_u64().unwrap() as usize;
        let read = |name: &str| {
            let mut bins = vec![f32::NAN; n];
            for (i, v) in value["bands"][name].as_array().unwrap().iter().enumerate() {
                bins[start + i] = v.as_f64().unwrap() as f32;
            }
            bins
        };
        t.waveform_bands = Some(audio_analysis::WaveformBands {
            low: read("low"),
            mid: read("mid"),
            high: read("high"),
        });
        s.decks[deck].normalization_enabled = true;
        s.decks[deck].normalization_gain_db = value["audio_measurements"]["normalization_gain_db"]
            .as_f64()
            .unwrap() as f32;
    }
    s.decks[0].position_seconds = 29.58;
    s.decks[0].playing = false;
    s.decks[0].channel_fader = 0.;
    s.decks[0].eq_low = 0.;
    s.decks[0].playback_rate = 0.9508579254;
    s.decks[1].position_seconds = 199.1002502441;
    s.decks[1].playback_rate = 0.9722200528;
    p.generation = crate::performance::generation(&s);
    p.actions = vec![action(
        "exchange",
        0.5,
        vec![EngineCommand::Play { deck: 0 }],
    )];
    p.actions[0].curves = vec![
        curve(ControlTarget::ChannelFader { deck: 0 }, &[(8., 1.)]),
        curve(ControlTarget::ChannelFader { deck: 1 }, &[(8., 0.)]),
    ];
    p.actions.push(Action {
        id: "late-bass".into(),
        when: Timing::After {
            action: "exchange".into(),
            offset: None,
        },
        after: vec![],
        commands: vec![EngineCommand::Pause { deck: 1 }],
        curves: vec![curve(ControlTarget::EqLow { deck: 0 }, &[(8., 1.)])],
        alignment: None,
    });
    let f = forecast(&s, &p);
    let withheld: Vec<_> = f
        .findings
        .iter()
        .filter(|i| i.code == "available_source_low_withheld" && i.deck == Some(0))
        .collect();
    assert!(!withheld.is_empty(), "{:?}", f.findings);
    assert!(withheld
        .iter()
        .any(|i| i.seconds[0] < 8.5 && i.seconds[1] > 4.));
    assert!(f
        .findings
        .iter()
        .any(|i| i.code == "source_low_withdrawal" && i.deck == Some(1)));
    assert!(f
        .windows
        .iter()
        .any(|w| w.decks[0].source_rms.is_some_and(|v| v[0] > 0.09) && w.decks[0].eq[1][0] == 0.));
    assert_eq!(f.sources[0].camelot.as_deref(), Some("10A"));
    assert_eq!(f.sources[1].camelot.as_deref(), Some("4A"));
    // Artist changes only the bass ownership course; source evidence is unchanged.
    p.actions[1].curves.clear();
    p.actions[1].commands.push(EngineCommand::SetEq {
        deck: 0,
        low: 1.,
        mid: 1.,
        high: 1.,
    });
    p.actions[0]
        .curves
        .push(curve(ControlTarget::EqLow { deck: 0 }, &[(4., 1.)]));
    let earlier = forecast(&s, &p);
    assert!(!earlier
        .findings
        .iter()
        .any(|i| i.code == "available_source_low_withheld" && i.seconds[0] >= 4.5));
}
#[test]
fn forecast_ended_source_is_known_zero_but_jog_hold_is_unproven() {
    let (mut s, p) = context();
    s.decks[0].position_seconds = 239.;
    let f = forecast(&s, &p);
    let ended = &f.windows.iter().find(|w| w.seconds[0] > 2.).unwrap().decks[0];
    assert_eq!(ended.contribution_cue, Some([0.; 3]));
    assert!(ended.source_rms.is_none());
    s.decks[0].jog_touch = true;
    let f = forecast(&s, &p);
    assert!(f
        .windows
        .iter()
        .all(|w| w.decks[0].contribution_cue.is_none() && w.decks[0].playing_bpm.is_none()));
}
#[test]
fn forecast_eq_withholding_requires_available_own_gain_and_routing() {
    let (mut s, p) = context();
    s.decks[0].eq_low = 0.;
    s.decks[1].channel_fader = 0.;
    let f = forecast(&s, &p);
    assert!(f
        .findings
        .iter()
        .any(|f| f.code == "available_source_low_withheld"));
    for case in 0..3 {
        let mut state = s.clone();
        match case {
            0 => state.decks[0].gain = 0.,
            1 => state.crossfader = 1.,
            _ => state.decks[0].filter = -0.5,
        }
        assert!(
            !forecast(&state, &p)
                .findings
                .iter()
                .any(|f| f.code == "available_source_low_withheld"),
            "case {case}"
        );
    }
}
#[test]
fn forecast_repeated_long_action_ids_have_a_compact_serialized_bound() {
    let (s, mut p) = context();
    p.actions.clear();
    for i in 0..22 {
        p.actions.push(action(
            &format!("{i:03}{}", "x".repeat(125)),
            1.,
            vec![EngineCommand::Ramp {
                target: ControlTarget::from_index(i),
                value: 0.5,
                seconds: 60.,
                clock: None,
                beats: None,
                quantize: None,
            }],
        ));
    }
    for i in 0..42 {
        p.actions.push(action(
            &format!("s{i:02}{}", "y".repeat(125)),
            2. + i as f64 * 0.2,
            vec![EngineCommand::Ramp {
                target: ControlTarget::from_index(22),
                value: 0.5,
                seconds: 0.1,
                clock: None,
                beats: None,
                quantize: None,
            }],
        ));
    }
    let f = forecast(&s, &p);
    assert!(f.windows.iter().any(|w| w.actions_omitted > 0));
    assert!(f.windows.iter().all(|w| w.actions.len() <= 4));
    let bytes = serde_json::to_vec(&f).unwrap().len();
    eprintln!("forecast repeated-ID serialized bytes: {bytes}");
    assert!(bytes < 60_000, "{bytes} bytes");
}
#[test]
fn forecast_trial06_grid_anomaly_is_separate_from_fractional_clock_drift() {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/trial06-bass-handoff.json")).unwrap();
    let captured = &value["grid_anomaly"];
    let (mut s, mut p) = context();
    let beats: Vec<f64> = serde_json::from_value(captured["beats_seconds"].clone()).unwrap();
    let bpm = captured["bpm"].as_f64().unwrap();
    s.decks[0].beatgrid_uniform = false;
    s.decks[0].beatgrid_offset_seconds = beats[0];
    s.decks[0].bpm = Some(bpm);
    s.decks[0].position_seconds = 263.;
    let t = s.decks[0].track.as_mut().unwrap();
    t.duration_seconds = 310.;
    t.waveform_bands = Some(audio_analysis::WaveformBands {
        low: vec![0.1; 310],
        mid: vec![0.1; 310],
        high: vec![0.1; 310],
    });
    t.neural_rhythm = Some(audio_analysis::NeuralRhythm {
        beats_seconds: beats,
        downbeats_seconds: vec![],
        bpm,
        beat_confidence: 0.8,
        downbeat_confidence: 0.8,
        model: "captured trial06".into(),
        analysis_seconds: 0.,
        bar_evidence: vec![],
        beats_per_bar: Some(4),
    });
    p.actions[0].commands = vec![EngineCommand::SetEq {
        deck: 1,
        low: 1.,
        mid: 1.,
        high: 1.,
    }];
    let f = forecast(&s, &p);
    let anomalies: Vec<_> = f
        .windows
        .iter()
        .filter(|w| {
            w.decks[0]
                .uncertainty
                .iter()
                .any(|r| r == "effective_grid_interval_anomaly")
        })
        .collect();
    assert!(!anomalies.is_empty());
    assert!(anomalies
        .iter()
        .all(|w| w.fractional_phase_error_beats.is_none()));
    assert!(f
        .findings
        .iter()
        .any(|i| i.code == "grid_interval_anomaly" && i.evidence.contains("0.3")));
    assert!(f
        .windows
        .iter()
        .any(|w| w.seconds[1] < 3. && w.fractional_phase_error_beats.is_some()));
}

#[test]
fn forecast_completion_relative_mixer_bounds_keep_nominal_overlap_cues() {
    let mut m = mixer();
    m.sample_rate = 48_000;
    let mut s = snapshot(&m);
    for deck in &mut s.decks {
        deck.track.as_mut().unwrap().waveform_bands = Some(audio_analysis::WaveformBands {
            low: vec![0.1; 240],
            mid: vec![0.05; 240],
            high: vec![0.025; 240],
        });
    }
    let mut p = artist_score(&m);
    let mut launch = action("launch", 2., vec![EngineCommand::Play { deck: 1 }]);
    launch.when = Timing::Event {
        event: crate::performance::event_handle(&s, 0, 4).unwrap(),
    };
    p.actions = vec![
        launch,
        Action {
            id: "blend".into(),
            when: Timing::After {
                action: "launch".into(),
                offset: None,
            },
            after: vec![],
            commands: vec![],
            curves: vec![
                curve(ControlTarget::ChannelFader { deck: 0 }, &[(16., 0.)]),
                curve(ControlTarget::ChannelFader { deck: 1 }, &[(16., 1.)]),
            ],
            alignment: None,
        },
    ];
    let f = forecast(&s, &p);
    let overlap: Vec<_> = f
        .windows
        .iter()
        .filter(|w| w.seconds[0] > 2.1 && w.seconds[1] < 18.)
        .collect();
    assert!(!overlap.is_empty());
    for window in overlap {
        assert!(window.fractional_phase_error_beats.is_some());
        assert!(window
            .band_dominance
            .iter()
            .all(|band| *band != BandDominance::Unknown));
        for deck in &window.decks {
            assert_eq!(deck.coverage, ForecastCoverage::Partial);
            assert_eq!(deck.uncertainty, ["control_action_start_is_bounded"]);
            assert!((deck.timing_uncertainty_seconds - 2. / 48_000.).abs() < 1e-9);
            assert_eq!(deck.source_timing_uncertainty_seconds, 0.);
            assert!(deck.playing_bpm.is_some());
            assert!(deck
                .contribution_cue
                .is_some_and(|cue| cue.iter().all(|v| v.is_finite() && *v > 0.)));
            assert!(deck
                .low_eq_open_cue
                .is_some_and(|cue| cue.is_finite() && cue > 0.));
        }
    }
    // The eligibility change must not admit missing source evidence.
    let mut missing = s.clone();
    missing.decks[1].track.as_mut().unwrap().waveform_bands = None;
    let unknown = forecast(&missing, &p);
    assert!(unknown
        .windows
        .iter()
        .filter(|w| w.seconds[0] > 2.1 && w.seconds[1] < 18.)
        .all(|w| w.decks[1].contribution_cue.is_none() && w.decks[1].low_eq_open_cue.is_none()));

    // Exercise the actual callback's completion-relative start and ramp course.
    let compiled = crate::performance::compile(501, &p, &s).unwrap();
    m.arm_batch(compiled.batch);
    render(&mut m, 6.);
    let actual = snapshot(&m);
    for (deck, expected) in [0.75, 0.25].into_iter().enumerate() {
        assert!((actual.decks[deck].channel_fader_actual - expected).abs() < 0.002);
    }
    render(&mut m, 12.1);
    let native = m.scheduled.as_ref().unwrap();
    assert_eq!(native.state, BatchLifecycle::Completed);
    assert!(native.receipts[..3]
        .iter()
        .all(|receipt| receipt.state == 2));
    assert!((snapshot(&m).decks[1].position_seconds - 16.1).abs() < 0.003);
}
