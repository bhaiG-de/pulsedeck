//! Full pipeline smoke test: library compilation enables production inference.
use deck_engine::{analyze_beats_and_sections, analyze_decoded_audio, AudioSourceInfo};

#[test]
#[ignore = "requires installed Beat This! final0 models"]
fn sections_extend_full_model_events_without_replacing_timing() {
    let rate = 22_050_u32;
    let mut seed = 0x2545_f491u32;
    let pcm: Vec<[f32; 2]> = (0..rate * 80)
        .map(|i| {
            let t = f64::from(i) / f64::from(rate);
            let age = t % 0.5;
            let beat = (t / 0.5) as usize;
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = f64::from(seed) / f64::from(u32::MAX) * 2.0 - 1.0;
            let kick = (std::f64::consts::TAU * (50.0 * age + 8.0 * (1.0 - (-age * 30.0).exp())))
                .sin()
                * (-age * 18.0).exp();
            let snare = if beat % 2 == 1 {
                noise * (-age * 25.0).exp() * 0.5
            } else {
                0.0
            };
            let hat = noise * (-(t % 0.25) * 120.0).exp() * 0.15;
            let level = if t < 16.0 || t >= 64.0 { 0.35 } else { 0.8 };
            let sample = ((0.6 * kick + snare + hat) * level) as f32;
            [sample, sample]
        })
        .collect();
    let info = AudioSourceInfo {
        id: "fixture:sections".into(),
        location: "fixture:sections".into(),
        title: "Synthetic section smoke".into(),
        artist: None,
        extension: None,
    };
    let mut analysis = analyze_decoded_audio(&pcm, rate, &info, true).unwrap();
    let native = analysis.neural_rhythm.clone().expect("full model timing");
    let bpm = analysis.bpm;
    let source = analysis.bpm_source.clone();
    analyze_beats_and_sections(&pcm, rate, &mut analysis, &|| false).unwrap();
    assert_eq!(analysis.bpm, bpm);
    assert_eq!(analysis.bpm_source, source);
    assert_eq!(
        analysis.neural_rhythm.as_ref().unwrap().beats_seconds,
        native.beats_seconds
    );
    let sections = analysis.sections.expect("sections from actual downbeats");
    assert!(sections.bar_times.len() >= 9);
    assert_eq!(sections.beats_per_bar, native.beats_per_bar.unwrap());
    assert!(sections.bar_times.iter().all(|t| native
        .downbeats_seconds
        .iter()
        .any(|b| (t - b).abs() < 1e-6)));
    assert!(sections
        .sections
        .iter()
        .all(|s| sections.bar_times.contains(&s.start_seconds)));
}
