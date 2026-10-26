use super::*;
use control_protocol::performance::*;
fn rhythm_mixer(sync: bool, anomaly: bool) -> Mixer {
    let mut m = mixer();
    for deck in 0..2 {
        let mut track = fixture(240.);
        let t = Arc::get_mut(&mut track).unwrap();
        let mut at = 0.;
        let beats: Vec<_> = (0..470)
            .map(|i| {
                let value = at;
                at += if deck == 0 {
                    0.5
                } else if anomaly && i >= 20 {
                    0.53
                } else {
                    0.5025 + if i % 2 == 0 { 0.001 } else { -0.001 }
                };
                value
            })
            .collect();
        t.analysis.neural_rhythm = Some(audio_analysis::NeuralRhythm {
            downbeats_seconds: beats.iter().step_by(4).copied().collect(),
            beats_seconds: beats,
            bpm: 120.,
            beat_confidence: 0.9,
            downbeat_confidence: 0.9,
            model: "synthetic slow pulse mismatch and timestamp jitter".into(),
            analysis_seconds: 0.,
            bar_evidence: vec![],
            beats_per_bar: Some(4),
        });
        t.analysis.beatgrid_source = Some("beat_this".into());
        t.analysis.bpm_source = Some("beat_this".into());
        t.analysis.waveform_bands = Some(audio_analysis::WaveformBands {
            low: vec![0.1; 240],
            mid: vec![0.05; 240],
            high: vec![0.025; 240],
        });
        m.load(deck, track);
        m.decks[deck].grid_downbeat = true; // Synthetic grids explicitly share known bar one.
    }
    m.command(EngineCommand::SetChannelFader { deck: 1, value: 0. });
    m.command(EngineCommand::Play { deck: 0 });
    if sync {
        m.command(EngineCommand::SetSyncRateLimits {
            deck: 1,
            limits: Some(crate::SyncRateLimits {
                min_rate: 0.97,
                max_rate: 1.03,
                max_slew_per_second: 0.02,
            }),
        });
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
    }
    m
}
fn rhythm_plan(m: &Mixer, maintain: bool) -> Plan {
    let mut p = artist_score(m);
    p.actions[0].id = "whole-overlap".into();
    p.actions[0].when = Timing::SourceEvent {
        deck: 0,
        raw_index: 4,
    };
    p.actions[0].commands = vec![EngineCommand::Play { deck: 1 }];
    p.actions[0].curves = [(0, 0., 42.), (1, 1., 16.)]
        .into_iter()
        .map(|(deck, value, seconds)| Curve {
            target: ControlTarget::ChannelFader { deck },
            points: vec![CurvePoint {
                at: Duration::Seconds { value: seconds },
                value,
            }],
        })
        .collect();
    p.actions[0].alignment = Some(Alignment {
        deck: 1,
        phase_tolerance_beats: 0.12,
        bar_beats: Some(4),
        maintain,
    });
    p
}
#[test]
fn artist_rhythm_native_bounded_sync_holds_full_overlap_with_jitter() {
    let mut m = rhythm_mixer(true, false);
    let s = snapshot(&m);
    let p = rhythm_plan(&m, true);
    let compiled = crate::performance::compile(201, &p, &s).unwrap();
    let forecast = compiled.validation.forecast.as_ref().unwrap();
    assert_eq!(forecast.rhythm_contracts.len(), 1);
    assert!(forecast.rhythm_contracts[0].seconds[1] > 44.);
    m.arm_batch(compiled.batch);
    let mut worst = 0_f64;
    let mut samples = 0;
    for _ in 0..45_000 {
        m.frame();
        if m.decks.iter().all(|d| d.playing) && m.scheduled.as_ref().unwrap().state.active() {
            let error = (m.decks[0].grid_position(m.decks[0].seconds())
                - m.decks[1].grid_position(m.decks[1].seconds())
                + 0.5)
                .rem_euclid(1.)
                - 0.5;
            worst = worst.max(error.abs());
            samples += 1;
            assert!(m.decks[1]
                .sync_rate_limits
                .unwrap()
                .contains(m.decks[1].actual_rate()));
        }
    }
    assert!(samples > 40_000);
    assert!(worst < 0.12, "{worst}");
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.state, BatchLifecycle::Completed, "{:?}", b.reason);
    assert!(b.receipts[..3].iter().all(|r| r.state == 2));
    assert!(snapshot(&m).decks[0].channel_fader_actual < 0.002);
}
#[test]
fn artist_rhythm_free_clocks_still_report_phase_excursion_below_two_percent() {
    let m = rhythm_mixer(false, false);
    let p = rhythm_plan(&m, false);
    let forecast = crate::performance::validate(&p, &snapshot(&m))
        .unwrap()
        .forecast
        .unwrap();
    assert!(
        forecast
            .findings
            .iter()
            .any(|f| f.code == "phase_exceeds_entry_tolerance"),
        "{:?}",
        forecast.findings
    );
    assert!(!forecast
        .findings
        .iter()
        .any(|f| f.code == "playing_pulse_difference"));
    assert!(forecast.rhythm_contracts.is_empty());
}
#[test]
fn artist_rhythm_unreachable_future_pulse_recovers_authored_baseline() {
    let mut m = rhythm_mixer(true, true);
    let p = rhythm_plan(&m, true);
    m.arm_batch(
        crate::performance::compile(202, &p, &snapshot(&m))
            .unwrap()
            .batch,
    );
    render(&mut m, 20.);
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.reason, BatchReason::RhythmLost);
    assert_eq!(b.state, BatchLifecycle::Recovered);
    assert!(b.receipts[..3].iter().any(|r| r
        .failure
        .is_some_and(|f| f.cause == CommandFailureCause::AlignmentFailed)));
    let actual = snapshot(&m);
    assert!((actual.decks[0].eq_mid - 0.5).abs() < 0.001);
    assert!(actual.decks[0].channel_fader_actual > 0.998);
    assert!(actual.decks[1].channel_fader_actual < 0.002);
}
#[test]
fn artist_rhythm_requires_preparation_and_manual_takeover_yields() {
    let m = rhythm_mixer(false, false);
    let p = rhythm_plan(&m, true);
    assert_eq!(
        crate::performance::validate(&p, &snapshot(&m))
            .unwrap_err()
            .code,
        "rhythm_not_prepared"
    );
    for manual in [
        EngineCommand::Seek {
            deck: 1,
            seconds: 20.,
        },
        EngineCommand::SetSync {
            deck: 1,
            enabled: false,
        },
    ] {
        let mut m = rhythm_mixer(true, false);
        let p = rhythm_plan(&m, true);
        m.arm_batch(
            crate::performance::compile(203, &p, &snapshot(&m))
                .unwrap()
                .batch,
        );
        render(&mut m, 5.);
        m.command(manual);
        render(&mut m, 0.1);
        assert_eq!(m.scheduled.as_ref().unwrap().state, BatchLifecycle::Yielded);
    }
}
#[test]
fn artist_rhythm_callback_rechecks_prepared_role_before_first_effect() {
    let mut m = rhythm_mixer(true, false);
    let p = rhythm_plan(&m, true);
    let batch = crate::performance::compile(204, &p, &snapshot(&m))
        .unwrap()
        .batch;
    // Emulate a native role change between compile and callback, without a manual revision.
    m.master_deck = 1;
    m.arm_batch(batch);
    render(&mut m, 3.);
    assert_eq!(
        m.scheduled.as_ref().unwrap().reason,
        BatchReason::RhythmLost
    );
    assert!(!m.decks[1].playing);
    assert_eq!(snapshot(&m).decks[1].channel_fader_actual, 0.);
}
#[test]
fn artist_generation_accepts_only_owned_sync_tempo_derivation() {
    let m = rhythm_mixer(true, false);
    let p = rhythm_plan(&m, true);
    let original = snapshot(&m);
    let mut updated = original.clone();
    updated.decks[1].tempo_percent += 0.1;
    assert!(crate::performance::validate(&p, &updated).is_ok());
    for field in 0..6 {
        let mut changed = updated.clone();
        match field {
            0 => changed.control_revision += 1,
            1 => changed.decks[1].sync_enabled = false,
            2 => changed.decks[1].sync_rate_limits = None,
            3 => {
                changed.decks[0].is_master = false;
                changed.decks[1].is_master = true
            }
            4 => changed.decks[0].tempo_percent += 0.1,
            _ => changed.load_revisions[1] += 1,
        }
        assert_eq!(
            crate::performance::validate(&p, &changed).unwrap_err().code,
            "generation_conflict",
            "field{field}"
        );
    }
}
#[test]
fn artist_source_event_resolves_exact_integer_anchor_and_rejects_expired_or_stale() {
    let mut m = mixer();
    let mut p = artist_score(&m);
    p.actions[0].when = Timing::SourceEvent {
        deck: 0,
        raw_index: 4,
    };
    let s = snapshot(&m);
    let exact = crate::performance::event_handle(&s, 0, 4).unwrap();
    let compiled = crate::performance::compile(205, &p, &s).unwrap();
    assert_eq!(
        compiled.batch.steps[0].as_ref().unwrap().at,
        exact.source_seconds
    );
    render(&mut m, 2.1);
    assert_eq!(
        crate::performance::validate(&p, &snapshot(&m))
            .unwrap_err()
            .code,
        "past_event"
    );
    m.command(EngineCommand::Seek {
        deck: 0,
        seconds: 0.,
    });
    assert_eq!(
        crate::performance::validate(&p, &snapshot(&m))
            .unwrap_err()
            .code,
        "generation_conflict"
    );
}
#[test]
fn artist_finish_uses_original_endpoint_and_reserves_actual_completion() {
    let mut m = mixer();
    let mut p = artist_score(&m);
    p.actions[0].when = Timing::Finish {
        before_endpoint_seconds: 0.,
    };
    p.actions[0].commands = vec![];
    p.actions[0].curves = vec![Curve {
        target: ControlTarget::ChannelFader { deck: 0 },
        points: vec![CurvePoint {
            at: Duration::Seconds { value: 2. },
            value: 0.,
        }],
    }];
    p.closing = Some(Closing {
        endpoint_seconds: 10.,
        carrier: 0,
        silence_from_seconds: None,
        session_id: Some("original-session".into()),
    });
    let first = crate::performance::validate(&p, &snapshot(&m)).unwrap();
    assert!((first.actions[0].end_seconds - 10.).abs() < 1e-9);
    assert!(first.actions[0].start_seconds < 8.);
    render(&mut m, 3.);
    let later = crate::performance::compile(206, &p, &snapshot(&m)).unwrap();
    assert!((later.validation.actions[0].end_seconds - 7.).abs() < 1e-9);
    m.arm_batch(later.batch);
    render(&mut m, 6.99);
    assert_eq!(
        m.scheduled.as_ref().unwrap().state,
        BatchLifecycle::Completed
    );
    assert!(snapshot(&m).decks[0].channel_fader_actual < 0.002);
    let mut no_session = p.clone();
    no_session.closing.as_mut().unwrap().session_id = None;
    assert_eq!(
        crate::performance::validate(&no_session, &snapshot(&m))
            .unwrap_err()
            .code,
        "finish_requires_session"
    );
}
#[test]
fn artist_rhythm_preparation_is_rechecked_before_first_group_exposure() {
    let mut m = rhythm_mixer(true, false);
    let mut p = rhythm_plan(&m, true);
    let mut disable = p.actions[0].clone();
    disable.id = "disable".into();
    disable.when = Timing::In {
        duration: Duration::Seconds { value: 0.1 },
    };
    disable.commands = vec![EngineCommand::SetSync {
        deck: 1,
        enabled: false,
    }];
    disable.curves.clear();
    disable.alignment = None;
    p.actions.push(disable);
    assert_eq!(
        crate::performance::validate(&p, &snapshot(&m))
            .unwrap_err()
            .code,
        "rhythm_control_conflict"
    );
    p.actions.pop();
    m.arm_batch(
        crate::performance::compile(207, &p, &snapshot(&m))
            .unwrap()
            .batch,
    );
    render(&mut m, 0.2);
    // Emulate a native-owned policy edit after arm, before the maintained group;
    // manual controls are separately covered and must still yield immediately.
    m.decks[1].sync_enabled = false;
    render(&mut m, 2.);
    assert!(!m.decks[1].playing);
    assert_eq!(snapshot(&m).decks[1].channel_fader_actual, 0.);
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.reason, BatchReason::RhythmLost);
    assert!(b.receipts[..3].iter().all(|r| r.state == 4));
}
#[test]
fn artist_rhythm_trial08_cached_grids_reproduce_drift_and_hold_with_native_sync() {
    let capture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/trial08-phase-overlap.json")).unwrap();
    let replay = |sync: bool| {
        let mut m = mixer();
        for deck in 0..2 {
            let source = &capture["sources"][deck];
            let mut track = fixture(240.);
            let t = Arc::get_mut(&mut track).unwrap();
            let beats: Vec<f64> = serde_json::from_value(source["beats_seconds"].clone()).unwrap();
            let bpm = source["bpm"].as_f64().unwrap();
            t.analysis.path = source["path"].as_str().unwrap().into();
            t.analysis.bpm = Some(bpm);
            t.analysis.bpm_source = Some("beat_this".into());
            t.analysis.beatgrid_source = Some("beat_this".into());
            t.analysis.neural_rhythm = Some(audio_analysis::NeuralRhythm {
                downbeats_seconds: beats.iter().step_by(4).copied().collect(),
                beats_seconds: beats,
                bpm,
                beat_confidence: 0.9,
                downbeat_confidence: 0.9,
                model: "trial08 cached events; timestamp replay only".into(),
                analysis_seconds: 0.,
                bar_evidence: vec![],
                beats_per_bar: Some(4),
            });
            t.analysis.waveform_bands = Some(audio_analysis::WaveformBands {
                low: vec![0.1; 240],
                mid: vec![0.1; 240],
                high: vec![0.1; 240],
            });
            m.load(deck, track);
            let rate = capture["rates"][deck].as_f64().unwrap();
            m.command(EngineCommand::SetTempo {
                deck,
                percent: (rate - 1.) * 100.,
            });
            m.command(EngineCommand::SetKeylock {
                deck,
                enabled: true,
            });
        }
        m.command(EngineCommand::SetChannelFader { deck: 0, value: 1. });
        m.command(EngineCommand::SetChannelFader { deck: 1, value: 0. });
        let entry = capture["sources"][0]["entry_seconds"].as_f64().unwrap();
        let rate = capture["rates"][0].as_f64().unwrap();
        m.command(EngineCommand::Seek {
            deck: 0,
            seconds: entry - 2. * rate,
        });
        m.command(EngineCommand::Play { deck: 0 });
        if sync {
            m.command(EngineCommand::SetSyncRateLimits {
                deck: 1,
                limits: Some(crate::SyncRateLimits {
                    min_rate: 0.97,
                    max_rate: 1.03,
                    max_slew_per_second: 0.02,
                }),
            });
            m.command(EngineCommand::SetSync {
                deck: 1,
                enabled: true,
            });
        }
        let mut p = rhythm_plan(&m, sync);
        p.actions[0].when = Timing::SourceEvent {
            deck: 0,
            raw_index: 320,
        };
        p.actions[0].curves[0].points[0].at = Duration::Seconds { value: 48. };
        let compiled = crate::performance::compile(208, &p, &snapshot(&m)).unwrap();
        let forecast = compiled.validation.forecast.as_ref().unwrap();
        if !sync {
            assert!(forecast
                .findings
                .iter()
                .any(|f| f.code == "phase_exceeds_entry_tolerance"));
            assert!(!forecast
                .findings
                .iter()
                .any(|f| f.code == "playing_pulse_difference"));
        }
        m.arm_batch(compiled.batch);
        let mut worst = 0_f64;
        for _ in 0..51_000 {
            m.frame();
            if m.decks.iter().all(|d| d.playing) && m.scheduled.as_ref().unwrap().state.active() {
                let error = (m.decks[0].grid_position(m.decks[0].seconds())
                    - m.decks[1].grid_position(m.decks[1].seconds())
                    + 0.5)
                    .rem_euclid(1.)
                    - 0.5;
                worst = worst.max(error.abs());
            }
        }
        let b = m.scheduled.as_ref().unwrap();
        assert_eq!(
            b.state,
            BatchLifecycle::Completed,
            "sync{sync}: {:?}",
            b.reason
        );
        assert!(b.receipts[..3].iter().all(|r| r.state == 2));
        worst
    };
    let free = replay(false);
    let held = replay(true);
    assert!(free > 0.4, "captured free-clock drift {free}");
    assert!(held < 0.12, "bounded native Sync phase {held}");
    eprintln!("TRIAL08_GRID_REPLAY free_phase_peak={free:.6} maintained_phase_peak={held:.6}");
}
#[test]
fn artist_rhythm_stopped_master_cannot_open_an_aligned_follower() {
    let mut m = rhythm_mixer(true, false);
    m.decks[1].playing = true;
    let mut p = rhythm_plan(&m, true);
    p.actions[0].when = Timing::In {
        duration: Duration::Seconds { value: 1. },
    };
    p.actions[0].commands.clear();
    p.actions[0].alignment.as_mut().unwrap().bar_beats = None;
    let mut stopped = p.actions[0].clone();
    stopped.id = "stop-master".into();
    stopped.when = Timing::In {
        duration: Duration::Seconds { value: 0.1 },
    };
    stopped.commands = vec![EngineCommand::Pause { deck: 0 }];
    stopped.curves.clear();
    stopped.alignment = None;
    p.actions.push(stopped);
    assert_eq!(
        crate::performance::validate(&p, &snapshot(&m))
            .unwrap_err()
            .code,
        "rhythm_not_prepared"
    );
    p.actions.pop();
    m.arm_batch(
        crate::performance::compile(209, &p, &snapshot(&m))
            .unwrap()
            .batch,
    );
    // Native-owned stop after arm, without manual revision. Freeze the master
    // at the upcoming matching phase, so phase alone cannot catch the failure.
    m.decks[0].playing = false;
    m.decks[0].position = 1000.;
    render(&mut m, 1.01);
    assert!((m.decks[1].grid_position(m.decks[1].seconds()) - 2.).abs() < 0.03);
    assert_eq!(snapshot(&m).decks[1].channel_fader_actual, 0.);
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.reason, BatchReason::RhythmLost);
    assert!(b.receipts[..2].iter().all(|r| r.state == 4));
}
