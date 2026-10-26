//! Off-callback interpretation of cached source envelopes and compiled controls.
//! No PCM rendering, new analysis, source selection or musical acceptance gate.
use super::Member;
use crate::{scheduled::endpoints, ControlTarget, EngineCommand, EngineSnapshot};
use control_protocol::{forecast::*, performance::Plan};

const MAX_WINDOWS: usize = 24;
const DEVELOPMENT_SECONDS: f64 = 34.;

#[derive(Clone)]
struct State {
    position: Option<f64>,
    playing: bool,
    rate: Option<f64>,
    controls: [f32; crate::CONTROL_TARGET_COUNT],
    normalization: Option<f64>,
    pitch: f64,
    keylock: bool,
    uncertainty: Vec<String>,
    timing_uncertainty: f64,
    control_timing_uncertainty: f64,
}
fn note(state: &mut State, reason: &str) {
    if !state.uncertainty.iter().any(|s| s == reason) {
        state.uncertainty.push(reason.into());
    }
}
fn initial(s: &EngineSnapshot, deck: usize) -> State {
    let d = &s.decks[deck];
    let mut controls = [0.; crate::CONTROL_TARGET_COUNT];
    controls[ControlTarget::Crossfader.index()] = s.crossfader;
    controls[ControlTarget::ChannelFader { deck }.index()] = d.channel_fader;
    controls[ControlTarget::DeckGain { deck }.index()] = d.gain;
    for (target, value) in [
        (ControlTarget::EqLow { deck }, d.eq_low),
        (ControlTarget::EqMid { deck }, d.eq_mid),
        (ControlTarget::EqHigh { deck }, d.eq_high),
        (ControlTarget::Filter { deck }, d.filter),
        (ControlTarget::Echo { deck }, d.echo),
        (ControlTarget::Reverb { deck }, d.reverb),
    ] {
        controls[target.index()] = value;
    }
    let mut state = State {
        position: Some(d.position_seconds),
        playing: d.playing,
        rate: Some(d.playback_rate),
        controls,
        normalization: Some(if d.normalization_enabled {
            d.normalization_gain_db as f64
        } else {
            0.
        }),
        pitch: d.pitch_semitones,
        keylock: d.keylock,
        uncertainty: vec![],
        timing_uncertainty: 0.,
        control_timing_uncertainty: 0.,
    };
    if d.jog_touch {
        state.position = None;
        state.rate = None;
        note(&mut state, "jog_hold_source_output_unmodelled");
    }
    if d.sync_enabled {
        state.rate = None;
        note(&mut state, "sync_source_timing_unproven");
    }
    if d.loop_start_seconds.is_some() {
        state.position = None;
        note(&mut state, "loop_source_timing_unproven");
    }
    if d.launch_pending {
        note(&mut state, "existing_pending_launch_unmodelled");
        state.position = None;
    }
    if s.active_ramps.iter().enumerate().any(|(i, active)| {
        *active
            && ControlTarget::from_index(i)
                .deck()
                .is_none_or(|n| n == deck)
    }) {
        note(&mut state, "existing_ramps_unmodelled");
    }
    if d.fx_tail_active {
        note(&mut state, "existing_fx_tail_unmodelled");
    }
    state
}
fn advance(state: &mut State, seconds: f64, duration: f64) {
    if !state.playing || seconds <= 0. {
        return;
    }
    state.position = state
        .position
        .zip(state.rate)
        .map(|(p, r)| (p + seconds * r).min(duration));
    if state.position.is_some_and(|p| p >= duration) {
        state.playing = false;
    }
}
fn state_at(s: &EngineSnapshot, deck: usize, members: &[Member], at: f64) -> State {
    let mut state = initial(s, deck);
    let duration = s.decks[deck]
        .track
        .as_ref()
        .map_or(0., |t| t.duration_seconds);
    let mut time = 0.;
    for m in members.iter().filter(|m| m.start <= at + 1e-8) {
        if m.command.deck() != Some(deck)
            && !matches!(
                m.command,
                EngineCommand::SetCrossfader { .. }
                    | EngineCommand::Ramp {
                        target: ControlTarget::Crossfader,
                        ..
                    }
                    | EngineCommand::SetMaster { .. }
            )
        {
            continue;
        }
        advance(&mut state, (m.start - time).max(0.), duration);
        time = m.start;
        if m.latest_start - m.start > 1e-7 {
            if super::changes_clock(&m.command, deck)
                || matches!(
                    m.command,
                    EngineCommand::Play { .. } | EngineCommand::PlayQuantized { .. }
                )
            {
                state.timing_uncertainty = state.timing_uncertainty.max(m.latest_start - m.start);
                note(&mut state, "source_clock_action_start_is_bounded");
            } else if at < m.end + super::completion_margin(&m.command, s.sample_rate) {
                state.control_timing_uncertainty = state
                    .control_timing_uncertainty
                    .max(m.latest_start - m.start);
                note(&mut state, "control_action_start_is_bounded");
            }
        }
        match &m.command {
            EngineCommand::Play { .. } => state.playing = true,
            EngineCommand::Pause { .. } => state.playing = false,
            EngineCommand::PlayQuantized { .. } => {
                state.playing = true;
                state.position = None;
                note(&mut state, "nested_quantized_launch_timing_unproven");
            }
            EngineCommand::Seek { seconds, .. } => {
                state.position = Some(seconds.clamp(0., duration))
            }
            EngineCommand::SetTempo { percent, .. } => state.rate = Some(1. + percent / 100.),
            EngineCommand::SetKeylock { enabled, .. } => state.keylock = *enabled,
            EngineCommand::SetPitch { semitones, .. } => state.pitch = *semitones,
            EngineCommand::SetNormalization { enabled, .. } => {
                state.normalization = if !enabled {
                    Some(0.)
                } else {
                    Some(s.decks[deck].normalization_gain_db as f64)
                };
            }
            EngineCommand::Ramp {
                target,
                value,
                seconds,
                ..
            } => {
                if matches!(
                    target,
                    ControlTarget::Echo { .. } | ControlTarget::Reverb { .. }
                ) && *value > 0.
                {
                    note(&mut state, "authored_fx_tail_unmodelled");
                }
                let index = target.index();
                let fraction = ((at - m.start) / seconds.max(1e-9)).clamp(0., 1.) as f32;
                state.controls[index] += (*value - state.controls[index]) * fraction;
                if matches!(target, ControlTarget::TempoPercent { .. }) {
                    state.rate = None;
                    note(&mut state, "tempo_curve_source_timing_unproven");
                }
            }
            command if super::changes_clock(command, deck) => {
                state.position = None;
                state.rate = None;
                note(&mut state, "transport_or_rate_edit_unmodelled");
            }
            command => {
                if matches!(command,EngineCommand::SetFx {echo,reverb,..} if *echo>0.||*reverb>0.) {
                    note(&mut state, "authored_fx_tail_unmodelled");
                }
                let mut values = [None; crate::CONTROL_TARGET_COUNT];
                endpoints(command, &mut values);
                for (i, v) in values.into_iter().enumerate() {
                    if let Some(v) = v {
                        state.controls[i] = v;
                    }
                }
            }
        }
    }
    advance(&mut state, (at - time).max(0.), duration);
    state
}
fn bands(s: &EngineSnapshot, deck: usize, range: [f64; 2]) -> Option<[f64; 3]> {
    let track = s.decks[deck].track.as_ref()?;
    let bands = track.waveform_bands.as_ref()?;
    let n = bands.low.len();
    if n == 0 || bands.mid.len() != n || bands.high.len() != n || track.duration_seconds <= 0. {
        return None;
    }
    let width = track.duration_seconds / n as f64;
    let start = range[0].max(0.);
    let end = range[1]
        .max(start + width.min(0.01))
        .min(track.duration_seconds);
    if start >= end {
        return None;
    }
    let first = (start / width).floor() as usize;
    let last = ((end / width).ceil() as usize).min(n);
    let mut square = [0.; 3];
    let mut covered = 0.;
    for i in first..last {
        let overlap = (end.min((i + 1) as f64 * width) - start.max(i as f64 * width)).max(0.);
        let values = [bands.low[i], bands.mid[i], bands.high[i]];
        if values.iter().any(|v| !v.is_finite() || *v < 0.) {
            return None;
        }
        for b in 0..3 {
            square[b] += values[b] as f64 * values[b] as f64 * overlap;
        }
        covered += overlap;
    }
    (covered > 0.).then(|| square.map(|v| (v / covered).sqrt()))
}
fn eq(state: &State, deck: usize) -> [f32; 3] {
    [
        ControlTarget::EqLow { deck },
        ControlTarget::EqMid { deck },
        ControlTarget::EqHigh { deck },
    ]
    .map(|t| state.controls[t.index()])
}
fn cross_gain(state: &State, deck: usize) -> f64 {
    let angle = (state.controls[ControlTarget::Crossfader.index()] as f64 + 1.)
        * std::f64::consts::FRAC_PI_4;
    if deck == 0 {
        angle.cos()
    } else {
        angle.sin()
    }
}
fn contribution(
    s: &EngineSnapshot,
    deck: usize,
    members: &[Member],
    times: [f64; 2],
) -> ForecastContribution {
    let a = state_at(s, deck, members, times[0]);
    let b = state_at(s, deck, members, (times[1] - 1e-7).max(times[0]));
    let mid = state_at(s, deck, members, (times[0] + times[1]) * 0.5);
    let mut reasons = mid.uncertainty.clone();
    let positions = a.position.zip(b.position).map(|(a, b)| [a, b]);
    let effective = positions
        .and_then(|p| {
            s.decks[deck]
                .grid_position(p[0])
                .zip(s.decks[deck].grid_position(p[1]))
        })
        .map(|(a, b)| [a, b]);
    let source = positions
        .and_then(|p| bands(s, deck, p))
        .zip(mid.normalization)
        .map(|(v, g)| v.map(|n| n * 10f64.powf(g / 20.)));
    if source.is_none() {
        reasons.push(
            if positions.is_none() {
                "source_position_unavailable"
            } else {
                "cached_bands_or_normalization_unavailable"
            }
            .into(),
        );
    }
    if [&a, &mid, &b]
        .iter()
        .any(|state| state.controls[ControlTarget::Filter { deck }.index()].abs() > 0.001)
    {
        reasons.push("filter_spectral_response_unmodelled".into());
    }
    if [&a, &mid, &b].iter().any(|state| {
        state.controls[ControlTarget::Echo { deck }.index()] > 0.001
            || state.controls[ControlTarget::Reverb { deck }.index()] > 0.001
    }) {
        reasons.push("fx_response_unmodelled".into());
    }
    let pitch = mid
        .rate
        .map(|r| mid.pitch + if mid.keylock { 0. } else { 12. * r.log2() });
    if pitch.is_some_and(|p| p.abs() > 0.5) {
        reasons.push("pitch_spectral_redistribution_unmodelled".into());
    }
    let weight = mid.controls[ControlTarget::ChannelFader { deck }.index()] as f64
        * mid.controls[ControlTarget::DeckGain { deck }.index()] as f64
        * cross_gain(&mid, deck)
        * if mid.playing { 1. } else { 0. };
    // Bounded mixer starts limit precision, not availability of the nominal model.
    let nominal_modelled = reasons
        .iter()
        .all(|reason| reason == "control_action_start_is_bounded");
    let low_eq_open_cue = source.filter(|_| nominal_modelled).map(|v| v[0] * weight);
    let known_silent = !a.playing
        && !mid.playing
        && !b.playing
        && reasons.iter().all(|r| {
            r == "source_position_unavailable" || r == "cached_bands_or_normalization_unavailable"
        });
    let proxy = if known_silent {
        Some([0.; 3])
    } else if nominal_modelled {
        source.map(|v| std::array::from_fn(|band| v[band] * eq(&mid, deck)[band] as f64 * weight))
    } else {
        None
    };
    ForecastContribution {
        coverage: if proxy.is_some() && reasons.is_empty() {
            ForecastCoverage::Available
        } else if source.is_some() {
            ForecastCoverage::Partial
        } else {
            ForecastCoverage::Unavailable
        },
        uncertainty: reasons,
        timing_uncertainty_seconds: mid.timing_uncertainty.max(mid.control_timing_uncertainty),
        source_timing_uncertainty_seconds: mid.timing_uncertainty,
        source_seconds: positions,
        effective_beats: effective,
        playing: mid.playing,
        rate: mid.rate,
        playing_bpm: effective
            .filter(|_| mid.playing && a.playing && b.playing && mid.timing_uncertainty == 0.)
            .map(|p| 60. * (p[1] - p[0]) / (times[1] - times[0])),
        pitch_shift_semitones: pitch,
        normalization_gain_db: mid.normalization,
        fader: [
            a.controls[ControlTarget::ChannelFader { deck }.index()],
            b.controls[ControlTarget::ChannelFader { deck }.index()],
        ],
        eq: [eq(&a, deck), eq(&b, deck)],
        source_rms: source,
        contribution_cue: proxy,
        low_eq_open_cue,
    }
}
fn grid_anomaly(s: &EngineSnapshot, deck: usize, range: [f64; 2]) -> Option<String> {
    let d = &s.decks[deck];
    let first = d.grid_position(range[0])?.floor() as i64;
    let last = d.grid_position(range[1])?.ceil() as i64;
    if last - first > 512 {
        return Some("Effective event density exceeds bounded pulse inspection".into());
    }
    let interval = |i: i64| {
        d.grid_time(i as f64)
            .zip(d.grid_time((i + 1) as f64))
            .map(|(a, b)| b - a)
            .filter(|n| *n > 0. && n.is_finite())
    };
    let mut neighbors: Vec<_> = ((first - 8)..(last + 8)).filter_map(interval).collect();
    if neighbors.len() < 4 {
        return None;
    }
    neighbors.sort_by(f64::total_cmp);
    let median = neighbors[neighbors.len() / 2];
    let values: Vec<_> = (first..last).filter_map(interval).collect();
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(0., f64::max);
    (min<median*0.8||max>median*1.25).then(||format!("Effective grid intervals {:.3}..{:.3}s versus neighboring median {:.3}s; may be pulse/grid ambiguity or real tempo change. Phase/pulse comparison is unproven here.",min,max,median))
}
fn finding(
    code: &str,
    seconds: [f64; 2],
    deck: Option<usize>,
    source_seconds: Option<[f64; 2]>,
    evidence: String,
) -> ForecastFinding {
    ForecastFinding {
        code: code.into(),
        seconds,
        deck,
        source_seconds,
        evidence,
    }
}
pub(super) fn build(plan: &Plan, s: &EngineSnapshot, members: &[Member]) -> TransitionForecast {
    let sources = std::array::from_fn(|deck| {
        let t = s.decks[deck].track.as_ref();
        ForecastSource {
            deck,
            source: t.map(|t| t.path.clone()),
            load_revision: s.load_revisions[deck],
            grid_revision: s.decks[deck].grid_revision,
            source_fingerprint: t.and_then(|t| t.source_fingerprint.clone()),
            analysis_version: t.map(|t| t.analysis_version),
            bin_seconds: t.and_then(|t| {
                t.waveform_bands
                    .as_ref()
                    .filter(|b| !b.low.is_empty())
                    .map(|b| t.duration_seconds / b.low.len() as f64)
            }),
            key: t.and_then(|t| t.key.clone()),
            camelot: t.and_then(|t| t.camelot.clone()),
            key_confidence: t.and_then(|t| t.key_confidence),
            bpm_source: t.and_then(|t| t.bpm_source.clone()),
            beats_per_bar: t.and_then(|t| t.neural_rhythm.as_ref()?.beats_per_bar),
        }
    });
    let end = members.iter().map(|m| m.end).fold(0., f64::max);
    let elapsed = (s
        .rendered_frames
        .saturating_sub(plan.generation.observed_rendered_frames)) as f64
        / s.sample_rate as f64;
    let development = s
        .decks
        .iter()
        .filter_map(|d| {
            d.local_bpm.filter(|b| *b > 0.).map(|b| {
                let meter = d
                    .track
                    .as_ref()
                    .and_then(|t| t.neural_rhythm.as_ref()?.beats_per_bar)
                    .unwrap_or(4);
                16. * meter as f64 * 60. / b
            })
        })
        .fold(DEVELOPMENT_SECONDS, f64::max)
        .min(90.);
    let horizon = plan.closing.as_ref().map_or(end + development, |c| {
        (c.endpoint_seconds - elapsed).max(0.)
    });
    let mut focus: Vec<[f64; 2]> = vec![];
    for m in members {
        if m.end - m.start > 64. {
            focus.push([(m.start - 4.).max(0.), (m.start + 32.).min(horizon)]);
            focus.push([(m.end - 8.).max(0.), (m.end + development).min(horizon)]);
        } else {
            focus.push([(m.start - 4.).max(0.), (m.end + development).min(horizon)]);
        }
    }
    focus.retain(|r| r[1] > r[0]);
    focus.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut merged: Vec<[f64; 2]> = vec![];
    for r in focus {
        if let Some(last) = merged.last_mut().filter(|last| r[0] <= last[1]) {
            last[1] = last[1].max(r[1]);
        } else {
            merged.push(r);
        }
    }
    let total = merged.iter().map(|r| r[1] - r[0]).sum::<f64>();
    let spacing = (total / 36.).ceil().max(2.);
    let mut cuts = vec![0., horizon];
    for r in &merged {
        cuts.extend(r);
        let mut t = r[0] + spacing;
        while t < r[1] {
            cuts.push(t);
            t += spacing;
        }
    }
    for m in members {
        cuts.extend([m.start, m.end]);
        if let EngineCommand::Ramp { seconds, .. } = m.command {
            cuts.push(m.start + seconds);
        }
    }
    // Include source exhaustion before aggregating windows; a partial EOF window
    // must never turn a steady grid into an invented slower played pulse.
    let mut segments = vec![0., horizon];
    segments.extend(members.iter().map(|m| m.start));
    segments.sort_by(f64::total_cmp);
    segments.dedup();
    for pair in segments.windows(2) {
        for deck in 0..2 {
            let state = state_at(s, deck, members, pair[0]);
            if state.playing {
                if let Some((position, rate)) =
                    state.position.zip(state.rate).filter(|(_, r)| *r > 0.)
                {
                    if let Some(track) = s.decks[deck].track.as_ref() {
                        let eof = pair[0] + (track.duration_seconds - position) / rate;
                        if eof > pair[0] + 1e-7 && eof < pair[1] {
                            cuts.push(eof);
                        }
                    }
                }
            }
        }
    }
    cuts.retain(|v| v.is_finite() && *v >= 0. && *v <= horizon);
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    let eligible: Vec<_> = cuts
        .windows(2)
        .enumerate()
        .filter(|(_, p)| {
            p[1] - p[0] >= 0.001
                && merged
                    .iter()
                    .any(|r| p[0] >= r[0] - 1e-7 && p[1] <= r[1] + 1e-7)
        })
        .map(|(i, _)| i)
        .collect();
    // Spread the bounded output across the entire course, retaining the final
    // eight intervals so dense early preparation cannot hide the closing course.
    let selected: Vec<_> = if eligible.len() <= MAX_WINDOWS {
        eligible.clone()
    } else {
        let head = eligible.len() - 8;
        let count = MAX_WINDOWS - 8;
        (0..count)
            .map(|i| eligible[i * (head - 1) / (count - 1)])
            .chain(eligible[head..].iter().copied())
            .collect()
    };
    let mut windows = vec![];
    let mut omitted = vec![];
    let mut findings = vec![];
    for (index, pair) in cuts.windows(2).enumerate() {
        let times = [pair[0], pair[1]];
        if times[1] - times[0] < 0.001 {
            continue;
        }
        if !selected.contains(&index) {
            omitted.push(times);
            continue;
        }
        let mut decks = std::array::from_fn(|d| contribution(s, d, members, times));
        let mut anomaly = false;
        for deck in 0..2 {
            if decks[deck].playing {
                if let Some(evidence) = decks[deck]
                    .source_seconds
                    .and_then(|p| grid_anomaly(s, deck, p))
                {
                    anomaly = true;
                    decks[deck]
                        .uncertainty
                        .push("effective_grid_interval_anomaly".into());
                    findings.push(finding(
                        "grid_interval_anomaly",
                        times,
                        Some(deck),
                        decks[deck].source_seconds,
                        evidence,
                    ));
                }
            }
        }
        let dominance = std::array::from_fn(|band| {
            match (decks[0].contribution_cue, decks[1].contribution_cue) {
                (Some(a), Some(b)) => {
                    if a[band].max(b[band]) < 1e-5 {
                        BandDominance::Low
                    } else if a[band] > b[band] * 2. {
                        BandDominance::A
                    } else if b[band] > a[band] * 2. {
                        BandDominance::B
                    } else {
                        BandDominance::Shared
                    }
                }
                _ => BandDominance::Unknown,
            }
        });
        let phase = if !anomaly
            && decks
                .iter()
                .all(|d| d.playing && d.source_timing_uncertainty_seconds == 0.)
        {
            decks[0]
                .effective_beats
                .zip(decks[1].effective_beats)
                .map(|(a, b)| {
                    std::array::from_fn(|i| {
                        let p = (a[i] - b[i]).rem_euclid(1.);
                        p.min(1. - p)
                    })
                })
        } else {
            None
        };
        let mut actions = members
            .iter()
            .filter(|m| m.start < times[1] && m.end >= times[0])
            .map(|m| plan.actions[m.action].id.clone())
            .fold(vec![], |mut v, id| {
                if !v.contains(&id) {
                    v.push(id);
                }
                v
            });
        let actions_omitted = actions.len().saturating_sub(4);
        actions.truncate(4);
        windows.push(ForecastWindow {
            seconds: times,
            actions,
            actions_omitted,
            decks,
            band_dominance: dominance,
            fractional_phase_error_beats: phase,
        });
    }
    let mut reference_low = 0_f64;
    for i in 0..windows.len() {
        let w = &windows[i];
        if let Some(phase) = w.fractional_phase_error_beats.filter(|_| {
            w.decks
                .iter()
                .all(|d| d.playing && d.fader[0].max(d.fader[1]) > 0.1)
        }) {
            for (action_id, action) in plan.actions.iter().enumerate() {
                if let Some(alignment) = action.alignment.filter(|a| !a.maintain) {
                    if members.iter().any(|m| {
                        m.action == action_id && m.start < w.seconds[1] && m.end >= w.seconds[0]
                    }) && phase[0].max(phase[1]) > alignment.phase_tolerance_beats
                    {
                        findings.push(finding("phase_exceeds_entry_tolerance",w.seconds,Some(alignment.deck),w.decks[alignment.deck].source_seconds,
                            format!("Action {} exposed fixed-clock fractional phase {:.4}→{:.4} beats exceeds its authored entry tolerance {:.4}. The entry gate does not maintain following; this is grid telemetry, not an acoustic verdict.",action.id,phase[0],phase[1],alignment.phase_tolerance_beats)));
                    }
                }
            }
        }
        let cues = w.decks[0].contribution_cue.zip(w.decks[1].contribution_cue);
        if let Some((a, b)) = cues {
            let strongest = a[0].max(b[0]);
            if reference_low > 0.001 && strongest < reference_low * 0.35 {
                findings.push(finding("low_band_support_reduced",w.seconds,None,None,format!("Strongest nominal low-band cue {:.4} versus earlier {:.4}; inspect whether this withdrawal is intended. These are individual envelope cues, not summed audio.",strongest,reference_low)));
            }
            reference_low = reference_low.max(strongest);
        }
        for deck in 0..2 {
            let d = &w.decks[deck];
            let other = &w.decks[1 - deck];
            if let Some(source) = d.source_rms.filter(|_| d.playing) {
                let fader = (d.fader[0] + d.fader[1]) * 0.5;
                if fader > 0.05
                    && d.eq[0][0].max(d.eq[1][0]) < 0.2
                    && d.low_eq_open_cue
                        .zip(other.contribution_cue)
                        .is_some_and(|(open, c)| open > c[0] * 2. && open > 0.005)
                {
                    findings.push(finding("available_source_low_withheld",w.seconds,Some(deck),d.source_seconds,
                        format!("Cached normalized low RMS {:.4} is present, while EQ low {:.3}→{:.3} and fader {:.3}→{:.3} withhold it during reduced support from the other deck.",source[0],d.eq[0][0],d.eq[1][0],d.fader[0],d.fader[1])));
                }
                if let Some(previous) = i
                    .checked_sub(1)
                    .map(|j| &windows[j])
                    .filter(|p| (p.seconds[1] - w.seconds[0]).abs() < 0.01)
                    .filter(|p| p.decks[deck].normalization_gain_db == d.normalization_gain_db)
                    .and_then(|p| p.decks[deck].source_rms)
                {
                    if previous[0] > 0.005
                        && (source[0] < previous[0] * 0.45 || source[0] > previous[0] * 2.2)
                    {
                        findings.push(finding(if source[0]<previous[0]{"source_low_withdrawal"}else{"source_low_arrival"},w.seconds,Some(deck),d.source_seconds,format!("Cached normalized source low RMS {:.4}→{:.4}; source structure, before fader/EQ weighting.",previous[0],source[0])));
                    }
                }
            }
        }
        if !w.decks.iter().any(|d| {
            d.uncertainty
                .iter()
                .any(|n| n == "effective_grid_interval_anomaly")
        }) {
            if let Some((a, b)) = w.decks[0]
                .playing_bpm
                .zip(w.decks[1].playing_bpm)
                .filter(|_| w.decks.iter().all(|d| d.playing && d.fader[1] > 0.1))
            {
                if (a - b).abs() / a.min(b).max(1.) > 0.02 {
                    findings.push(finding("playing_pulse_difference",w.seconds,None,None,format!("Effective played pulse {:.2} vs {:.2} BPM; phase course {:?}. This describes the chosen clocks, not an acoustic beatmatch verdict or a requirement to enable Sync.",a,b,w.fractional_phase_error_beats)));
                }
            }
        }
    }
    // Keep concrete withheld/support issues ahead of repetitive source changes.
    findings.sort_by_key(|f| match f.code.as_str() {
        "phase_exceeds_entry_tolerance" => 0,
        "available_source_low_withheld" => 0,
        "low_band_support_reduced" => 1,
        "grid_interval_anomaly" => 2,
        "playing_pulse_difference" => 3,
        _ => 4,
    });
    // Merge repeated contiguous observations while retaining timed source bounds.
    let mut compact: Vec<ForecastFinding> = vec![];
    for item in findings {
        if let Some(previous) = compact.last_mut().filter(|p| {
            p.code == item.code
                && p.deck == item.deck
                && (p.seconds[1] - item.seconds[0]).abs() < 0.01
        }) {
            if !previous.evidence.starts_with("First-window example") {
                previous.evidence = format!(
                    "First-window example [{:.3}, {:.3}] renderer seconds: {}",
                    previous.seconds[0], previous.seconds[1], previous.evidence
                );
            }
            previous.seconds[1] = item.seconds[1];
            if let (Some(p), Some(n)) = (&mut previous.source_seconds, item.source_seconds) {
                p[1] = n[1];
            }
        } else {
            compact.push(item);
        }
    }
    let findings_omitted = compact.len().saturating_sub(24);
    compact.truncate(24);
    let mut omitted_ranges: Vec<[f64; 2]> = vec![];
    for range in omitted {
        if let Some(last) = omitted_ranges
            .last_mut()
            .filter(|r| (r[1] - range[0]).abs() < 0.01)
        {
            last[1] = range[1];
        } else {
            omitted_ranges.push(range);
        }
    }
    let rhythm_contracts = plan
        .actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            a.alignment.filter(|a| a.maintain).map(|alignment| {
                let start = members
                    .iter()
                    .filter(|m| m.action == i)
                    .map(|m| m.start)
                    .fold(f64::INFINITY, f64::min);
                let end = members
                    .iter()
                    .filter(|m| m.action == i)
                    .map(|m| m.end + super::completion_margin(&m.command, s.sample_rate))
                    .fold(0., f64::max);
                ForecastRhythmContract {
                    action: a.id.clone(),
                    deck: alignment.deck,
                    seconds: [start, end],
                    phase_tolerance_beats: alignment.phase_tolerance_beats,
                    bar_beats: alignment.bar_beats,
                    sync_rate_limits: s.decks[alignment.deck].sync_rate_limits,
                }
            })
        })
        .collect();
    TransitionForecast {origin_rendered_frame:s.rendered_frames,sample_rate:s.sample_rate,horizon_seconds:horizon,
        method:"Cached time-weighted low/mid/high RMS with observed normalization and nominal pre-master fader/gain/EQ/crossfader weights (master bus, sampler and mixed phase are not modelled); analysis/EQ crossovers differ. Not summed/rendered audio, instrument labels, perceived energy, phase-coherent level, or a smoothness verdict. Controls use midpoint nominal weights over each RMS window; native smoothing is omitted. Source positions with nonzero source_timing_uncertainty_seconds are nominal, and exact contribution/phase is withheld. Uncertain timing/FX/filter/pitch are explicit. Development follows 16 estimated bars (64 beats if meter unknown), capped at 90 seconds. Findings are advisory; omitted ranges are not forecast.".into(),
        sources,windows,omitted_ranges,findings:compact,findings_omitted,rhythm_contracts}
}
