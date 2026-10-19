//! Generated audio only: no hardware, Spotify session, or copyrighted capture.
use super::*;
use std::f64::consts::TAU;
fn tone(frequency: f64, rate: u32, seconds: f64) -> Arc<TrackData> {
    let mut t = decode::demo_track(0);
    let d = Arc::get_mut(&mut t).unwrap();
    d.samples = (0..(seconds * rate as f64) as usize)
        .map(|i| {
            let s = (TAU * frequency * i as f64 / rate as f64).sin() as f32;
            [s, -s]
        })
        .collect();
    d.sample_rate = rate;
    d.analysis.sample_rate = rate;
    d.analysis.duration_seconds = seconds;
    t
}
#[test]
fn bandlimited_src_preserves_passband_and_rejects_downsampling_aliases() {
    for f in [997., 10000., 18000.] {
        let t = tone(f, 44100, 1.5);
        let mut power = 0.;
        for i in 4800..52800 {
            let s = processor::sample(
                &t.samples,
                i as f64 * 44100. / 48000.,
                44100. / 48000.,
                None,
            );
            power += f64::from(s[0]).powi(2);
            assert!((s[0] + s[1]).abs() < 1e-6);
        }
        let db = 20. * ((power / 48000.).sqrt() / 0.5_f64.sqrt()).log10();
        assert!(db.abs() < 0.1, "{f} Hz passband {db} dB");
    }
    let t = tone(18000., 48000, 1.5);
    let mut power = 0.;
    for i in 2400..26400 {
        let s = processor::sample(&t.samples, i as f64 * 2., 2., None);
        power += f64::from(s[0]).powi(2);
    }
    let db = 20. * ((power / 24000.).sqrt() / 0.5_f64.sqrt()).log10();
    assert!(db < -80., "downsampling alias rejection {db} dB");
}
#[test]
fn continuous_keylock_tracks_changes_without_cache_churn_or_varispeed() {
    let t = tone(440., 48000, 4.);
    let mut p = processor::Processor::new();
    let mut cursor = 0.;
    let mut output = Vec::new();
    for i in 0..96000 {
        let rate = 1.08 + 0.02 * (TAU * i as f64 / 48000.).sin();
        let s = p.render(&t.samples, cursor, 48000, 48000, rate, 0., true, 1, None);
        assert!(s.iter().all(|v| v.is_finite()));
        assert!((s[0] + s[1]).abs() < 0.001, "stereo phase");
        output.push(s[0]);
        cursor += rate;
    }
    let crossings = output[12000..84000]
        .windows(2)
        .filter(|w| w[0] <= 0. && w[1] > 0.)
        .count();
    let frequency = crossings as f64 / 1.5;
    assert!(
        (frequency - 440.).abs() < 3.,
        "keylocked frequency {frequency}"
    );
    assert_eq!(
        p.resets, 1,
        "changing sync rate must not reset/rebuild whole source"
    );
    assert_eq!(p.blocks, 1500);
}
#[test]
fn independent_pitch_holds_source_duration_and_seek_resets_once() {
    let t = tone(440., 48000, 4.);
    let mut p = processor::Processor::new();
    let mut out = Vec::new();
    for i in 0..96000 {
        out.push(p.render(&t.samples, i as f64, 48000, 48000, 1., 3., true, 1, None)[0]);
    }
    let crossings = out[12000..84000]
        .windows(2)
        .filter(|w| w[0] <= 0. && w[1] > 0.)
        .count();
    assert!((crossings as f64 / 1.5 - 523.251).abs() < 3.);
    p.render(&t.samples, 100., 48000, 48000, 1., 3., true, 2, None);
    assert_eq!(p.resets, 2);
}
#[test]
fn protection_is_linked_transparent_below_ceiling_and_bounded_on_impulses() {
    let mut p = protection::Protection::new();
    let mut out = Vec::new();
    for i in 0..24000 {
        let s = (TAU * 997. * i as f64 / 48000.).sin() as f32 * 0.3;
        out.push(p.frame([s, -s], 48000));
    }
    for i in 0..23000 {
        let s = (TAU * 997. * i as f64 / 48000.).sin() as f32 * 0.3;
        assert!((out[i + 128][0] - s).abs() < 1e-6);
        assert_eq!(out[i + 128][0], -out[i + 128][1]);
    }
    let mut p = protection::Protection::new();
    for i in 0..48000 {
        let s = if i % 1703 == 0 {
            10.
        } else {
            1.4 * (TAU * 997. * i as f64 / 48000.).sin() as f32
        };
        let o = p.frame([s, -s * 0.25], 48000);
        assert!(o[0].abs() <= 0.891251);
        assert!((o[0] * -0.25 - o[1]).abs() < 1e-6);
    }
    assert!(p.overloaded_frames > 0);
    assert!(p.reduction_db() > 0.);
    assert!(p
        .frame([f32::NAN, f32::INFINITY], 48000)
        .iter()
        .all(|v| v.is_finite()));
    assert_eq!(p.invalid_frames, 1);
}
#[test]
fn device_output_receives_protected_audio_and_telemetry() {
    let (_producer, consumer) = RingBuffer::new(128);
    let (retired, _reclaim) = RingBuffer::new(256);
    let atoms = SnapshotAtoms::new();
    let mut state = RenderState {
        mixer: Mixer::new(),
        commands: consumer,
        retired,
        recording: None,
        routing: AudioRouting::default(),
        channels: 2,
    };
    let t = tone(997., 48000, 2.);
    state.mixer.load(0, t.clone());
    state.mixer.load(1, t);
    state.mixer.decks[0].playing = true;
    state.mixer.decks[1].playing = true;
    state.mixer.master_gain.target = 2.;
    state.mixer.decks[0].gain.target = 2.;
    state.mixer.decks[1].gain.target = 2.;
    let mut output = [0f32; 1024];
    for _ in 0..80 {
        state.render(&mut output, &atoms);
        assert!(output.iter().all(|v| v.is_finite() && v.abs() <= 0.891251));
    }
    let s = atoms.read([None, None]);
    assert!(s.audio_quality.master_input_peak[0] > 1.);
    assert!(s.master_peak[0] <= 0.891251);
    assert!(s.audio_quality.master_gain_reduction_db > 0.);
    assert_eq!(s.audio_quality.output_latency_frames, 128);
    assert_eq!(s.audio_quality.callback_count, 80);
}
/// Prints reproducible bounded native block timing. Explicit opt-in avoids
/// machine-load-dependent unit assertions; the report records actual values.
#[test]
#[ignore = "timing benchmark; run explicitly on an otherwise idle machine"]
fn benchmark_two_continuous_keylocked_decks() {
    for keylock in [false, true] {
        let t = tone(440., 44100, 40.);
        let (_producer, consumer) = RingBuffer::new(128);
        let (retired, _reclaim) = RingBuffer::new(256);
        let atoms = SnapshotAtoms::new();
        let mut state = RenderState {
            mixer: Mixer::new(),
            commands: consumer,
            retired,
            recording: None,
            routing: AudioRouting::default(),
            channels: 2,
        };
        state.mixer.load(0, t.clone());
        state.mixer.load(1, t);
        for d in &mut state.mixer.decks {
            d.playing = true;
            d.keylock = keylock;
            d.tempo = 6.;
        }
        let mut timings = Vec::with_capacity(6000);
        let mut output = [0f32; 256];
        for _ in 0..6000 {
            let start = std::time::Instant::now();
            state.render(&mut output, &atoms);
            timings.push(start.elapsed().as_nanos() as u64);
        }
        timings.sort_unstable();
        println!(
            "{}",
            serde_json::json!({"keylock":keylock,"frames_per_callback":128,"callback_budget_ns":2_666_666,"p50_ns":timings[3000],"p99_ns":timings[5940],"p99_9_ns":timings[5994],"max_ns":timings[5999],"diagnostics":atoms.read([None,None]).audio_quality})
        );
    }
}

#[test]
fn processor_latency_compensation_keeps_impulse_and_seek_on_source_clock() {
    let mut t = tone(440., 48000, 1.);
    let d = Arc::get_mut(&mut t).unwrap();
    d.samples.fill([0.; 2]);
    d.samples[12000] = [0.5; 2];
    let mut p = processor::Processor::new();
    let mut peak = (0usize, 0f32);
    for i in 0..24000 {
        let s = p.render(&t.samples, i as f64, 48000, 48000, 1., 0., true, 1, None);
        if s[0].abs() > peak.1 {
            peak = (i, s[0].abs());
        }
    }
    assert!(
        peak.0.abs_diff(12000) <= 2,
        "uncompensated DSP delay: peak at {}",
        peak.0
    );
    let mut peak = (0usize, 0f32);
    for i in 0..12000 {
        let s = p.render(
            &t.samples,
            (i + 6000) as f64,
            48000,
            48000,
            1.,
            0.,
            true,
            2,
            None,
        );
        if s[0].abs() > peak.1 {
            peak = (i, s[0].abs());
        }
    }
    assert!(
        peak.0.abs_diff(6000) <= 2,
        "seek source origin: peak at {}",
        peak.0
    );
    assert_eq!(p.resets, 2);
}

#[test]
fn fractional_src_has_low_phase_quantization_error() {
    let t = tone(18000., 44100, 1.5);
    let mut error = 0.;
    let mut power = 0.;
    for i in 4800..52800 {
        let pos = i as f64 * 44100. / 48000.;
        let actual = processor::sample(&t.samples, pos, 44100. / 48000., None)[0] as f64;
        let reference = (TAU * 18000. * pos / 44100.).sin();
        error += (actual - reference).powi(2);
        power += reference * reference;
    }
    let error_db = 10. * (error / power).log10();
    assert!(error_db < -85., "fractional SRC error {error_db} dB");
}

#[test]
fn stretched_impulses_keep_bounded_musical_onset_error() {
    let mut t = tone(440., 48000, 1.);
    let d = Arc::get_mut(&mut t).unwrap();
    d.samples.fill([0.; 2]);
    d.samples[12000] = [0.5; 2];
    for rate in [0.94, 1.06, 1.12] {
        let mut p = processor::Processor::new();
        let mut peak = (0usize, 0f32);
        for i in 0..20000 {
            let s = p.render(
                &t.samples,
                i as f64 * rate,
                48000,
                48000,
                rate,
                0.,
                true,
                1,
                None,
            );
            if s[0].abs() > peak.1 {
                peak = (i, s[0].abs());
            }
        }
        let expected = (12000. / rate).round() as usize;
        println!(
            "rate {rate} expected onset {expected} measured {} error_frames {}",
            peak.0,
            peak.0.abs_diff(expected)
        );
        assert!(
            peak.0.abs_diff(expected) <= 64,
            "onset drift exceeds one processor output block at {rate}"
        );
    }
}

#[test]
fn src_interior_fast_path_matches_reference_without_changing_filter_quality() {
    let data = tone(17893., 44100, 0.2);
    for step in [0.1, 0.91875, 1., 1.06, 2., -1.] {
        for base in [-48., 0., 49., 100.5, 8790.] {
            for phase in 0..256 {
                let position = base + phase as f64 / 256.;
                for region in [None, Some((20., 200.))] {
                    assert_eq!(
                        processor::sample(&data.samples, position, step, region),
                        processor::sample_reference(&data.samples, position, step, region),
                        "step={step} position={position} region={region:?}"
                    );
                }
            }
        }
    }
}
