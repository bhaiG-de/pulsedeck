use crate::{
    decode::TrackData, dsp::Mixer, scheduled::*, snapshot::SnapshotAtoms, ControlTarget,
    EngineCommand,
};
use std::sync::{Arc, OnceLock};
fn fixture(seconds: f64) -> Arc<TrackData> {
    static ANALYSIS: OnceLock<crate::TrackAnalysis> = OnceLock::new();
    let mut a = ANALYSIS
        .get_or_init(|| crate::decode::demo_track(0).analysis.clone())
        .clone();
    a.duration_seconds = seconds;
    a.sample_rate = 1000;
    a.bpm = Some(120.);
    a.bpm_source = Some("manual".into());
    a.beatgrid_source = Some("manual".into());
    Arc::new(TrackData {
        analysis_pending: false,
        samples: vec![[0.1; 2]; (seconds * 1000.) as usize],
        sample_rate: 1000,
        analysis: a,
    })
}
fn mixer() -> Mixer {
    let mut m = Mixer::new();
    m.sample_rate = 1000;
    m.load(0, fixture(240.));
    m.load(1, fixture(240.));
    m.decks[0].grid_uniform = true;
    m.decks[1].grid_uniform = true;
    m.command(EngineCommand::SetChannelFader { deck: 1, value: 0. });
    m.command(EngineCommand::SetEq {
        deck: 1,
        low: 0.3,
        mid: 1.,
        high: 1.,
    });
    m.command(EngineCommand::Play { deck: 0 });
    m
}
fn snapshot(m: &Mixer) -> crate::EngineSnapshot {
    let atoms = SnapshotAtoms::new();
    atoms
        .running
        .store(true, std::sync::atomic::Ordering::Relaxed);
    atoms.publish(m, 2, 64, [0.; 2], [0.; 2]);
    atoms.read(
        m.decks
            .each_ref()
            .map(|d| d.track.as_ref().map(|t| t.analysis.clone())),
    )
}
fn plan(m: &Mixer) -> (BatchSchedule, Vec<ScheduledCommand>) {
    let s = snapshot(m);
    let schedule = BatchSchedule {
        operation_id: "fixture".into(),
        open: false,
        expiry_seconds: None,
        completion_carrier: None,
        clock: 0,
        expected_load_revisions: s.load_revisions,
        expected_position_revisions: s.decks.each_ref().map(|d| d.position_revision),
        expected_grid_revisions: s.decks.each_ref().map(|d| d.grid_revision),
        expected_engine_epoch: 0,
        expected_control_revision: s.control_revision,
        arm_before_event: 2,
        completion_event: 24,
        pause_outgoing: true,
        intent: serde_json::json!({"protected_payoff":"fixture","baseline_frames":0}),
        recovery: BatchRecovery {
            carrier: 1,
            seconds: 0.2,
            controls: vec![
                ramp(1, ControlTarget::ChannelFader { deck: 1 }, 1., None, 0.2),
                ramp(1, ControlTarget::EqLow { deck: 1 }, 1., None, 0.2),
                ramp(0, ControlTarget::ChannelFader { deck: 0 }, 0., None, 0.2),
            ],
        },
    };
    let mut steps = vec![ScheduledCommand {
        at_event: 4,
        deadline_event: 4,
        command: EngineCommand::PlayQuantized {
            deck: 1,
            clock: 0,
            clock_revision: s.load_revisions[0],
            at_seconds: None,
            quantize: 1.,
        },
        require: Some(BatchPrerequisite {
            deck: 1,
            playing: Some(false),
            ready: true,
            ..Default::default()
        }),
    }];
    for c in [
        ramp(0, ControlTarget::ChannelFader { deck: 0 }, 0., Some(8.), 1.),
        ramp(1, ControlTarget::ChannelFader { deck: 1 }, 1., Some(8.), 1.),
        ramp(1, ControlTarget::EqLow { deck: 1 }, 1., Some(8.), 1.),
    ] {
        steps.push(ScheduledCommand {
            at_event: 8,
            deadline_event: 8,
            command: c,
            require: Some(BatchPrerequisite {
                deck: 1,
                playing: Some(true),
                ready: true,
                advanced_from_seconds: Some(0.),
                min_advanced_beats: 3.,
                phase_tolerance_beats: Some(0.01),
                ..Default::default()
            }),
        });
    }
    (schedule, steps)
}
fn ramp(
    _deck: usize,
    target: ControlTarget,
    value: f32,
    beats: Option<f64>,
    seconds: f64,
) -> EngineCommand {
    EngineCommand::Ramp {
        target,
        value,
        seconds,
        clock: beats.map(|_| 0),
        beats,
        quantize: None,
    }
}
fn render(m: &mut Mixer, seconds: f64) {
    for _ in 0..(seconds * m.sample_rate as f64) as usize {
        m.frame();
    }
}
#[test]
fn queue_completes_without_model_for_120_seconds_and_groups_controls() {
    let mut m = mixer();
    let (schedule, steps) = plan(&m);
    let batch = compile(1, &schedule, steps, &snapshot(&m)).unwrap();
    assert!(m.arm_batch(batch).is_none());
    render(&mut m, 120.);
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.state, BatchLifecycle::Completed);
    assert_eq!(b.reason, BatchReason::None);
    assert!(m.decks[1].playing);
    assert!(!m.decks[0].playing);
    assert_eq!(m.decks[1].eq[0].target, 1.);
    assert_eq!(b.receipts[1].frame, b.receipts[2].frame);
    assert_eq!(b.receipts[2].frame, b.receipts[3].frame);
    assert!((b.receipts[0].clock_seconds - 2.).abs() < 0.002);
    assert!(b.receipts.iter().take(4).all(|e| e.state == 2));
}
#[test]
fn invalid_last_step_or_guard_never_arms_partial_controls() {
    let mut m = mixer();
    let (schedule, mut steps) = plan(&m);
    steps.push(ScheduledCommand {
        at_event: 9,
        deadline_event: 9,
        command: EngineCommand::SetTempo {
            deck: 1,
            percent: 4.,
        },
        require: None,
    });
    assert!(compile(1, &schedule, steps, &snapshot(&m)).is_err());
    render(&mut m, 5.);
    assert!(!m.decks[1].playing);
    assert_eq!(m.decks[1].channel_fader.target, 0.);
    let (mut schedule, steps) = plan(&m);
    schedule.expected_load_revisions[0] += 1;
    assert!(compile(1, &schedule, steps, &snapshot(&m)).is_err());
    assert!(m.scheduled.is_none());
}
#[test]
fn outgoing_eof_recovers_carrier_using_renderer_clock() {
    let mut m = mixer();
    let (schedule, steps) = plan(&m);
    m.arm_batch(compile(1, &schedule, steps, &snapshot(&m)).unwrap());
    render(&mut m, 5.);
    let revision = m.decks[0].position_revision;
    // Shorten the decoded RAM after arming to exercise the real render EOF
    // path while the carrier is playing and the handover ramps are unfinished.
    let track = m.decks[0].track.as_ref().unwrap();
    let ended = Arc::new(TrackData {
        analysis_pending: false,
        samples: track.samples[..5002].to_vec(),
        sample_rate: track.sample_rate,
        analysis: track.analysis.clone(),
    });
    m.decks[0].track = Some(ended);
    render(&mut m, 0.5);
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.state, BatchLifecycle::Recovered);
    assert_eq!(b.reason, BatchReason::ClockStopped);
    assert_eq!(m.decks[0].position_revision, revision);
    assert!(!m.decks[0].playing);
    assert_eq!(m.decks[0].position, 5002.);
    assert!(m.decks[1].playing);
    assert_eq!(m.decks[1].channel_fader.target, 1.);
    assert_eq!(m.decks[1].eq[0].target, 1.);
    assert_eq!(m.decks[0].channel_fader.target, 0.);
    assert!(!m.active_ramps().iter().any(|v| *v));
}
#[test]
fn manual_changes_win_and_ownership_cancel_does_not_restore_cached_state() {
    for change in [
        EngineCommand::SetEq {
            deck: 1,
            low: 0.7,
            mid: 0.9,
            high: 0.8,
        },
        EngineCommand::Seek {
            deck: 1,
            seconds: 20.,
        },
        EngineCommand::SetTempo {
            deck: 1,
            percent: 3.,
        },
        EngineCommand::SetTempo {
            deck: 0,
            percent: 3.,
        },
        EngineCommand::SetMaster { deck: 1 },
    ] {
        let mut m = mixer();
        let (schedule, steps) = plan(&m);
        m.arm_batch(compile(1, &schedule, steps, &snapshot(&m)).unwrap());
        render(&mut m, 5.);
        m.command(change.clone());
        render(&mut m, 1.);
        assert_eq!(m.scheduled.as_ref().unwrap().state, BatchLifecycle::Yielded);
        assert_eq!(
            m.scheduled.as_ref().unwrap().reason,
            BatchReason::ManualTakeover
        );
        if let EngineCommand::SetEq { low, .. } = change {
            assert_eq!(m.decks[1].eq[0].target, low);
        }
    }
}
#[test]
fn native_source_and_grid_invalidation_yield_without_restoration() {
    for grid in [false, true] {
        let mut m = mixer();
        let (schedule, steps) = plan(&m);
        m.arm_batch(compile(1, &schedule, steps, &snapshot(&m)).unwrap());
        render(&mut m, 5.);
        if grid {
            m.grid_revisions[1] += 1;
        } else {
            m.load_revisions[1] += 1;
        }
        render(&mut m, 0.5);
        let b = m.scheduled.as_ref().unwrap();
        assert_eq!(b.state, BatchLifecycle::Yielded);
        assert_eq!(
            b.reason,
            if grid {
                BatchReason::GridChanged
            } else {
                BatchReason::SourceChanged
            }
        );
        assert!(m.decks[1].channel_fader.target < 1.);
    }
}
#[test]
fn callback_arm_rechecks_deadline_and_preserves_first_owner() {
    let mut m = mixer();
    let (schedule, steps) = plan(&m);
    let batch = compile(1, &schedule, steps.clone(), &snapshot(&m)).unwrap();
    render(&mut m, 1.1);
    m.arm_batch(batch);
    assert_eq!(
        m.scheduled.as_ref().unwrap().reason,
        BatchReason::DeadlineExpired
    );
    assert!(!m.decks[1].playing);
    let mut m = mixer();
    let (schedule, steps) = plan(&m);
    let first = compile(1, &schedule, steps.clone(), &snapshot(&m)).unwrap();
    let second = compile(2, &schedule, steps, &snapshot(&m)).unwrap();
    m.arm_batch(first);
    let retired = m.arm_batch(second).unwrap();
    assert_eq!(retired.reason, BatchReason::ResourceConflict);
    assert_eq!(m.scheduled.as_ref().unwrap().id, 1);
}
#[test]
fn same_target_overlaps_fail_and_adjacent_segments_are_supported() {
    let m = mixer();
    let (mut schedule, mut steps) = plan(&m);
    schedule.completion_event = 30;
    steps.push(ScheduledCommand {
        at_event: 12,
        deadline_event: 12,
        command: ramp(1, ControlTarget::ChannelFader { deck: 1 }, 1., Some(4.), 1.),
        require: Some(BatchPrerequisite {
            deck: 1,
            playing: Some(true),
            ready: true,
            advanced_from_seconds: Some(0.),
            min_advanced_beats: 1.,
            ..Default::default()
        }),
    });
    assert!(compile(1, &schedule, steps.clone(), &snapshot(&m)).is_err());
    steps.last_mut().unwrap().at_event = 16;
    steps.last_mut().unwrap().deadline_event = 16;
    assert!(compile(1, &schedule, steps, &snapshot(&m)).is_ok());
}
#[test]
fn audio_interruption_is_terminal_and_never_false_completion() {
    let mut m = mixer();
    let (s, steps) = plan(&m);
    m.arm_batch(compile(1, &s, steps, &snapshot(&m)).unwrap());
    render(&mut m, 5.);
    m.interrupt_batch();
    render(&mut m, 20.);
    assert_eq!(
        m.scheduled.as_ref().unwrap().state,
        BatchLifecycle::Interrupted
    );
    assert_eq!(
        m.scheduled.as_ref().unwrap().reason,
        BatchReason::AudioStopped
    );
}
#[test]
fn rejected_launch_never_clears_the_healthy_outgoing_channel() {
    let mut m = mixer();
    let (mut schedule, steps) = plan(&m);
    schedule.completion_carrier = Some(1);
    schedule.recovery.carrier = 0;
    schedule.recovery.controls = vec![
        EngineCommand::SetChannelFader { deck: 0, value: 1. },
        EngineCommand::SetChannelFader { deck: 1, value: 0. },
        EngineCommand::SetEq {
            deck: 0,
            low: 1.,
            mid: 1.,
            high: 1.,
        },
    ];
    m.arm_batch(compile(1, &schedule, steps, &snapshot(&m)).unwrap());
    // Simulate preparation becoming unavailable after admission but before launch.
    m.decks[1].analysis_pending = true;
    for _ in 0..6_000 {
        m.frame();
        assert!(m.decks[0].playing);
        assert!(m.decks[0].channel_fader.target >= 0.999);
        assert_eq!(m.decks[1].channel_fader.target, 0.);
        assert!(!m.decks[1].playing);
    }
    let batch = m.scheduled.as_ref().unwrap();
    assert_eq!(batch.state, BatchLifecycle::Recovered);
    assert_ne!(
        batch.receipts[0].state, 2,
        "Failed launch must never be reported as completed"
    );
    assert!(batch.receipts[1..4].iter().all(|r| r.state != 2));
}
#[test]
fn failed_group_prerequisite_applies_none_of_its_controls() {
    let mut m = mixer();
    let (s, mut steps) = plan(&m);
    steps[2].require.as_mut().unwrap().min_advanced_beats = 100.;
    m.arm_batch(compile(1, &s, steps, &snapshot(&m)).unwrap());
    render(&mut m, 4.1);
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.state, BatchLifecycle::Recovering);
    assert_eq!(b.reason, BatchReason::PrerequisiteFailed);
    assert_eq!(b.receipts[1].state, 3);
    assert_eq!(b.receipts[2].state, 3);
    assert_eq!(b.receipts[3].state, 3);
}
#[test]
fn event_indices_follow_loaded_nonuniform_grid_and_revision_not_mean_bpm() {
    let mut m = mixer();
    let mut track = fixture(240.);
    let data = Arc::get_mut(&mut track).unwrap();
    data.analysis.neural_rhythm = Some(audio_analysis::NeuralRhythm {
        beats_seconds: vec![0.2, 0.7, 1.3, 1.9, 2.6, 3.2, 3.8, 4.4, 5.],
        downbeats_seconds: vec![0.2, 2.6, 5.],
        bpm: 120.,
        beat_confidence: 0.8,
        downbeat_confidence: 0.7,
        model: "fixture".into(),
        analysis_seconds: 0.,
        bar_evidence: Vec::new(),
        beats_per_bar: Some(4),
    });
    data.analysis.beatgrid_offset_seconds = Some(0.2);
    data.analysis.bpm_source = Some("beat_this".into());
    data.analysis.beatgrid_source = Some("beat_this".into());
    m.load(0, track);
    m.decks[0].grid_uniform = false;
    let s = snapshot(&m);
    assert!((event_seconds(&s, 0, 4).unwrap() - 2.6).abs() < 1e-8);
    assert!(event_seconds(&s, 0, 9).is_err());
    let before = m.grid_revisions[0];
    m.command(EngineCommand::SetBeatgrid {
        deck: 0,
        offset_seconds: 0.7,
        anchor_index: Some(1),
    });
    assert!(m.grid_revisions[0] > before);
    let s = snapshot(&m);
    assert!((event_seconds(&s, 0, 4).unwrap() - 2.6).abs() < 1e-8);
    m.command(EngineCommand::SetBpm { deck: 0, bpm: 100. });
    let s = snapshot(&m);
    assert!(s.decks[0].beatgrid_uniform);
    assert!((event_seconds(&s, 0, 4).unwrap() - 3.1).abs() < 1e-8);
}
#[test]
fn synced_nonuniform_handover_retains_native_rate_ownership_but_manual_tempo_yields() {
    for manual_takeover in [false, true] {
        let mut m = mixer();
        for deck in 0..2 {
            let mut track = fixture(240.);
            let data = Arc::get_mut(&mut track).unwrap();
            let mut time = 0.;
            let beats: Vec<f64> = (0..480)
                .map(|i| {
                    let at = time;
                    time += if (i + deck * 4) % 8 < 4 { 0.505 } else { 0.495 };
                    at
                })
                .collect();
            data.analysis.neural_rhythm = Some(audio_analysis::NeuralRhythm {
                downbeats_seconds: beats.iter().step_by(4).copied().collect(),
                beats_seconds: beats,
                bpm: 120.,
                beat_confidence: 0.8,
                downbeat_confidence: 0.8,
                model: "fixture".into(),
                analysis_seconds: 0.,
                bar_evidence: vec![],
                beats_per_bar: Some(4),
            });
            data.analysis.beatgrid_offset_seconds = Some(0.);
            data.analysis.bpm_source = Some("beat_this".into());
            data.analysis.beatgrid_source = Some("beat_this".into());
            m.load(deck, track);
        }
        m.command(EngineCommand::SetMaster { deck: 0 });
        // Explicit manual tempo now releases sync ownership, so prepare it
        // before enabling the native follower.
        m.command(EngineCommand::SetTempo {
            deck: 1,
            percent: 0.,
        });
        m.command(EngineCommand::SetSync {
            deck: 1,
            enabled: true,
        });
        m.command(EngineCommand::Play { deck: 0 });
        let (schedule, mut steps) = plan(&m);
        for step in &mut steps {
            if let Some(require) = &mut step.require {
                if require.phase_tolerance_beats.is_some() {
                    require.phase_tolerance_beats = Some(0.1);
                }
            }
        }
        let batch = compile(1, &schedule, steps, &snapshot(&m)).unwrap();
        // A transient internal base perturbation must settle to the canonical
        // tempo without claiming manual ownership or invalidating the batch.
        // Audible phase correction remains separate and bounded.
        m.decks[1].tempo += 0.2;
        render(&mut m, 0.1);
        assert!((m.decks[1].rate() - batch.rates[1]).abs() < 1e-10);
        m.arm_batch(batch);
        let mut minimum_rate = f64::INFINITY;
        let mut maximum_rate = f64::NEG_INFINITY;
        for _ in 0..4500 {
            m.frame();
            minimum_rate = minimum_rate.min(m.decks[1].actual_rate());
            maximum_rate = maximum_rate.max(m.decks[1].actual_rate());
        }
        assert_eq!(m.scheduled.as_ref().unwrap().state, BatchLifecycle::Running);
        assert!(maximum_rate - minimum_rate <= 0.03 + 1e-10);
        assert!((m.decks[1].rate() - 1.).abs() < 1e-10);
        if manual_takeover {
            m.command(EngineCommand::SetTempo {
                deck: 1,
                percent: 3.,
            });
            assert_eq!(m.scheduled.as_ref().unwrap().state, BatchLifecycle::Yielded);
            assert_eq!(
                m.scheduled.as_ref().unwrap().reason,
                BatchReason::ManualTakeover
            );
        } else {
            render(&mut m, 9.);
            let batch = m.scheduled.as_ref().unwrap();
            assert_eq!(batch.state, BatchLifecycle::Completed);
            assert_eq!(batch.reason, BatchReason::None);
            assert!(batch.receipts[..batch.len].iter().all(|r| r.state == 2));
            assert!(m.decks[1].playing);
            assert!(!m.decks[0].playing);
        }
    }
}
#[test]
fn documented_json_fixture_runs_offline_without_rebinding_or_model_calls() {
    let mut m = mixer();
    let args: serde_json::Value = serde_json::from_str(include_str!(
        "../../../research/examples/scheduled-handover.fixture.json"
    ))
    .unwrap();
    let schedule: BatchSchedule = serde_json::from_value(args["schedule"].clone()).unwrap();
    let commands = args["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| {
            let a = &step["args"];
            let deck = |v: &serde_json::Value| if v == "A" { 0 } else { 1 };
            let command = if step["command"] == "engine.command" {
                EngineCommand::PlayQuantized {
                    deck: deck(&a["deck"]),
                    clock: deck(&a["clock"]),
                    clock_revision: a["clock_revision"].as_u64().unwrap(),
                    at_seconds: None,
                    quantize: a["quantize"].as_f64().unwrap(),
                }
            } else {
                let target = if a["target"]["type"] == "channel_fader" {
                    ControlTarget::ChannelFader {
                        deck: deck(&a["target"]["deck"]),
                    }
                } else {
                    ControlTarget::EqLow {
                        deck: deck(&a["target"]["deck"]),
                    }
                };
                EngineCommand::Ramp {
                    target,
                    value: a["value"].as_f64().unwrap() as f32,
                    clock: Some(deck(&a["clock"])),
                    beats: Some(a["beats"].as_f64().unwrap()),
                    seconds: 1.,
                    quantize: None,
                }
            };
            ScheduledCommand {
                at_event: step["at_event"].as_i64().unwrap(),
                deadline_event: step["at_event"].as_i64().unwrap(),
                command,
                require: Some(serde_json::from_value(step["require"].clone()).unwrap()),
            }
        })
        .collect();
    m.arm_batch(compile(7, &schedule, commands, &snapshot(&m)).unwrap());
    render(&mut m, 120.);
    assert_eq!(
        m.scheduled.as_ref().unwrap().state,
        BatchLifecycle::Completed
    );
    assert!(m.decks[1].playing);
    assert_eq!(m.decks[1].channel_fader.target, 1.);
    assert_eq!(m.decks[0].channel_fader.target, 0.);
}
#[test]
fn maximum_capacity_execution_and_publication_allocate_and_free_nothing() {
    let mut m = mixer();
    let (mut schedule, mut commands) = plan(&m);
    schedule.completion_event = 80;
    for i in 4..MAX_SCHEDULED_STEPS {
        commands.push(ScheduledCommand {
            at_event: 16 + (i - 4) as i64,
            deadline_event: 16 + (i - 4) as i64,
            command: ramp(
                1,
                ControlTarget::EqLow { deck: 1 },
                if i % 2 == 0 { 0.9 } else { 1. },
                None,
                0.01,
            ),
            require: None,
        });
    }
    let batch = compile(88, &schedule, commands, &snapshot(&m)).unwrap();
    let atoms = SnapshotAtoms::new();
    let counts = crate::realtime_audit::allocation_counts(|| {
        m.arm_batch(batch);
        for _ in 0..45_000 {
            m.frame();
            if m.rendered_frames.is_multiple_of(64) {
                atoms.publish(&m, 2, 64, [0.; 2], [0.; 2]);
            }
        }
    });
    assert_eq!(
        counts, [0; 3],
        "alloc,realloc,dealloc in native execution/publication"
    );
    assert_eq!(
        m.scheduled.as_ref().unwrap().state,
        BatchLifecycle::Completed
    );
    assert_eq!(m.scheduled.as_ref().unwrap().completed, 64);
}

fn open_queue(m: &Mixer, initial: Vec<ScheduledCommand>) -> Box<NativeBatch> {
    let (mut schedule, _) = plan(m);
    schedule.open = true;
    schedule.expiry_seconds = Some(120.);
    schedule.completion_event = 200;
    schedule.completion_carrier = Some(1);
    schedule.recovery.carrier = 0;
    schedule.recovery.controls = vec![
        EngineCommand::SetChannelFader { deck: 0, value: 1. },
        EngineCommand::SetChannelFader { deck: 1, value: 0. },
        EngineCommand::SetEq {
            deck: 0,
            low: 1.,
            mid: 1.,
            high: 1.,
        },
        EngineCommand::SetEq {
            deck: 1,
            low: 1.,
            mid: 1.,
            high: 1.,
        },
    ];
    compile(500, &schedule, initial, &snapshot(m)).unwrap()
}

#[test]
fn listening_controls_preserve_queue_but_deck_takeover_still_yields() {
    let mut m = mixer();
    let (_, steps) = plan(&m);
    m.arm_batch(open_queue(&m, vec![steps[0].clone()]));
    for command in [
        EngineCommand::SetMasterGain { gain: 0.4 },
        EngineCommand::SetCueGain { gain: 0.3 },
        EngineCommand::SetCueMix { mix: 0.2 },
        EngineCommand::SetHeadphoneCue {
            deck: 1,
            enabled: true,
        },
    ] {
        m.command(command);
        let b = m.scheduled.as_ref().unwrap();
        assert_eq!(b.state, BatchLifecycle::Armed);
        assert_eq!(b.control_revision, m.control_revision);
    }
    for _ in 0..2100 {
        m.frame();
    }
    assert!(m.decks[1].playing);
    assert_eq!(m.scheduled.as_ref().unwrap().receipts[0].state, 2);
    m.command(EngineCommand::SetChannelFader {
        deck: 1,
        value: 0.4,
    });
    assert_eq!(m.scheduled.as_ref().unwrap().state, BatchLifecycle::Yielded);
    assert_eq!(
        m.scheduled.as_ref().unwrap().reason,
        BatchReason::ManualTakeover
    );
}
fn queue_patch(
    patch_id: u64,
    expected_revision: u64,
    append: Vec<ScheduledQueueAction>,
    invalidate: Vec<u64>,
    seal: Option<i64>,
) -> BatchQueuePatch {
    BatchQueuePatch {
        patch_id,
        expected_revision,
        append,
        invalidate,
        seal,
    }
}
fn queue_actions(commands: Vec<ScheduledCommand>, first_id: u64) -> Vec<ScheduledQueueAction> {
    commands
        .into_iter()
        .enumerate()
        .map(|(i, step)| ScheduledQueueAction {
            action_id: first_id + i as u64,
            step,
        })
        .collect()
}
fn apply_patch(m: &mut Mixer, p: BatchQueuePatch) {
    let compiled = compile_patch(500, p, &snapshot(m)).unwrap();
    drop(m.patch_batch(compiled));
}
#[test]
fn open_queue_launch_then_append_and_seal_never_replays_completed_launch() {
    let mut m = mixer();
    let (_, steps) = plan(&m);
    m.arm_batch(open_queue(&m, vec![steps[0].clone()]));
    render(&mut m, 2.6);
    let launch = m.scheduled.as_ref().unwrap().receipts[0];
    assert_eq!(launch.state, 2);
    assert!(m.decks[1].playing);
    let before = m.decks[1].position_revision;
    apply_patch(
        &mut m,
        queue_patch(1, 1, queue_actions(steps[1..].to_vec(), 2), vec![], None),
    );
    assert_eq!(m.scheduled.as_ref().unwrap().revision, 2);
    render(&mut m, 7.);
    // Every completed action is observable before sealing/global completion.
    assert!(m.scheduled.as_ref().unwrap().receipts[..4]
        .iter()
        .all(|r| r.state == 2));
    assert!(m.scheduled.as_ref().unwrap().open);
    assert_eq!(m.decks[1].position_revision, before);
    assert_eq!(
        m.scheduled.as_ref().unwrap().receipts[0].frame,
        launch.frame
    );
    apply_patch(&mut m, queue_patch(2, 2, vec![], vec![], Some(24)));
    render(&mut m, 120.);
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.state, BatchLifecycle::Completed);
    assert_eq!(b.revision, 3);
    assert!(!m.decks[0].playing);
    assert!(m.decks[1].playing);
    assert_eq!(b.receipts[0].frame, launch.frame);
}
#[test]
fn queue_cas_races_retries_and_conflicting_ids_preserve_first_accepted_work() {
    let mut m = mixer();
    let (_, steps) = plan(&m);
    m.arm_batch(open_queue(&m, vec![]));
    let first = queue_patch(1, 1, queue_actions(vec![steps[0].clone()], 1), vec![], None);
    let p1 = compile_patch(500, first.clone(), &snapshot(&m)).unwrap();
    let retry = compile_patch(500, first, &snapshot(&m)).unwrap();
    let raced = compile_patch(
        500,
        queue_patch(2, 1, queue_actions(vec![steps[0].clone()], 2), vec![], None),
        &snapshot(&m),
    )
    .unwrap();
    drop(m.patch_batch(p1));
    drop(m.patch_batch(retry));
    assert_eq!(m.scheduled.as_ref().unwrap().revision, 2);
    assert_eq!(m.scheduled.as_ref().unwrap().update_count, 1);
    drop(m.patch_batch(raced));
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.len, 1);
    assert_eq!(b.revision, 2);
    assert_eq!(b.updates[1].reason, BatchReason::RevisionConflict);
    assert!(compile_patch(
        500,
        queue_patch(3, 2, queue_actions(vec![steps[0].clone()], 1), vec![], None),
        &snapshot(&m)
    )
    .is_err());
    render(&mut m, 3.);
    assert!(m.decks[1].playing);
}
#[test]
fn queue_invalidation_is_atomic_for_groups_and_launch_dependencies() {
    let mut m = mixer();
    let (_, steps) = plan(&m);
    m.arm_batch(open_queue(&m, steps));
    apply_patch(&mut m, queue_patch(1, 1, vec![], vec![2], None));
    assert_eq!(
        m.scheduled.as_ref().unwrap().updates[0].reason,
        BatchReason::InvalidPatch
    );
    assert_eq!(m.scheduled.as_ref().unwrap().revision, 1);
    apply_patch(&mut m, queue_patch(2, 1, vec![], vec![1], None));
    assert_eq!(
        m.scheduled.as_ref().unwrap().updates[1].reason,
        BatchReason::InvalidPatch
    );
    apply_patch(&mut m, queue_patch(3, 1, vec![], vec![1, 2, 3, 4], None));
    assert_eq!(m.scheduled.as_ref().unwrap().revision, 2);
    render(&mut m, 10.);
    assert!(!m.decks[1].playing);
    assert!(m.scheduled.as_ref().unwrap().receipts[..4]
        .iter()
        .all(|r| r.state == 3));
}
#[test]
fn queue_patch_late_or_started_invalidation_and_bad_seal_leave_queue_intact() {
    let mut m = mixer();
    let (_, steps) = plan(&m);
    m.arm_batch(open_queue(&m, vec![steps[0].clone()]));
    let pending =
        compile_patch(500, queue_patch(1, 1, vec![], vec![1], None), &snapshot(&m)).unwrap();
    render(&mut m, 2.5);
    drop(m.patch_batch(pending));
    assert!(matches!(
        m.scheduled.as_ref().unwrap().updates[0].reason,
        BatchReason::InvalidPatch | BatchReason::PositionChanged
    ));
    assert!(m.decks[1].playing);
    apply_patch(&mut m, queue_patch(2, 1, vec![], vec![], Some(24)));
    assert_eq!(
        m.scheduled.as_ref().unwrap().updates[1].reason,
        BatchReason::EndpointFailed
    );
    assert!(m.scheduled.as_ref().unwrap().open);
    let mut m = mixer();
    let (_, steps) = plan(&m);
    m.arm_batch(open_queue(&m, vec![]));
    let late = compile_patch(
        500,
        queue_patch(1, 1, queue_actions(vec![steps[0].clone()], 1), vec![], None),
        &snapshot(&m),
    )
    .unwrap();
    render(&mut m, 2.5);
    drop(m.patch_batch(late));
    assert_eq!(
        m.scheduled.as_ref().unwrap().updates[0].reason,
        BatchReason::DeadlineExpired
    );
    assert_eq!(m.scheduled.as_ref().unwrap().len, 0);
    assert!(!m.decks[1].playing);
}
#[test]
fn queue_lifetime_capacity_includes_invalidated_ids_and_never_reuses_slots() {
    let mut m = mixer();
    m.arm_batch(open_queue(&m, vec![]));
    let actions = (0..64)
        .map(|i| ScheduledQueueAction {
            action_id: i + 1,
            step: ScheduledCommand {
                at_event: 4 + i as i64,
                deadline_event: 4 + i as i64,
                command: EngineCommand::SetEq {
                    deck: 1,
                    low: 1.,
                    mid: 1.,
                    high: 1.,
                },
                require: None,
            },
        })
        .collect();
    apply_patch(&mut m, queue_patch(1, 1, actions, vec![], None));
    assert_eq!(m.scheduled.as_ref().unwrap().len, 64);
    apply_patch(&mut m, queue_patch(2, 2, vec![], (1..=64).collect(), None));
    let extra = ScheduledQueueAction {
        action_id: 65,
        step: ScheduledCommand {
            at_event: 80,
            deadline_event: 80,
            command: EngineCommand::SetEq {
                deck: 1,
                low: 1.,
                mid: 1.,
                high: 1.,
            },
            require: None,
        },
    };
    assert!(compile_patch(
        500,
        queue_patch(3, 3, vec![extra], vec![], None),
        &snapshot(&m)
    )
    .is_err());
    assert!(m.scheduled.as_ref().unwrap().receipts[..64]
        .iter()
        .all(|r| r.state == 3));
}
#[test]
fn open_queue_expiry_eof_and_manual_changes_have_bounded_honest_outcomes() {
    for manual in [false, true] {
        let mut m = mixer();
        let mut b = open_queue(&m, vec![]);
        b.expires_frame = 500;
        m.arm_batch(b);
        if manual {
            m.command(EngineCommand::SetChannelFader {
                deck: 0,
                value: 0.4,
            });
        }
        render(&mut m, 1.);
        let b = m.scheduled.as_ref().unwrap();
        assert_eq!(
            b.state,
            if manual {
                BatchLifecycle::Yielded
            } else {
                BatchLifecycle::Recovered
            }
        );
        assert_eq!(
            b.reason,
            if manual {
                BatchReason::ManualTakeover
            } else {
                BatchReason::DeadlineExpired
            }
        );
        assert_eq!(
            m.decks[0].channel_fader.target,
            if manual { 0.4 } else { 1. }
        );
    }
    let mut m = mixer();
    m.arm_batch(open_queue(&m, vec![]));
    m.decks[0].playing = false;
    render(&mut m, 1.);
    assert_eq!(
        m.scheduled.as_ref().unwrap().reason,
        BatchReason::ClockStopped
    );
    assert_eq!(m.scheduled.as_ref().unwrap().state, BatchLifecycle::Failed);
}
#[test]
fn queue_patch_execution_and_action_publication_allocate_and_free_nothing() {
    let mut m = mixer();
    let (_, steps) = plan(&m);
    m.arm_batch(open_queue(&m, vec![]));
    let patch = compile_patch(
        500,
        queue_patch(1, 1, queue_actions(vec![steps[0].clone()], 1), vec![], None),
        &snapshot(&m),
    )
    .unwrap();
    let atoms = SnapshotAtoms::new();
    let mut retired = None;
    let counts = crate::realtime_audit::allocation_counts(|| {
        retired = Some(m.patch_batch(patch));
        for _ in 0..4_000 {
            m.frame();
            if m.rendered_frames.is_multiple_of(64) {
                atoms.publish(&m, 2, 64, [0.; 2], [0.; 2]);
            }
        }
    });
    assert_eq!(counts, [0; 3]);
    assert!(m.decks[1].playing);
    drop(retired);
}
#[test]
fn queue_update_source_race_and_stream_stop_never_restore_or_replay_work() {
    for source_changed in [false, true] {
        let mut m = mixer();
        let (_, steps) = plan(&m);
        m.arm_batch(open_queue(&m, vec![]));
        let compiled = compile_patch(
            500,
            queue_patch(1, 1, queue_actions(vec![steps[0].clone()], 1), vec![], None),
            &snapshot(&m),
        )
        .unwrap();
        if source_changed {
            m.load_revisions[1] += 1;
        } else {
            m.interrupt_batch();
        }
        drop(m.patch_batch(compiled));
        render(&mut m, 3.);
        let b = m.scheduled.as_ref().unwrap();
        assert_eq!(b.len, 0);
        assert!(!m.decks[1].playing);
        assert_eq!(b.revision, 1);
        assert_eq!(
            b.state,
            if source_changed {
                BatchLifecycle::Yielded
            } else {
                BatchLifecycle::Interrupted
            }
        );
        assert_eq!(m.decks[1].channel_fader.target, 0.);
    }
}

fn command_patch(
    patch_id: u64,
    expected_revision: u64,
    append: Vec<QueuedCommand>,
    invalidate: Vec<u64>,
) -> CommandQueuePatch {
    CommandQueuePatch {
        rebind_dependencies: vec![],
        patch_id,
        expected_revision,
        append,
        invalidate,
    }
}
fn queued(action_id: u64, command: EngineCommand, when: QueueTiming) -> QueuedCommand {
    QueuedCommand {
        group_id: None,
        action_id,
        command,
        when,
    }
}
fn arm_commands(m: &mut Mixer) {
    let b = compile_command_queue(900, &snapshot(m)).unwrap();
    assert!(m.arm_batch(b).is_none());
}
fn apply_commands(m: &mut Mixer, p: CommandQueuePatch) {
    let p = compile_command_patch(900, p, &snapshot(m)).unwrap();
    drop(m.patch_batch(p));
}
#[test]
fn generic_commands_idle_without_expiry_then_append_after_execution_and_rotate() {
    let mut m = mixer();
    m.command(EngineCommand::Pause { deck: 0 });
    arm_commands(&mut m);
    render(&mut m, 130.);
    assert_eq!(m.scheduled.as_ref().unwrap().state, BatchLifecycle::Armed);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(1, EngineCommand::Play { deck: 0 }, QueueTiming::Now)],
            vec![],
        ),
    );
    render(&mut m, 0.01);
    assert!(m.decks[0].playing);
    assert_eq!(m.scheduled.as_ref().unwrap().receipts[0].state, 2);
    let frame = m.scheduled.as_ref().unwrap().receipts[0].frame;
    apply_commands(
        &mut m,
        command_patch(
            2,
            2,
            vec![queued(
                2,
                EngineCommand::SetChannelFader {
                    deck: 0,
                    value: 0.6,
                },
                QueueTiming::Now,
            )],
            vec![],
        ),
    );
    render(&mut m, 0.2);
    assert_eq!(
        m.scheduled.as_ref().unwrap().state,
        BatchLifecycle::Completed
    );
    assert_eq!(m.scheduled.as_ref().unwrap().receipts[0].frame, frame);
    assert_eq!(m.scheduled.as_ref().unwrap().receipts[1].state, 2);
    assert_eq!(m.decks[0].channel_fader.target, 0.6);
    render(&mut m, 120.);
    assert_eq!(m.decks[0].channel_fader.target, 0.6);
    let b = compile_command_queue(901, &snapshot(&m)).unwrap();
    assert!(m.arm_batch(b).is_some());
    assert_eq!(m.scheduled.as_ref().unwrap().id, 901);
}
#[test]
fn generic_source_timing_auto_snaps_entry_and_relative_dependency_is_exact() {
    let mut m = mixer();
    m.command(EngineCommand::Seek {
        deck: 1,
        seconds: 0.333,
    });
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![
                queued(
                    1,
                    EngineCommand::Play { deck: 1 },
                    QueueTiming::AtEvent { clock: 0, event: 4 },
                ),
                queued(
                    2,
                    EngineCommand::SetFilter {
                        deck: 1,
                        amount: 0.3,
                    },
                    QueueTiming::AfterAction {
                        action_id: 1,
                        seconds: Some(0.25),
                        clock: None,
                        beats: None,
                        quantize: None,
                    },
                ),
            ],
            vec![],
        ),
    );
    render(&mut m, 3.);
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.receipts[0].state, 2);
    assert_eq!(b.receipts[1].state, 2);
    assert_eq!(b.receipts[1].frame, b.receipts[0].completed_frame + 250);
    assert!((m.decks[1].seconds() - 1.5).abs() < 0.002);
    assert_eq!(m.decks[0].channel_fader.target, 1.);
    assert_eq!(m.decks[1].channel_fader.target, 0.);
}
#[test]
fn generic_now_command_is_not_blocked_by_older_future_item() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetEq {
                    deck: 0,
                    low: 0.7,
                    mid: 1.,
                    high: 1.,
                },
                QueueTiming::AtEvent {
                    clock: 0,
                    event: 20,
                },
            )],
            vec![],
        ),
    );
    apply_commands(
        &mut m,
        command_patch(
            2,
            2,
            vec![queued(
                2,
                EngineCommand::SetFilter {
                    deck: 1,
                    amount: 0.2,
                },
                QueueTiming::Now,
            )],
            vec![],
        ),
    );
    render(&mut m, 0.1);
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.receipts[0].state, 0);
    assert_eq!(b.receipts[1].state, 2);
    assert_eq!(m.decks[0].eq[0].target, 1.);
}
#[test]
fn generic_remove_and_replace_pending_are_atomic_and_never_rewrite_executed() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetFilter {
                    deck: 1,
                    amount: 0.8,
                },
                QueueTiming::AfterSeconds { seconds: 1. },
            )],
            vec![],
        ),
    );
    apply_commands(
        &mut m,
        command_patch(
            2,
            2,
            vec![queued(
                2,
                EngineCommand::SetFilter {
                    deck: 1,
                    amount: 0.2,
                },
                QueueTiming::AfterSeconds { seconds: 1. },
            )],
            vec![1],
        ),
    );
    render(&mut m, 1.2);
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.receipts[0].state, 3);
    assert_eq!(b.receipts[1].state, 2);
    assert_eq!(m.decks[1].filter.target, 0.2);
    assert!(
        compile_command_patch(900, command_patch(3, 3, vec![], vec![2]), &snapshot(&m)).is_err()
    );
}
#[test]
fn generic_owned_global_control_yields_to_manual_but_unrelated_ramp_survives_stop() {
    let mut m = mixer();
    m.command(EngineCommand::Ramp {
        target: ControlTarget::CueGain,
        value: 0.,
        seconds: 30.,
        clock: None,
        beats: None,
        quantize: None,
    });
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::Ramp {
                    target: ControlTarget::MasterGain,
                    value: 1.5,
                    seconds: 10.,
                    clock: None,
                    beats: None,
                    quantize: None,
                },
                QueueTiming::Now,
            )],
            vec![],
        ),
    );
    render(&mut m, 0.1);
    assert!(m.active_ramps()[ControlTarget::CueGain.index()]);
    assert!(m.active_ramps()[ControlTarget::MasterGain.index()]);
    m.command(EngineCommand::SetMasterGain { gain: 0. });
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.state, BatchLifecycle::Yielded);
    assert_eq!(b.receipts[0].state, 3);
    assert!(m.active_ramps()[ControlTarget::CueGain.index()]);
    assert!(!m.active_ramps()[ControlTarget::MasterGain.index()]);
    assert_eq!(m.master_gain.target, 0.);
    let mut m = mixer();
    m.command(EngineCommand::Ramp {
        target: ControlTarget::CueGain,
        value: 0.,
        seconds: 30.,
        clock: None,
        beats: None,
        quantize: None,
    });
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetFilter {
                    deck: 1,
                    amount: 0.5,
                },
                QueueTiming::AfterSeconds { seconds: 10. },
            )],
            vec![],
        ),
    );
    m.cancel_batch(900, false);
    assert!(m.active_ramps()[ControlTarget::CueGain.index()]);
    assert_eq!(m.decks[1].filter.target, 0.);
}
#[test]
fn generic_no_dj_recovery_on_command_error_source_change_or_eof() {
    for source_changed in [false, true] {
        let mut m = mixer();
        arm_commands(&mut m);
        apply_commands(
            &mut m,
            command_patch(
                1,
                1,
                vec![queued(
                    1,
                    EngineCommand::SetFilter {
                        deck: 1,
                        amount: 0.5,
                    },
                    QueueTiming::AtEvent {
                        clock: 0,
                        event: 20,
                    },
                )],
                vec![],
            ),
        );
        if source_changed {
            m.load_revisions[1] += 1;
        } else {
            m.decks[0].position = m.decks[0].duration_frames();
            m.decks[0].playing = false;
        }
        render(&mut m, 0.1);
        let b = m.scheduled.as_ref().unwrap();
        assert!(matches!(
            b.state,
            BatchLifecycle::Yielded | BatchLifecycle::Failed
        ));
        assert_eq!(m.decks[1].filter.target, 0.);
        assert_eq!(m.decks[1].channel_fader.target, 0.);
    }
}
#[test]
fn generic_after_action_beats_and_seconds_start_from_verified_completion() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![
                queued(
                    1,
                    EngineCommand::SetFilter {
                        deck: 0,
                        amount: 0.4,
                    },
                    QueueTiming::Now,
                ),
                queued(
                    2,
                    EngineCommand::SetEq {
                        deck: 1,
                        low: 0.7,
                        mid: 1.,
                        high: 1.,
                    },
                    QueueTiming::AfterAction {
                        action_id: 1,
                        seconds: None,
                        clock: Some(0),
                        beats: Some(2.),
                        quantize: None,
                    },
                ),
                queued(
                    3,
                    EngineCommand::SetFilter {
                        deck: 1,
                        amount: 0.3,
                    },
                    QueueTiming::AfterAction {
                        action_id: 1,
                        seconds: Some(1.),
                        clock: None,
                        beats: None,
                        quantize: None,
                    },
                ),
            ],
            vec![],
        ),
    );
    render(&mut m, 2.);
    let b = m.scheduled.as_ref().unwrap();
    assert!(b.receipts[..3].iter().all(|r| r.state == 2));
    assert_eq!(b.receipts[1].frame, b.receipts[0].completed_frame + 1000);
    assert_eq!(b.receipts[2].frame, b.receipts[1].frame);
}
#[test]
fn generic_callback_updates_and_completion_allocate_and_free_nothing() {
    let mut m = mixer();
    arm_commands(&mut m);
    let p = compile_command_patch(
        900,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::Ramp {
                    target: ControlTarget::MasterGain,
                    value: 1.,
                    seconds: 0.2,
                    clock: None,
                    beats: None,
                    quantize: None,
                },
                QueueTiming::Now,
            )],
            vec![],
        ),
        &snapshot(&m),
    )
    .unwrap();
    let atoms = SnapshotAtoms::new();
    let mut retired = None;
    let counts = crate::realtime_audit::allocation_counts(|| {
        retired = Some(m.patch_batch(p));
        for _ in 0..1000 {
            m.frame();
            if m.rendered_frames.is_multiple_of(64) {
                atoms.publish(&m, 2, 64, [0.; 2], [0.; 2]);
            }
        }
    });
    assert_eq!(counts, [0; 3]);
    assert_eq!(m.scheduled.as_ref().unwrap().receipts[0].state, 2);
    drop(retired);
}

fn replace_source(m: &mut Mixer, deck: usize) {
    m.load_revisions[deck] += 1;
    drop(m.load(deck, fixture(240.)));
}
#[test]
fn generic_unrelated_source_replacement_preserves_a_and_later_b_append_captures_new_source() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetFilter {
                    deck: 0,
                    amount: 0.3,
                },
                QueueTiming::AfterSeconds { seconds: 1. },
            )],
            vec![],
        ),
    );
    assert_eq!(snapshot(&m).scheduled_batch.owned_decks, 1);
    replace_source(&mut m, 1);
    render(&mut m, 0.1);
    assert!(m.scheduled.as_ref().unwrap().state.active());
    apply_commands(
        &mut m,
        command_patch(
            2,
            2,
            vec![queued(2, EngineCommand::Play { deck: 1 }, QueueTiming::Now)],
            vec![],
        ),
    );
    assert!(m.scheduled.as_ref().unwrap().updates[1].accepted);
    assert_eq!(
        m.scheduled.as_ref().unwrap().steps[1]
            .as_ref()
            .unwrap()
            .source_revisions[1],
        m.load_revisions[1]
    );
    render(&mut m, 1.2);
    assert!(m.decks[1].playing);
    assert_eq!(m.decks[0].filter.target, 0.3);
    assert!(m.scheduled.as_ref().unwrap().receipts[..2]
        .iter()
        .all(|r| r.state == 2));
}
#[test]
fn generic_owned_source_replacement_cancels_and_cannot_be_rebased_by_append() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetFilter {
                    deck: 1,
                    amount: 0.7,
                },
                QueueTiming::AfterSeconds { seconds: 1. },
            )],
            vec![],
        ),
    );
    replace_source(&mut m, 1);
    let p = compile_command_patch(
        900,
        command_patch(
            2,
            2,
            vec![queued(2, EngineCommand::Play { deck: 1 }, QueueTiming::Now)],
            vec![],
        ),
        &snapshot(&m),
    )
    .unwrap();
    drop(m.patch_batch(p));
    assert!(!m.scheduled.as_ref().unwrap().updates[1].accepted);
    assert_eq!(
        m.scheduled.as_ref().unwrap().updates[1].reason,
        BatchReason::SourceChanged
    );
    assert_eq!(m.scheduled.as_ref().unwrap().len, 1);
    render(&mut m, 1.2);
    assert_eq!(m.scheduled.as_ref().unwrap().state, BatchLifecycle::Yielded);
    assert_eq!(m.scheduled.as_ref().unwrap().receipts[0].state, 3);
    assert_eq!(m.decks[1].filter.target, 0.);
    assert!(!m.decks[1].playing);
}
#[test]
fn generic_timing_clock_is_owned_even_when_control_target_is_other_deck() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetFilter {
                    deck: 1,
                    amount: 0.7,
                },
                QueueTiming::AtEvent { clock: 0, event: 8 },
            )],
            vec![],
        ),
    );
    assert_eq!(snapshot(&m).scheduled_batch.owned_decks, 3);
    replace_source(&mut m, 0);
    render(&mut m, 0.01);
    assert_eq!(
        m.scheduled.as_ref().unwrap().reason,
        BatchReason::SourceChanged
    );
    assert_eq!(m.decks[1].filter.target, 0.);
}
#[test]
fn generic_completed_deck_controls_release_ownership_and_unrelated_manual_commands_survive() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetFilter {
                    deck: 1,
                    amount: 0.3,
                },
                QueueTiming::Now,
            )],
            vec![],
        ),
    );
    render(&mut m, 0.2);
    assert_eq!(snapshot(&m).scheduled_batch.owned_decks, 0);
    apply_commands(
        &mut m,
        command_patch(
            2,
            2,
            vec![queued(
                2,
                EngineCommand::SetFilter {
                    deck: 0,
                    amount: 0.4,
                },
                QueueTiming::AfterSeconds { seconds: 1. },
            )],
            vec![],
        ),
    );
    m.command(EngineCommand::Seek {
        deck: 1,
        seconds: 5.,
    });
    m.command(EngineCommand::SetBpm { deck: 1, bpm: 123. });
    assert!(m.scheduled.as_ref().unwrap().state.active());
    assert_eq!(snapshot(&m).scheduled_batch.owned_decks, 1);
    replace_source(&mut m, 1);
    render(&mut m, 1.2);
    assert_eq!(m.scheduled.as_ref().unwrap().receipts[1].state, 2);
    assert_eq!(m.decks[0].filter.target, 0.4);
}
#[test]
fn generic_scoped_manual_revision_cas_rejects_owned_race_but_allows_unrelated_race() {
    let mut m = mixer();
    arm_commands(&mut m);
    let p = compile_command_patch(
        900,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetFilter {
                    deck: 0,
                    amount: 0.4,
                },
                QueueTiming::AfterSeconds { seconds: 1. },
            )],
            vec![],
        ),
        &snapshot(&m),
    )
    .unwrap();
    m.command(EngineCommand::Seek {
        deck: 1,
        seconds: 5.,
    });
    drop(m.patch_batch(p));
    assert!(m.scheduled.as_ref().unwrap().updates[0].accepted);
    let p = compile_command_patch(
        900,
        command_patch(
            2,
            2,
            vec![queued(
                2,
                EngineCommand::SetMasterGain { gain: 1.5 },
                QueueTiming::Now,
            )],
            vec![],
        ),
        &snapshot(&m),
    )
    .unwrap();
    m.command(EngineCommand::SetMasterGain { gain: 0. });
    drop(m.patch_batch(p));
    assert!(!m.scheduled.as_ref().unwrap().updates[1].accepted);
    assert_eq!(
        m.scheduled.as_ref().unwrap().updates[1].reason,
        BatchReason::ManualTakeover
    );
    assert_eq!(m.scheduled.as_ref().unwrap().len, 1);
    render(&mut m, 1.2);
    assert_eq!(m.decks[0].filter.target, 0.4);
    assert_eq!(m.master_gain.target, 0.);
}
#[test]
fn generic_completed_global_target_release_preserves_new_unrelated_ramp_on_cancel() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetMasterGain { gain: 0.9 },
                QueueTiming::Now,
            )],
            vec![],
        ),
    );
    render(&mut m, 0.2);
    assert_eq!(snapshot(&m).scheduled_batch.owned_targets, 0);
    apply_commands(
        &mut m,
        command_patch(
            2,
            2,
            vec![queued(
                2,
                EngineCommand::SetFilter {
                    deck: 0,
                    amount: 0.5,
                },
                QueueTiming::AfterSeconds { seconds: 1. },
            )],
            vec![],
        ),
    );
    m.command(EngineCommand::Ramp {
        target: ControlTarget::MasterGain,
        value: 0.,
        seconds: 10.,
        clock: None,
        beats: None,
        quantize: None,
    });
    assert!(m.scheduled.as_ref().unwrap().state.active());
    m.cancel_batch(900, false);
    assert!(m.active_ramps()[ControlTarget::MasterGain.index()]);
    assert_eq!(m.decks[0].filter.target, 0.);
}
#[test]
fn generic_old_completion_beat_context_cannot_attach_replacement_clock_source() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetFilter {
                    deck: 1,
                    amount: 0.3,
                },
                QueueTiming::Now,
            )],
            vec![],
        ),
    );
    render(&mut m, 0.2);
    replace_source(&mut m, 1);
    render(&mut m, 0.01);
    let p = command_patch(
        2,
        2,
        vec![queued(
            2,
            EngineCommand::SetCueGain { gain: 0.5 },
            QueueTiming::AfterAction {
                action_id: 1,
                seconds: None,
                clock: Some(1),
                beats: Some(4.),
                quantize: None,
            },
        )],
        vec![],
    );
    assert!(compile_command_patch(900, p, &snapshot(&m)).is_err());
    assert_eq!(m.scheduled.as_ref().unwrap().receipts[0].state, 2);
}
#[test]
fn generic_empty_arm_has_no_deck_ownership_and_new_items_capture_latest_context() {
    let mut m = mixer();
    let b = compile_command_queue(900, &snapshot(&m)).unwrap();
    replace_source(&mut m, 1);
    m.command(EngineCommand::Seek {
        deck: 1,
        seconds: 3.,
    });
    m.arm_batch(b);
    assert_eq!(m.scheduled.as_ref().unwrap().state, BatchLifecycle::Armed);
    assert_eq!(snapshot(&m).scheduled_batch.owned_decks, 0);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetFilter {
                    deck: 1,
                    amount: 0.3,
                },
                QueueTiming::Now,
            )],
            vec![],
        ),
    );
    render(&mut m, 0.2);
    assert_eq!(m.scheduled.as_ref().unwrap().receipts[0].state, 2);
    assert_eq!(m.decks[1].filter.target, 0.3);
}

#[test]
fn generic_headphone_listening_preserves_deck_work_but_owned_cue_target_yields() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetFilter {
                    deck: 0,
                    amount: 0.3,
                },
                QueueTiming::AfterSeconds { seconds: 1. },
            )],
            vec![],
        ),
    );
    m.command(EngineCommand::SetHeadphoneCue {
        deck: 0,
        enabled: true,
    });
    assert!(m.scheduled.as_ref().unwrap().state.active());
    render(&mut m, 1.2);
    assert_eq!(m.scheduled.as_ref().unwrap().receipts[0].state, 2);
    assert_eq!(m.decks[0].filter.target, 0.3);

    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetHeadphoneCue {
                    deck: 0,
                    enabled: false,
                },
                QueueTiming::AfterSeconds { seconds: 1. },
            )],
            vec![],
        ),
    );
    m.command(EngineCommand::SetHeadphoneCue {
        deck: 0,
        enabled: false,
    });
    assert_eq!(m.scheduled.as_ref().unwrap().state, BatchLifecycle::Yielded);
    assert_eq!(
        m.scheduled.as_ref().unwrap().reason,
        BatchReason::ManualTakeover
    );
}

fn parallel_ramps(first: u64, group: u64, when: QueueTiming) -> Vec<QueuedCommand> {
    [
        (ControlTarget::EqLow { deck: 0 }, 0.5, 0.1),
        (ControlTarget::TempoPercent { deck: 0 }, 3., 0.2),
        (ControlTarget::ChannelFader { deck: 0 }, 0.25, 0.3),
        (ControlTarget::EqHigh { deck: 0 }, 0.7, 0.4),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (target, value, seconds))| QueuedCommand {
        action_id: first + i as u64,
        group_id: Some(group),
        when,
        command: EngineCommand::Ramp {
            target,
            value,
            seconds,
            clock: None,
            beats: None,
            quantize: None,
        },
    })
    .collect()
}

#[test]
fn explicit_parallel_now_starts_same_frame_and_ramps_finish_independently() {
    let mut m = mixer();
    m.decks[0].sync_enabled = true;
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(1, 1, parallel_ramps(1, 7, QueueTiming::Now), vec![]),
    );
    m.frame();
    let b = m.scheduled.as_ref().unwrap();
    assert!(b.receipts[..4].iter().all(|r| r.state == 1 && r.frame == 0));
    assert!(!m.decks[0].sync_enabled);
    render(&mut m, 0.15);
    assert!((m.decks[0].tempo - 2.25).abs() < 0.02);
    assert!(m.active_ramps()[ControlTarget::TempoPercent { deck: 0 }.index()]);
    render(&mut m, 0.5);
    let b = m.scheduled.as_ref().unwrap();
    assert!(b.receipts[..4].iter().all(|r| r.state == 2));
    assert!(b.receipts[..4]
        .windows(2)
        .all(|r| r[0].completed_frame < r[1].completed_frame));
    assert_eq!(m.decks[0].tempo, 3.);
    assert_eq!(m.decks[0].bpm, Some(120.));
    assert_eq!(m.decks[0].eq[0].target, 0.5);
    assert_eq!(m.decks[0].channel_fader.target, 0.25);
    assert_eq!(m.decks[0].eq[2].target, 0.7);
}

#[test]
fn explicit_parallel_future_and_completion_dependencies_share_actual_anchor() {
    for when in [
        QueueTiming::AfterSeconds { seconds: 0.2 },
        QueueTiming::AfterBeats {
            clock: 0,
            beats: 0.4,
            quantize: None,
        },
    ] {
        let mut m = mixer();
        arm_commands(&mut m);
        let mut commands = parallel_ramps(1, 8, when);
        commands.extend(parallel_ramps(
            5,
            9,
            QueueTiming::AfterAction {
                action_id: 1,
                seconds: Some(0.1),
                clock: None,
                beats: None,
                quantize: None,
            },
        ));
        apply_commands(&mut m, command_patch(1, 1, commands, vec![]));
        render(&mut m, 1.5);
        let b = m.scheduled.as_ref().unwrap();
        assert!(
            b.receipts[..8].iter().all(|r| r.state == 2),
            "{:?}",
            &b.receipts[..8]
        );
        assert!(b.receipts[..4].iter().all(|r| r.frame == 200));
        let last = b.receipts[..4]
            .iter()
            .map(|r| r.completed_frame)
            .max()
            .unwrap();
        assert!(b.receipts[4..8].iter().all(|r| r.frame == last + 100));
    }
}

#[test]
fn explicit_parallel_validation_and_preflight_are_atomic() {
    let mut m = mixer();
    arm_commands(&mut m);
    let mut commands = parallel_ramps(1, 10, QueueTiming::Now);
    commands[1].command = EngineCommand::SetEq {
        deck: 0,
        low: 0.9,
        mid: 1.,
        high: 1.,
    };
    assert!(
        compile_command_patch(900, command_patch(1, 1, commands, vec![]), &snapshot(&m))
            .unwrap_err()
            .contains("conflicting")
    );
    assert_eq!(m.decks[0].eq[0].target, 1.);
    let mut commands = parallel_ramps(1, 11, QueueTiming::Now);
    commands[1].when = QueueTiming::AfterSeconds { seconds: 0.1 };
    assert!(
        compile_command_patch(900, command_patch(1, 1, commands, vec![]), &snapshot(&m))
            .unwrap_err()
            .contains("one timing")
    );
    let mut commands = parallel_ramps(1, 12, QueueTiming::Now);
    commands[1].command = EngineCommand::TriggerHotCue { deck: 1, index: 0 };
    apply_commands(&mut m, command_patch(1, 1, commands, vec![]));
    m.frame();
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.state, BatchLifecycle::Failed);
    assert!(b.receipts[..4].iter().all(|r| r.frame == 0 && r.state == 4));
    assert_eq!(m.decks[0].eq[0].target, 1.);
    assert_eq!(m.decks[0].channel_fader.target, 1.);
}

#[test]
fn explicit_parallel_pending_edit_is_whole_group_and_cancel_releases_only_owned_ramps() {
    let mut m = mixer();
    m.command(EngineCommand::Ramp {
        target: ControlTarget::CueGain,
        value: 0.,
        seconds: 5.,
        clock: None,
        beats: None,
        quantize: None,
    });
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            parallel_ramps(1, 13, QueueTiming::AfterSeconds { seconds: 0.2 }),
            vec![],
        ),
    );
    apply_commands(&mut m, command_patch(2, 2, vec![], vec![1]));
    assert!(!m.scheduled.as_ref().unwrap().updates[1].accepted);
    assert_eq!(m.scheduled.as_ref().unwrap().revision, 2);
    render(&mut m, 0.25);
    assert!(m.active_ramps()[ControlTarget::TempoPercent { deck: 0 }.index()]);
    m.cancel_batch(900, false);
    assert!(m.active_ramps()[ControlTarget::CueGain.index()]);
    for target in [
        ControlTarget::TempoPercent { deck: 0 },
        ControlTarget::ChannelFader { deck: 0 },
        ControlTarget::EqLow { deck: 0 },
        ControlTarget::EqHigh { deck: 0 },
    ] {
        assert!(!m.active_ramps()[target.index()]);
    }
}

#[test]
fn explicit_parallel_patch_ramps_and_completion_publish_without_callback_allocations() {
    let mut m = mixer();
    arm_commands(&mut m);
    let mut commands = parallel_ramps(1, 14, QueueTiming::Now);
    commands.extend(parallel_ramps(
        5,
        15,
        QueueTiming::AfterAction {
            action_id: 1,
            seconds: None,
            clock: None,
            beats: None,
            quantize: None,
        },
    ));
    let patch =
        compile_command_patch(900, command_patch(1, 1, commands, vec![]), &snapshot(&m)).unwrap();
    let atoms = SnapshotAtoms::new();
    let mut retired = None;
    let counts = crate::realtime_audit::allocation_counts(|| {
        retired = Some(m.patch_batch(patch));
        for _ in 0..1500 {
            m.frame();
            if m.rendered_frames.is_multiple_of(64) {
                atoms.publish(&m, 2, 64, [0.; 2], [0.; 2]);
            }
        }
    });
    assert_eq!(counts, [0; 3]);
    assert!(m.scheduled.as_ref().unwrap().receipts[..8]
        .iter()
        .all(|r| r.state == 2));
    drop(retired);
}

#[test]
fn explicit_parallel_beat_dependency_uses_last_completion_clock_and_launch_snaps_all() {
    let mut m = mixer();
    arm_commands(&mut m);
    let mut commands = parallel_ramps(1, 16, QueueTiming::Now);
    commands.extend(parallel_ramps(
        5,
        17,
        QueueTiming::AfterAction {
            action_id: 1,
            seconds: None,
            clock: Some(0),
            beats: Some(1.),
            quantize: None,
        },
    ));
    apply_commands(&mut m, command_patch(1, 1, commands, vec![]));
    render(&mut m, 2.);
    let b = m.scheduled.as_ref().unwrap();
    assert!(b.receipts[..8].iter().all(|r| r.state == 2));
    let last = b.receipts[..4]
        .iter()
        .max_by_key(|r| r.completed_frame)
        .unwrap();
    let expected_seconds = last.completed_clock_seconds[0] + 0.5;
    let group_frame = b.receipts[4].frame;
    assert!(b.receipts[4..8]
        .iter()
        .all(|r| r.frame == group_frame && (r.clock_seconds - expected_seconds).abs() < 0.002));

    let mut m = mixer();
    arm_commands(&mut m);
    let mut commands = parallel_ramps(
        1,
        18,
        QueueTiming::AtSeconds {
            clock: 0,
            seconds: 1.25,
        },
    );
    // The incoming launch's exact beat boundary is shared with every control.
    commands[1].command = EngineCommand::Play { deck: 1 };
    apply_commands(&mut m, command_patch(1, 1, commands, vec![]));
    render(&mut m, 2.);
    let b = m.scheduled.as_ref().unwrap();
    assert!(b.receipts[..4]
        .iter()
        .all(|r| r.state == 2 && r.frame == 1500));
    assert!(m.decks[1].playing);
}

fn replacement_patch(when: QueueTiming) -> CommandQueuePatch {
    let mut patch = command_patch(2, 2, parallel_ramps(9, 22, when), vec![1, 2, 3, 4]);
    patch.rebind_dependencies = (1..=4).map(|old| (old, 9)).collect();
    patch
}
fn arm_parallel_with_pending_group_dependency(m: &mut Mixer, source_when: QueueTiming) {
    arm_commands(m);
    let mut commands = parallel_ramps(1, 20, source_when);
    commands.extend(parallel_ramps(
        5,
        21,
        QueueTiming::AfterAction {
            // Any child can serve as the public group's all-complete anchor.
            action_id: 2,
            seconds: Some(0.1),
            clock: None,
            beats: None,
            quantize: None,
        },
    ));
    apply_commands(m, command_patch(1, 1, commands, vec![]));
}
#[test]
fn explicit_parallel_replacement_retargets_pending_group_without_ids_or_callback_allocations() {
    let mut m = mixer();
    arm_parallel_with_pending_group_dependency(&mut m, QueueTiming::AfterSeconds { seconds: 10. });
    let patch = compile_command_patch(
        900,
        replacement_patch(QueueTiming::AfterSeconds { seconds: 0.1 }),
        &snapshot(&m),
    )
    .unwrap();
    let atoms = SnapshotAtoms::new();
    let mut retired = None;
    let counts = crate::realtime_audit::allocation_counts(|| {
        retired = Some(m.patch_batch(patch));
        for _ in 0..2000 {
            m.frame();
            if m.rendered_frames.is_multiple_of(64) {
                atoms.publish(&m, 2, 64, [0.; 2], [0.; 2]);
            }
        }
    });
    assert_eq!(counts, [0; 3]);
    let b = m.scheduled.as_ref().unwrap();
    assert!(b.updates[1].accepted);
    assert!(b.receipts[..4].iter().all(|r| r.state == 3));
    assert!(b.receipts[4..12].iter().all(|r| r.state == 2));
    assert_eq!(b.receipts[4].action_id, 5);
    assert_eq!(b.steps[4].as_ref().unwrap().dependency_id, 9);
    let last = b.receipts[8..12]
        .iter()
        .map(|r| r.completed_frame)
        .max()
        .unwrap();
    assert!(b.receipts[4..8].iter().all(|r| r.frame == last + 100));
    drop(retired);
}
#[test]
fn explicit_parallel_replacement_rejects_cycle_and_incomplete_anchor_mapping_atomically() {
    for cycle in [false, true] {
        let mut m = mixer();
        arm_parallel_with_pending_group_dependency(
            &mut m,
            QueueTiming::AfterSeconds { seconds: 10. },
        );
        let mut patch = replacement_patch(if cycle {
            QueueTiming::AfterAction {
                action_id: 5,
                seconds: None,
                clock: None,
                beats: None,
                quantize: None,
            }
        } else {
            QueueTiming::AfterSeconds { seconds: 1. }
        });
        if !cycle {
            patch.rebind_dependencies = vec![(1, 9)];
        }
        apply_commands(&mut m, patch);
        let b = m.scheduled.as_ref().unwrap();
        assert!(!b.updates[1].accepted);
        assert_eq!(b.revision, 2);
        assert_eq!(b.len, 8);
        assert!(b.receipts[..8].iter().all(|r| r.state == 0));
        assert_eq!(b.steps[4].as_ref().unwrap().dependency_id, 2);
    }
}
#[test]
fn explicit_parallel_replacement_race_cannot_retarget_started_source() {
    let mut m = mixer();
    arm_parallel_with_pending_group_dependency(&mut m, QueueTiming::AfterSeconds { seconds: 0.2 });
    let patch = compile_command_patch(
        900,
        replacement_patch(QueueTiming::AfterSeconds { seconds: 1. }),
        &snapshot(&m),
    )
    .unwrap();
    render(&mut m, 0.25);
    assert!(m.scheduled.as_ref().unwrap().receipts[..4]
        .iter()
        .any(|r| r.state == 1));
    drop(m.patch_batch(patch));
    let b = m.scheduled.as_ref().unwrap();
    assert!(!b.updates[1].accepted);
    assert_eq!(b.revision, 2);
    assert_eq!(b.len, 8);
    assert_eq!(b.steps[4].as_ref().unwrap().dependency_id, 2);
    render(&mut m, 1.);
    assert!(m.scheduled.as_ref().unwrap().receipts[..8]
        .iter()
        .all(|r| r.state == 2));
}

#[test]
fn generic_independent_readiness_accepts_lower_ids_but_never_reuses_lifetime_ids() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                256,
                EngineCommand::SetCueMix { mix: 0.2 },
                QueueTiming::Now,
            )],
            vec![],
        ),
    );
    render(&mut m, 0.1);
    // An earlier registered group becomes ready after a later independent item.
    apply_commands(
        &mut m,
        command_patch(2, 2, parallel_ramps(192, 192, QueueTiming::Now), vec![]),
    );
    render(&mut m, 0.5);
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.len, 5);
    assert!(b.receipts[..5].iter().all(|receipt| receipt.state == 2));
    assert_eq!(b.receipts[0].action_id, 256);
    assert_eq!(b.receipts[1].action_id, 192);
    let duplicate = command_patch(
        3,
        3,
        vec![queued(
            192,
            EngineCommand::SetCueMix { mix: 0.4 },
            QueueTiming::Now,
        )],
        vec![],
    );
    assert!(compile_command_patch(900, duplicate, &snapshot(&m))
        .unwrap_err()
        .contains("unique"));
    let duplicate_in_patch = command_patch(
        3,
        3,
        vec![
            queued(100, EngineCommand::SetCueMix { mix: 0.4 }, QueueTiming::Now),
            queued(
                100,
                EngineCommand::SetCueGain { gain: 0.4 },
                QueueTiming::Now,
            ),
        ],
        vec![],
    );
    assert!(compile_command_patch(900, duplicate_in_patch, &snapshot(&m)).is_err());
    apply_commands(
        &mut m,
        command_patch(
            3,
            3,
            vec![queued(
                64,
                EngineCommand::SetCueGain { gain: 0.3 },
                QueueTiming::AfterSeconds { seconds: 2. },
            )],
            vec![],
        ),
    );
    apply_commands(&mut m, command_patch(4, 4, vec![], vec![64]));
    assert_eq!(m.scheduled.as_ref().unwrap().receipts[5].state, 3);
    assert!(compile_command_patch(
        900,
        command_patch(
            5,
            5,
            vec![queued(
                64,
                EngineCommand::SetCueGain { gain: 0.5 },
                QueueTiming::Now
            )],
            vec![]
        ),
        &snapshot(&m)
    )
    .is_err());
}

#[test]
fn generic_zero_beat_dependency_registered_after_completion_runs_next_frame() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                256,
                EngineCommand::SetCueMix { mix: 0.2 },
                QueueTiming::Now,
            )],
            vec![],
        ),
    );
    render(&mut m, 0.1);
    let completion = m.scheduled.as_ref().unwrap().receipts[0].completed_frame;
    let before = m.rendered_frames;
    assert!(before > completion + 20);
    let when = QueueTiming::AfterAction {
        action_id: 256,
        seconds: None,
        clock: Some(0),
        beats: Some(0.),
        quantize: None,
    };
    apply_commands(
        &mut m,
        command_patch(2, 2, parallel_ramps(192, 192, when), vec![]),
    );
    m.frame();
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.reason, BatchReason::None);
    assert!(b.receipts[1..5]
        .iter()
        .all(|receipt| receipt.state == 1 && receipt.frame == before));
    render(&mut m, 0.5);
    assert!(m.scheduled.as_ref().unwrap().receipts[..5]
        .iter()
        .all(|receipt| receipt.state == 2));
}

#[test]
fn generic_zero_offset_readiness_does_not_relax_positive_or_absolute_deadlines() {
    let mut m = mixer();
    arm_commands(&mut m);
    let compiled = compile_command_patch(
        900,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::SetCueGain { gain: 0.4 },
                QueueTiming::AfterBeats {
                    clock: 0,
                    beats: 0.,
                    quantize: None,
                },
            )],
            vec![],
        ),
        &snapshot(&m),
    )
    .unwrap();
    render(&mut m, 0.03);
    let before = m.rendered_frames;
    drop(m.patch_batch(compiled));
    m.frame();
    assert_eq!(m.scheduled.as_ref().unwrap().receipts[0].frame, before);
    render(&mut m, 0.1);
    assert!(compile_command_patch(
        900,
        command_patch(
            2,
            2,
            vec![queued(
                2,
                EngineCommand::SetCueGain { gain: 0.6 },
                QueueTiming::AtSeconds {
                    clock: 0,
                    seconds: 0.01
                }
            )],
            vec![]
        ),
        &snapshot(&m)
    )
    .is_err());
    render(&mut m, 1.);
    apply_commands(
        &mut m,
        command_patch(
            2,
            2,
            vec![queued(
                2,
                EngineCommand::SetCueGain { gain: 0.6 },
                QueueTiming::AfterAction {
                    action_id: 1,
                    seconds: None,
                    clock: Some(0),
                    beats: Some(1.),
                    quantize: None,
                },
            )],
            vec![],
        ),
    );
    m.frame();
    assert_eq!(
        m.scheduled.as_ref().unwrap().reason,
        BatchReason::DeadlineExpired
    );
    assert_eq!(m.cue_gain.target, 0.4);
}

#[test]
fn queue_timing_errors_explain_range_and_missing_clock() {
    let error = validate_queue_timing(QueueTiming::AfterBeats {
        clock: 0,
        beats: 259.,
        quantize: None,
    })
    .unwrap_err();
    assert!(error.contains("256") && error.contains("259"));
    let error = validate_queue_timing(QueueTiming::AfterAction {
        action_id: 1,
        seconds: None,
        clock: None,
        beats: Some(4.),
        quantize: None,
    })
    .unwrap_err();
    assert!(error.contains("requires a clock"));
    assert!(
        validate_queue_timing(QueueTiming::AfterSeconds { seconds: 3601. })
            .unwrap_err()
            .contains("3600")
    );
}

#[test]
fn parallel_absolute_bpm_rejects_same_deck_transport_but_allows_independent_mixer_controls() {
    let mut m = mixer();
    arm_commands(&mut m);
    let before = (
        m.decks[1].position,
        m.decks[1].tempo,
        m.decks[1].channel_fader.target,
    );
    for transport in [
        EngineCommand::Seek {
            deck: 1,
            seconds: 10.,
        },
        EngineCommand::Play { deck: 1 },
    ] {
        let actions = vec![
            QueuedCommand {
                action_id: 1,
                group_id: Some(50),
                when: QueueTiming::Now,
                command: transport,
            },
            QueuedCommand {
                action_id: 2,
                group_id: Some(50),
                when: QueueTiming::Now,
                command: EngineCommand::SetPlaybackBpm { deck: 1, bpm: 124. },
            },
        ];
        assert!(
            compile_command_patch(900, command_patch(1, 1, actions, vec![]), &snapshot(&m))
                .unwrap_err()
                .contains("conflicting")
        );
        assert_eq!(
            (
                m.decks[1].position,
                m.decks[1].tempo,
                m.decks[1].channel_fader.target
            ),
            before
        );
        assert_eq!(m.scheduled.as_ref().unwrap().len, 0);
    }
    let actions = vec![
        QueuedCommand {
            action_id: 1,
            group_id: Some(51),
            when: QueueTiming::Now,
            command: EngineCommand::SetPlaybackBpm { deck: 1, bpm: 124. },
        },
        QueuedCommand {
            action_id: 2,
            group_id: Some(51),
            when: QueueTiming::Now,
            command: ramp(1, ControlTarget::EqLow { deck: 1 }, 0.8, None, 0.2),
        },
        QueuedCommand {
            action_id: 3,
            group_id: Some(51),
            when: QueueTiming::Now,
            command: ramp(1, ControlTarget::ChannelFader { deck: 1 }, 0.6, None, 0.3),
        },
    ];
    apply_commands(&mut m, command_patch(1, 1, actions, vec![]));
    render(&mut m, 0.5);
    let b = m.scheduled.as_ref().unwrap();
    assert!(b.receipts[..3]
        .iter()
        .all(|r| r.state == 2 && r.frame == b.receipts[0].frame));
    assert_eq!(m.decks[1].channel_fader.target, 0.6);
    assert_eq!(m.decks[1].eq[0].target, 0.8);
}

#[test]
fn queued_seek_then_absolute_bpm_resolves_reachability_from_execution_position() {
    let mut m = mixer();
    let mut source = fixture(240.);
    let analysis = &mut Arc::get_mut(&mut source).unwrap().analysis;
    let mut time = 0.2;
    let beats: Vec<_> = (0..400)
        .map(|i| {
            let at = time;
            time += if i < 64 { 0.5 } else { 0.6 };
            at
        })
        .collect();
    analysis.bpm_source = Some("beat_this".into());
    analysis.beatgrid_source = Some("beat_this".into());
    analysis.neural_rhythm = Some(audio_analysis::NeuralRhythm {
        downbeats_seconds: beats.iter().step_by(4).copied().collect(),
        beats_seconds: beats,
        bpm: 120.,
        beat_confidence: 1.,
        downbeat_confidence: 1.,
        model: "fixture".into(),
        analysis_seconds: 0.,
        bar_evidence: vec![],
        beats_per_bar: Some(4),
    });
    m.load(1, source);
    m.command(EngineCommand::Seek {
        deck: 1,
        seconds: 60.2,
    });
    assert!((m.decks[1].local_grid_bpm().unwrap() - 100.).abs() < 1e-10);
    assert!(crate::playback_tempo_percent(m.decks[1].local_grid_bpm(), 220.).is_err());
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![
                queued(
                    1,
                    EngineCommand::Seek {
                        deck: 1,
                        seconds: 0.2,
                    },
                    QueueTiming::Now,
                ),
                queued(
                    2,
                    EngineCommand::SetPlaybackBpm { deck: 1, bpm: 220. },
                    QueueTiming::AfterAction {
                        action_id: 1,
                        seconds: None,
                        clock: None,
                        beats: None,
                        quantize: None,
                    },
                ),
            ],
            vec![],
        ),
    );
    render(&mut m, 0.1);
    let b = m.scheduled.as_ref().unwrap();
    assert!(b.receipts[..2].iter().all(|r| r.state == 2));
    assert!((m.decks[1].local_grid_bpm().unwrap() - 120.).abs() < 1e-10);
    assert_eq!(m.decks[1].target_playback_bpm(), Some(220.));
    assert!((m.decks[1].tempo - (220. / 120. - 1.) * 100.).abs() < 1e-10);
}

#[test]
fn queued_prerequisite_failure_identifies_exact_parallel_member_without_partial_controls() {
    let mut m = mixer();
    arm_commands(&mut m);
    let commands = vec![
        QueuedCommand {
            action_id: 1,
            group_id: Some(61),
            when: QueueTiming::Now,
            command: EngineCommand::SetChannelFader {
                deck: 1,
                value: 0.7,
            },
        },
        QueuedCommand {
            action_id: 2,
            group_id: Some(61),
            when: QueueTiming::Now,
            command: EngineCommand::SetSync {
                deck: 0,
                enabled: true,
            },
        },
    ];
    let patch =
        compile_command_patch(900, command_patch(1, 1, commands, vec![]), &snapshot(&m)).unwrap();
    let atoms = SnapshotAtoms::new();
    let mut retired = None;
    let counts = crate::realtime_audit::allocation_counts(|| {
        retired = Some(m.patch_batch(patch));
        m.frame();
        atoms.publish(&m, 2, 64, [0.; 2], [0.; 2]);
    });
    assert_eq!(counts, [0; 3]);
    let b = m.scheduled.as_ref().unwrap();
    assert_eq!(b.reason, BatchReason::PrerequisiteFailed);
    assert!(b.receipts[..2].iter().all(|r| r.state == 4
        && r.failure
            == Some(CommandFailure {
                cause: CommandFailureCause::SyncMasterSelf,
                deck: Some(0),
                action_id: 2
            })));
    assert_eq!(m.decks[1].channel_fader.target, 0.);
    let public = snapshot(&m);
    assert_eq!(
        public.scheduled_batch.events[0].failure,
        b.receipts[0].failure
    );
    let json = serde_json::to_value(public.scheduled_batch.events[0]).unwrap();
    assert_eq!(json["failure"]["cause"], "sync_master_self");
    assert_eq!(json["failure"]["action_id"], 2);
    drop(retired);
}

#[test]
fn queued_readiness_receipts_name_source_grid_clock_jog_loop_and_bpm_causes() {
    for (case, expected, deck) in [
        ("source", CommandFailureCause::SourceMissing, 1),
        ("clock", CommandFailureCause::ClockNotPlaying, 0),
        ("revision", CommandFailureCause::ClockSourceChanged, 0),
        ("grid", CommandFailureCause::GridMissing, 1),
        ("jog", CommandFailureCause::JogHeld, 0),
        ("loop", CommandFailureCause::LoopActive, 0),
        ("hotcue", CommandFailureCause::HotCueMissing, 1),
        ("bpm", CommandFailureCause::PlaybackBpmUnreachable, 1),
        ("sync_limits", CommandFailureCause::SyncRateOutsideLimits, 1),
    ] {
        let mut m = mixer();
        let mut command = EngineCommand::PlayQuantized {
            deck: 1,
            clock: 0,
            clock_revision: 0,
            at_seconds: Some(1.),
            quantize: 1.,
        };
        match case {
            "source" => {
                drop(m.unload(1));
            }
            "clock" => m.command(EngineCommand::Pause { deck: 0 }),
            "revision" => {
                if let EngineCommand::PlayQuantized { clock_revision, .. } = &mut command {
                    *clock_revision = 1;
                }
            }
            "grid" => m.decks[1].grid_valid = false,
            "jog" => m.decks[0].jog_touch = true,
            "loop" => m.decks[0].loop_region = Some((0., 4000.)),
            "hotcue" => command = EngineCommand::TriggerHotCue { deck: 1, index: 0 },
            "bpm" => command = EngineCommand::SetPlaybackBpm { deck: 1, bpm: 300. },
            "sync_limits" => {
                command = EngineCommand::SetSyncRateLimits {
                    deck: 1,
                    limits: Some(crate::SyncRateLimits {
                        min_rate: 1.01,
                        max_rate: 1.02,
                        max_slew_per_second: 0.01,
                    }),
                }
            }
            _ => unreachable!(),
        }
        arm_commands(&mut m);
        apply_commands(
            &mut m,
            command_patch(1, 1, vec![queued(1, command, QueueTiming::Now)], vec![]),
        );
        m.frame();
        let receipt = &m.scheduled.as_ref().unwrap().receipts[0];
        assert_eq!(
            receipt.failure,
            Some(CommandFailure {
                cause: expected,
                deck: Some(deck),
                action_id: 1
            }),
            "{case}"
        );
        assert_eq!(receipt.state, 4);
    }
}

#[test]
fn queued_offbeat_quantized_launch_resolves_before_trigger_and_completes_dependents() {
    for clock in 0..2 {
        for explicit in [false, true] {
            for tempo in [0., 100.] {
                let mut m = mixer();
                let incoming = 1 - clock;
                m.command(EngineCommand::Pause { deck: 0 });
                m.command(EngineCommand::SetBeatgrid {
                    deck: clock,
                    anchor_index: None,
                    offset_seconds: 0.238105,
                });
                m.command(EngineCommand::Seek {
                    deck: clock,
                    seconds: 10.,
                });
                m.command(EngineCommand::Seek {
                    deck: incoming,
                    seconds: 3.25,
                });
                m.command(EngineCommand::Play { deck: clock });
                m.command(EngineCommand::SetTempo {
                    deck: clock,
                    percent: tempo,
                });
                arm_commands(&mut m);
                let launch = if explicit {
                    EngineCommand::PlayQuantized {
                        deck: incoming,
                        clock,
                        clock_revision: m.load_revisions[clock],
                        at_seconds: None,
                        quantize: 1.,
                    }
                } else {
                    EngineCommand::Play { deck: incoming }
                };
                apply_commands(
                    &mut m,
                    command_patch(
                        1,
                        1,
                        vec![
                            queued(
                                1,
                                launch,
                                QueueTiming::AtSeconds {
                                    clock,
                                    seconds: 10.579,
                                },
                            ),
                            queued(
                                2,
                                EngineCommand::SetChannelFader {
                                    deck: incoming,
                                    value: 1.,
                                },
                                QueueTiming::AfterAction {
                                    action_id: 1,
                                    seconds: None,
                                    clock: None,
                                    beats: None,
                                    quantize: None,
                                },
                            ),
                        ],
                        vec![],
                    ),
                );
                let resolved = m.scheduled.as_ref().unwrap().receipts[0]
                    .resolved_clock_seconds
                    .unwrap();
                assert!(
                    (resolved - 10.738105).abs() < 1e-9,
                    "clock={clock}, explicit={explicit}: {resolved}"
                );
                render(&mut m, 1.);
                let b = m.scheduled.as_ref().unwrap();
                assert!(
                    b.receipts[..2].iter().all(|r| r.state == 2),
                    "clock={clock}, explicit={explicit}: {:?}",
                    &b.receipts[..2]
                );
                assert!(m.decks[incoming].playing);
                assert!(m.decks[clock].playing);
                assert!(b.receipts[0].completed_clock_seconds[clock] >= resolved);
                assert!(b.receipts[0].completed_clock_seconds[clock] - resolved <= 0.0041);
            }
        }
    }
}

#[test]
fn queued_quantized_launch_rejects_conflicting_clock_before_registration() {
    let mut m = mixer();
    arm_commands(&mut m);
    let command = EngineCommand::PlayQuantized {
        deck: 1,
        clock: 0,
        clock_revision: m.load_revisions[0],
        at_seconds: None,
        quantize: 1.,
    };
    let result = compile_command_patch(
        900,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                command,
                QueueTiming::AtSeconds {
                    clock: 1,
                    seconds: 1.,
                },
            )],
            vec![],
        ),
        &snapshot(&m),
    );
    assert!(result.err().unwrap().contains("clock must match"));
    assert_eq!(m.scheduled.as_ref().unwrap().len, 0);
}

#[test]
fn queued_explicit_launch_and_parallel_controls_share_resolved_dependency_boundary() {
    let mut m = mixer();
    arm_commands(&mut m);
    let mut commands = vec![queued(
        1,
        EngineCommand::SetEq {
            deck: 1,
            low: 0.,
            mid: 1.,
            high: 1.,
        },
        QueueTiming::Now,
    )];
    let mut group = parallel_ramps(
        5,
        72,
        QueueTiming::AfterAction {
            action_id: 1,
            seconds: None,
            clock: Some(0),
            beats: Some(2.5),
            quantize: None,
        },
    );
    group[1].command = EngineCommand::PlayQuantized {
        deck: 1,
        clock: 0,
        clock_revision: m.load_revisions[0],
        at_seconds: None,
        quantize: 1.,
    };
    commands.extend(group);
    apply_commands(&mut m, command_patch(1, 1, commands, vec![]));
    render(&mut m, 3.);
    let b = m.scheduled.as_ref().unwrap();
    assert!(
        b.receipts[..5].iter().all(|r| r.state == 2),
        "{:?}",
        &b.receipts[..5]
    );
    let frame = b.receipts[2].frame;
    assert!(b.receipts[1..5].iter().all(|r| r.frame == frame));
    assert_eq!(frame, 1500);
}

#[test]
fn resolved_source_trigger_headroom_and_actual_ramp_end_are_observable_before_and_after_start() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![queued(
                1,
                EngineCommand::Ramp {
                    target: ControlTarget::ChannelFader { deck: 1 },
                    value: 1.,
                    seconds: 5.,
                    clock: Some(0),
                    beats: None,
                    quantize: None,
                },
                QueueTiming::AtSeconds {
                    clock: 0,
                    seconds: 239.,
                },
            )],
            vec![],
        ),
    );
    let pending = snapshot(&m).scheduled_batch.events[0];
    assert_eq!(pending.resolved_clock, Some(0));
    assert_eq!(pending.resolved_clock_seconds, Some(239.));
    assert_eq!(pending.clock_source_headroom_seconds, Some(1.));
    assert_eq!(pending.ramp_end_frame, None);
    render(&mut m, 239.001);
    let running = snapshot(&m).scheduled_batch.events[0];
    assert_eq!(running.state, 1);
    assert_eq!(running.ramp_end_frame, Some(244000));
    assert_eq!(running.resolved_clock_seconds, Some(239.));
    render(&mut m, 5.2);
    let done = snapshot(&m).scheduled_batch.events[0];
    assert_eq!(done.state, 2);
    assert_eq!(done.ramp_end_frame, Some(244000));
    assert_eq!(done.clock_source_headroom_seconds, Some(1.));
    assert!(!m.decks[0].playing);
    assert_eq!(m.decks[1].channel_fader.target, 1.);
}

#[test]
fn dependency_resolution_reports_actual_source_target_before_the_dependent_dispatches() {
    let mut m = mixer();
    arm_commands(&mut m);
    apply_commands(
        &mut m,
        command_patch(
            1,
            1,
            vec![
                queued(
                    1,
                    EngineCommand::Play { deck: 1 },
                    QueueTiming::AtEvent { clock: 0, event: 4 },
                ),
                queued(
                    2,
                    EngineCommand::Ramp {
                        target: ControlTarget::ChannelFader { deck: 1 },
                        value: 1.,
                        seconds: 0.3,
                        clock: None,
                        beats: None,
                        quantize: None,
                    },
                    QueueTiming::AfterAction {
                        action_id: 1,
                        seconds: None,
                        clock: Some(0),
                        beats: Some(8.),
                        quantize: None,
                    },
                ),
            ],
            vec![],
        ),
    );
    assert_eq!(
        snapshot(&m).scheduled_batch.events[1].resolved_clock_seconds,
        None
    );
    render(&mut m, 2.01);
    let after_play = snapshot(&m);
    let pending = after_play.scheduled_batch.events[1];
    let expected = after_play.scheduled_batch.events[0].completed_clock_seconds[0] + 4.;
    assert_eq!(pending.state, 0);
    assert_eq!(pending.resolved_clock, Some(0));
    assert!((pending.resolved_clock_seconds.unwrap() - expected).abs() < 1e-10);
    assert!((pending.clock_source_headroom_seconds.unwrap() - (240. - expected)).abs() < 1e-10);
    assert_eq!(pending.ramp_end_frame, None);
    render(&mut m, 4.5);
    let done = snapshot(&m).scheduled_batch.events[1];
    assert_eq!(done.state, 2);
    assert!((done.resolved_clock_seconds.unwrap() - expected).abs() < 1e-10);
    assert!((done.clock_seconds - expected).abs() < 0.002);
    assert_eq!(done.ramp_end_frame, Some(done.frame + 300));
}
