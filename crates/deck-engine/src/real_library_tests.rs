//! Opt-in integration checks against unchanged local music and actual planner recipes.
//! No sound device is opened and no mixed audio or library metadata is written.
use crate::{decode, dsp::Mixer, scheduled, snapshot::SnapshotAtoms, *};
use std::{
    path::Path,
    sync::{atomic::Ordering, Arc},
};

#[derive(serde::Deserialize)]
struct Manifest {
    snapshot: EngineSnapshot,
    schedule: BatchSchedule,
    commands: Vec<ScheduledCommand>,
    #[serde(default)]
    stall_incoming_after_launch: bool,
}

fn snapshot(mixer: &Mixer) -> EngineSnapshot {
    let atoms = SnapshotAtoms::new();
    atoms.running.store(true, Ordering::Release);
    atoms
        .stream_revision
        .store(mixer.engine_epoch, Ordering::Release);
    atoms.publish(mixer, 2, 512, [0.; 2], [0.; 2]);
    atoms.read(
        mixer
            .decks
            .each_ref()
            .map(|d| d.track.as_ref().map(|t| t.analysis.clone())),
    )
}

fn restore_mixer(state: &EngineSnapshot) -> Mixer {
    let mut mixer = Mixer::new();
    mixer.sample_rate = state.sample_rate;
    for (index, expected) in state.decks.iter().enumerate() {
        let analysis = expected
            .track
            .as_ref()
            .expect("Real source metadata is required");
        assert!(
            !analysis.path.starts_with("spotify:"),
            "This check uses local files only"
        );
        assert!(
            !expected.sync_enabled,
            "Manifest starts before the native follower is enabled"
        );
        let mut decoded =
            decode::decode_file_playable(Path::new(&analysis.path), Some(analysis.clone()))
                .unwrap();
        let data = Arc::get_mut(&mut decoded).expect("Decoded source is exclusively owned");
        assert!(
            (data.samples.len() as f64 / data.sample_rate as f64 - analysis.duration_seconds).abs()
                < 0.2,
            "Source length changed: {}",
            analysis.path
        );
        if let (Some(actual), Some(expected)) = (
            &data.analysis.source_fingerprint,
            &analysis.source_fingerprint,
        ) {
            assert_eq!(actual, expected, "Source content changed");
        }
        // Use the exact captured effective analysis; decoded samples remain unchanged.
        let measurements = data.analysis.audio_measurements.clone();
        data.analysis = analysis.clone();
        data.analysis.audio_measurements = measurements;
        data.analysis_pending = false;
        mixer.load(index, decoded);
        mixer.command(EngineCommand::SetNormalization {
            deck: index,
            enabled: expected.normalization_enabled,
        });
        let deck = &mut mixer.decks[index];
        deck.position = expected.position_seconds * deck.track.as_ref().unwrap().sample_rate as f64;
        deck.position_revision = expected.position_revision;
        deck.playing = expected.playing;
        deck.tempo = (expected.actual_playback_rate - 1.) * 100.;
        deck.pitch = expected.pitch_semitones;
        deck.keylock = expected.keylock;
        deck.bpm = expected.bpm;
        deck.grid_offset = expected.beatgrid_offset_seconds;
        deck.grid_valid = expected.beatgrid_valid;
        deck.grid_uniform = expected.beatgrid_uniform;
        deck.grid_manual = expected.beatgrid_manual;
        deck.grid_downbeat = expected.beatgrid_downbeat;
        deck.grid_anchor_index = expected.beatgrid_anchor_index.unwrap_or(0);
        deck.grid_scale = if expected.beatgrid_uniform {
            1.
        } else {
            analysis
                .neural_rhythm
                .as_ref()
                .map_or(1., |g| g.bpm / expected.bpm.unwrap_or(g.bpm))
        };
        deck.gain.target = expected.gain;
        deck.channel_fader.target = expected.channel_fader;
        deck.eq[0].target = expected.eq_low;
        deck.eq[1].target = expected.eq_mid;
        deck.eq[2].target = expected.eq_high;
        deck.filter.target = expected.filter;
        deck.echo.target = expected.echo;
        deck.reverb.target = expected.reverb;
    }
    mixer.master_deck = state.decks.iter().position(|d| d.is_master).unwrap_or(0);
    mixer.master_gain.target = state.master_gain;
    mixer.crossfader.target = state.crossfader;
    mixer.engine_epoch = state.engine_epoch;
    mixer.load_revisions = state.load_revisions;
    mixer.grid_revisions = state.decks.each_ref().map(|d| d.grid_revision);
    mixer.control_revision = state.control_revision;
    mixer.deck_control_revisions = state.deck_control_revisions;
    mixer.target_control_revisions = state.target_control_revisions;
    mixer.rendered_frames = state.rendered_frames;
    mixer
}

#[test]
#[ignore = "Requires explicit real-song planner manifests; never opens audio hardware"]
fn render_actual_local_library_planner_transitions() {
    let paths = std::env::var_os("PULSEDECK_REAL_MIX_MANIFESTS")
        .expect("Set PULSEDECK_REAL_MIX_MANIFESTS to native planner manifest paths");
    let mut checked = 0;
    for path in std::env::split_paths(&paths) {
        let manifest: Manifest = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let mut mixer = restore_mixer(&manifest.snapshot);
        let clock = manifest.schedule.clock;
        let carrier = manifest
            .schedule
            .completion_carrier
            .expect("Recipe must identify incoming carrier");
        let launch_event = manifest.commands.iter().map(|c| c.at_event).min().unwrap();
        let launch = scheduled::event_seconds(&manifest.snapshot, clock, launch_event).unwrap();
        let rate = mixer.decks[clock].rate();
        let original = mixer.decks[clock].seconds();
        let start = original.max(launch - 4. * rate);
        mixer.decks[clock].position =
            start * mixer.decks[clock].track.as_ref().unwrap().sample_rate as f64;
        let state = snapshot(&mixer);
        let clear =
            scheduled::event_seconds(&state, clock, manifest.schedule.completion_event).unwrap();
        let seconds = (clear - start) / rate + 3.;
        assert!(
            (1. ..=180.).contains(&seconds),
            "Render window must be bounded"
        );
        let batch = scheduled::compile(1, &manifest.schedule, manifest.commands, &state).unwrap();
        assert!(mixer.arm_batch(batch).is_none());
        let frames = (seconds * mixer.sample_rate as f64).ceil() as usize;
        let window = (mixer.sample_rate / 10).max(1) as usize;
        let mut power = 0.;
        let mut min_rms = f64::INFINITY;
        let mut peak = 0_f32;
        let mut silence_windows = 0;
        let mut max_speed_change_percent = [0_f64; 2];
        let mut max_phase_error = 0_f64;
        let mut max_tempo_difference = 0_f64;
        let mut overlap_samples = 0usize;
        let mut window_tempo_sum = 0_f64;
        let mut window_samples = 0usize;
        let mut max_mean_tempo_difference = 0_f64;
        let speed_limit = manifest.schedule.intent["max_stretch_percent"]
            .as_f64()
            .unwrap_or(2.);
        let mut stalled = false;
        let mut recovery_logged = false;
        for frame in 0..frames {
            if manifest.stall_incoming_after_launch && !stalled && mixer.decks[carrier].playing {
                // Model a failed/stalled incoming launch without a manual-control revision.
                mixer.decks[carrier].playing = false;
                stalled = true;
            }
            let (raw_master, _) = mixer.frame();
            if !recovery_logged
                && mixer
                    .scheduled
                    .as_ref()
                    .is_some_and(|b| b.state == scheduled::BatchLifecycle::Recovering)
            {
                recovery_logged = true;
                println!(
                    "REAL_RECOVERY {}",
                    serde_json::json!({"path":path,"completed":mixer.scheduled.as_ref().unwrap().completed,"decks":mixer.decks.each_ref().map(|d| serde_json::json!({"seconds":d.seconds(),"beat":d.grid_position(d.seconds()),"bpm":d.local_grid_bpm().map(|b|b*d.actual_rate()),"rate":d.actual_rate(),"limited":d.sync_rate_limited}))})
                );
            }
            assert!(
                raw_master.iter().all(|sample| sample.is_finite()),
                "Invalid mixer bus at frame {frame}"
            );
            // Match RenderState's actual master headroom and lookahead output path.
            let master = mixer.master_protection.frame(
                raw_master.map(|sample| sample * crate::protection::HEADROOM),
                mixer.sample_rate,
            );
            for (deck, maximum) in mixer.decks.iter().zip(&mut max_speed_change_percent) {
                *maximum = maximum.max((deck.actual_rate() - 1.).abs() * 100.);
                assert!(
                    *maximum <= speed_limit + 0.01,
                    "Recipe exceeded its natural-speed limit"
                );
            }
            if frame % 256 == 0
                && mixer
                    .decks
                    .iter()
                    .all(|d| d.playing && d.channel_fader.target > 0.05)
            {
                let a = &mixer.decks[clock];
                let b = &mixer.decks[carrier];
                let phase = (a.grid_position(a.seconds()) - b.grid_position(b.seconds()) + 0.5)
                    .rem_euclid(1.)
                    - 0.5;
                let signed_difference = a.local_grid_bpm().unwrap() * a.actual_rate()
                    - b.local_grid_bpm().unwrap() * b.actual_rate();
                let difference = signed_difference.abs();
                max_phase_error = max_phase_error.max(phase.abs());
                max_tempo_difference = max_tempo_difference.max(difference);
                overlap_samples += 1;
                window_tempo_sum += signed_difference;
                window_samples += 1;
                if window_samples * 256 >= 4 * mixer.sample_rate as usize {
                    max_mean_tempo_difference = max_mean_tempo_difference
                        .max((window_tempo_sum / window_samples as f64).abs());
                    window_samples = 0;
                    window_tempo_sum = 0.;
                }
            }
            assert!(
                master.iter().all(|sample| sample.is_finite()),
                "Invalid output at frame {frame}"
            );
            assert!(
                mixer.decks.iter().any(|deck| deck.playing
                    && !deck.analysis_pending
                    && deck.channel_fader.target > 0.001
                    && deck.gain.target > 0.001),
                "No contributing carrier at frame {frame}"
            );
            peak = peak.max(master[0].abs()).max(master[1].abs());
            power += (master[0] as f64).powi(2) + (master[1] as f64).powi(2);
            if (frame + 1) % window == 0 {
                let rms = (power / (2 * window) as f64).sqrt();
                if frame >= window * 5 {
                    min_rms = min_rms.min(rms);
                    silence_windows += usize::from(rms < 1e-7);
                }
                power = 0.;
            }
        }
        let batch = mixer.scheduled.as_ref().unwrap();
        if manifest.stall_incoming_after_launch {
            assert!(stalled);
            assert_eq!(batch.state, scheduled::BatchLifecycle::Recovered);
            assert!(mixer.decks[clock].playing);
            assert!(mixer.decks[clock].channel_fader.target >= 0.999);
            assert!(mixer.decks[carrier].channel_fader.target <= 0.001);
            assert!(
                !mixer.decks[carrier].playing,
                "Recovered incoming deck must be available for preparation"
            );
            assert_eq!(mixer.master_deck, clock);
        } else {
            assert_eq!(
                batch.state,
                scheduled::BatchLifecycle::Completed,
                "{}: {:?}",
                path.display(),
                batch.reason
            );
            assert!(
                batch
                    .receipts
                    .iter()
                    .take(batch.len)
                    .all(|receipt| receipt.state == 2),
                "Every recipe action must execute"
            );
            assert!(mixer.decks[carrier].playing);
            assert!(mixer.decks[carrier].channel_fader.target >= 0.999);
            assert!(mixer.decks[carrier]
                .eq
                .iter()
                .all(|band| (band.target - 1.).abs() < 0.002));
            assert!(mixer.decks[carrier].filter.target.abs() < 0.002);
            assert_eq!(mixer.master_deck, carrier);
            assert!(!mixer.decks[carrier].sync_enabled);
            assert!(overlap_samples > 0);
            assert!(
                max_phase_error <= 0.12,
                "Audible overlap phase drifted {max_phase_error} beats"
            );
            if window_samples > 0 {
                max_mean_tempo_difference =
                    max_mean_tempo_difference.max((window_tempo_sum / window_samples as f64).abs());
            }
            assert!(
                max_mean_tempo_difference <= 0.6,
                "Audible overlap mean tempo difference {max_mean_tempo_difference} BPM"
            );
            assert!(!mixer.decks[clock].playing);
            assert!(mixer.decks[clock].channel_fader.target <= 0.001);
        }
        assert!(peak <= 1.0001, "Protected output exceeded full scale");
        assert_eq!(
            silence_windows, 0,
            "Unexpected digital silence in this real selected passage"
        );
        println!(
            "REAL_MIX {}",
            serde_json::json!({
                "manifest":path,"technique":manifest.schedule.intent["technique"],
                "outgoing":manifest.snapshot.decks[clock].track.as_ref().unwrap().title,
                "incoming":manifest.snapshot.decks[carrier].track.as_ref().unwrap().title,
                "rendered_seconds":seconds,"sample_rate":mixer.sample_rate,
                "actions_total":batch.len,"actions_completed":batch.receipts.iter().take(batch.len).filter(|r|r.state==2).count(),"min_100ms_rms":min_rms,"peak":peak,
            "max_speed_change_percent":max_speed_change_percent,
                "overlap_samples":overlap_samples,"max_phase_error_beats":max_phase_error,"max_tempo_difference_bpm":max_tempo_difference,"max_mean_tempo_difference_bpm":max_mean_tempo_difference,
                "silent_windows":silence_windows,"state":batch.state,"fault_injected":stalled
            })
        );
        checked += 1;
    }
    assert!(
        checked >= 3,
        "Supply each of the three supported real-song recipes"
    );
}

#[test]
#[ignore = "Requires two actual local-song manifests; offline PCM measurements only"]
fn measure_actual_local_library_programmes() {
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(std::env::var("PULSEDECK_REAL_MEASURE_INPUT").unwrap()).unwrap(),
    )
    .unwrap();
    let mut results = Vec::new();
    for deck in &manifest.snapshot.decks {
        let a = deck.track.as_ref().unwrap();
        let mut decoded =
            decode::decode_file_playable(Path::new(&a.path), Some(a.clone())).unwrap();
        let m = decoded.analysis.audio_measurements.clone().unwrap();
        assert!(m.current(decoded.sample_rate, decoded.samples.len()));
        assert!(m.audible_start_frame.unwrap() < m.audible_end_frame.unwrap());
        let first = m.audible_start_frame.unwrap() as usize;
        let end = m.audible_end_frame.unwrap() as usize;
        assert!(decoded.samples[..first]
            .iter()
            .flatten()
            .all(|x| x.abs() <= 0.001));
        assert!(decoded.samples[end..]
            .iter()
            .flatten()
            .all(|x| x.abs() <= 0.001));
        let gain = 10_f32.powf(m.normalization_gain_db as f32 / 20.);
        for frame in &mut Arc::get_mut(&mut decoded).unwrap().samples {
            for x in frame {
                *x *= gain;
            }
        }
        let after = audio_analysis::measure_audio(&decoded.samples, decoded.sample_rate);
        assert!(
            (after.integrated_lufs.unwrap() - m.integrated_lufs.unwrap() - m.normalization_gain_db)
                .abs()
                < 0.05
        );
        assert!(after.true_peak_dbtp.unwrap() <= -0.99);
        assert!(after.integrated_lufs.unwrap() <= -17.99);
        results.push(
            serde_json::json!({"source":a.path,"title":a.title,"measured":m,"normalized":after}),
        );
    }
    if let Ok(path) = std::env::var("PULSEDECK_REAL_MEASURE_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&results).unwrap()).unwrap();
    }
    println!("{}", serde_json::to_string(&results).unwrap());
}
