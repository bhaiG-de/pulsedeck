//! Pure preflight for artist-authored scores; all execution stays in NativeBatch.
use crate::{scheduled::*, ControlTarget, EngineCommand, EngineSnapshot};
use control_protocol::{performance::*, ControlError};
use std::collections::HashMap;
#[path = "performance_forecast.rs"]
mod forecast;

type Result<T> = std::result::Result<T, ControlError>;
fn invalid(code: &str, message: impl Into<String>) -> ControlError {
    ControlError::new(code, message)
}
pub fn generation(s: &EngineSnapshot) -> Generation {
    Generation {
        engine_epoch: s.engine_epoch,
        load_revisions: s.load_revisions,
        grid_revisions: s.decks.each_ref().map(|d| d.grid_revision),
        position_revisions: s.decks.each_ref().map(|d| d.position_revision),
        control_revision: s.control_revision,
        tempo_percent: s.decks.each_ref().map(|d| d.tempo_percent),
        observed_rendered_frames: s.rendered_frames,
        sample_rate: s.sample_rate,
        sync_enabled: s.decks.each_ref().map(|d| d.sync_enabled),
        master_deck: s.decks.iter().position(|d| d.is_master).unwrap_or(0),
        sync_rate_limits: s.decks.each_ref().map(|d| d.sync_rate_limits),
    }
}
pub fn event_handle(s: &EngineSnapshot, deck: usize, raw_index: i64) -> Result<EventHandle> {
    let seconds = event_seconds(s, deck, raw_index).map_err(|e| invalid("event_unavailable", e))?;
    let d = &s.decks[deck];
    Ok(EventHandle {
        deck,
        source: d.track.as_ref().unwrap().path.clone(),
        load_revision: s.load_revisions[deck],
        grid_revision: d.grid_revision,
        raw_index,
        effective_beat: d.grid_position(seconds).unwrap(),
        source_seconds: seconds,
    })
}
pub fn resolve_event(s: &EngineSnapshot, handle: &EventHandle) -> Result<f64> {
    let exact = event_handle(s, handle.deck, handle.raw_index)?;
    if exact.source != handle.source
        || exact.load_revision != handle.load_revision
        || exact.grid_revision != handle.grid_revision
    {
        return Err(invalid(
            "stale_event",
            "Event handle source/grid generation changed; observe again",
        ));
    }
    if (exact.source_seconds - handle.source_seconds).abs() > 1e-9
        || (exact.effective_beat - handle.effective_beat).abs() > 1e-9
    {
        return Err(invalid("event_mismatch","Use the exact returned event handle; rounded source seconds/effective beat are not event identities"));
    }
    let d = &s.decks[handle.deck];
    if !d.playing || d.loop_start_seconds.is_some() {
        return Err(invalid(
            "clock_unavailable",
            "An absolute event requires a playing, unlooped clock",
        ));
    }
    let seconds = (exact.source_seconds - d.position_seconds) / d.playback_rate;
    if seconds <= 0.02 {
        return Err(invalid(
            "past_event",
            "Event is past or lacks the 20ms arming margin; choose a fresh future handle",
        )
        .with_detail("raw_index", handle.raw_index)
        .with_detail("lead_seconds", seconds));
    }
    Ok(seconds)
}
fn duration(s: &EngineSnapshot, d: Duration, start: f64) -> Result<f64> {
    let value = match d {
        Duration::Seconds { value } => value,
        Duration::Beats { clock, value } => {
            let deck = s
                .decks
                .get(clock)
                .ok_or_else(|| invalid("invalid_arguments", "Beat clock must be 0/A or 1/B"))?;
            if !deck.playing || deck.sync_enabled || deck.loop_start_seconds.is_some() {
                return Err(invalid("beat_timing_unproven","Beat durations require a playing stable clock; use that deck as clock or prepare/observe after launch, Sync or loop edits"));
            }
            if !value.is_finite() || value < 0. || value > 256. {
                return Err(invalid(
                    "invalid_arguments",
                    "Duration beats must be finite in 0..256",
                ));
            }
            let at = deck.position_seconds + start * deck.playback_rate;
            let beat = deck.grid_position(at).ok_or_else(|| {
                invalid("grid_unavailable", "Beat duration needs a prepared grid")
            })?;
            (deck.grid_time(beat + value).ok_or_else(|| {
                invalid(
                    "grid_unavailable",
                    "Beat duration exceeds the available grid",
                )
            })? - at)
                / deck.playback_rate
        }
    };
    if !value.is_finite() || !(0. ..=3600.).contains(&value) {
        return Err(invalid(
            "invalid_arguments",
            "Duration seconds must be finite in 0..3600",
        ));
    }
    Ok(value)
}
struct Member {
    command: EngineCommand,
    start: f64,
    latest_start: f64,
    end: f64,
    action: usize,
    curve: Option<usize>,
}
pub struct CompiledPerformance {
    pub validation: Validation,
    pub(crate) batch: Box<NativeBatch>,
}
/// Compiles the entire score against one coherent snapshot. Commit calls this
/// again and the renderer checks these generations before accepting ownership.
pub fn validate(plan: &Plan, s: &EngineSnapshot) -> Result<Validation> {
    compile(0, plan, s).map(|p| p.validation)
}
pub(crate) fn compile(id: u64, plan: &Plan, s: &EngineSnapshot) -> Result<CompiledPerformance> {
    let mut current = generation(s);
    current.observed_rendered_frames = plan.generation.observed_rendered_frames;
    // Sync owns derived follower tempo; role, policy and manual revision remain exact guards.
    for deck in 0..2 {
        if current.sync_enabled[deck]
            && current.master_deck != deck
            && current.sync_rate_limits[deck].is_some()
        {
            current.tempo_percent[deck] = plan.generation.tempo_percent[deck];
        }
    }
    if current != plan.generation || s.rendered_frames < plan.generation.observed_rendered_frames {
        return Err(invalid("generation_conflict","Source, position, grid, rate or control generation changed; observe and rebase the score"));
    }
    if plan.operation_id.is_empty()
        || plan.operation_id.len() > 128
        || plan.actions.is_empty()
        || plan.actions.len() > 64
    {
        return Err(invalid(
            "invalid_arguments",
            "A score requires an operation_id and 1..64 actions",
        ));
    }
    let mut canonical = plan.clone();
    for action in &mut canonical.actions {
        if let Timing::SourceEvent { deck, raw_index } = action.when {
            action.when = Timing::Event {
                event: event_handle(s, deck, raw_index)?,
            };
        }
    }
    let plan = &canonical;
    let elapsed = (s.rendered_frames - plan.generation.observed_rendered_frames) as f64
        / s.sample_rate as f64;
    let mut index = HashMap::new();
    for (i, a) in plan.actions.iter().enumerate() {
        if a.after.len() > 64
            || a.after
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != a.after.len()
        {
            return Err(invalid(
                "invalid_arguments",
                "Completion constraints require at most 64 unique action IDs",
            ));
        }
        if a.id.is_empty() || a.id.len() > 128 || index.insert(a.id.as_str(), i).is_some() {
            return Err(invalid(
                "invalid_arguments",
                "Action IDs must be nonempty and unique",
            ));
        }
    }
    let mut resolved: Vec<Option<(f64, f64)>> = vec![None; plan.actions.len()];
    let mut earliest_ends = vec![0.; plan.actions.len()];
    let mut members = Vec::new();
    for _ in 0..plan.actions.len() {
        let mut progress = false;
        for (i, a) in plan.actions.iter().enumerate() {
            if resolved[i].is_some() {
                continue;
            }
            let mut deps = a.after.clone();
            if let Timing::After { action, .. } = &a.when {
                deps.push(action.clone());
            }
            let mut ready = true;
            for dep in &deps {
                let j = *index.get(dep.as_str()).ok_or_else(|| {
                    invalid(
                        "dependency_missing",
                        format!("{} depends on unknown action {dep}", a.id),
                    )
                })?;
                if resolved[j].is_none() {
                    ready = false;
                }
            }
            if !ready {
                continue;
            }
            let (start, latest_start) = match &a.when {
                Timing::Event { event } => (
                    event_start(s, event, &members, false)?,
                    event_start(s, event, &members, true)?,
                ),
                Timing::SourceEvent { .. } => unreachable!("source events canonicalized above"),
                Timing::Finish {
                    before_endpoint_seconds,
                } => {
                    if !before_endpoint_seconds.is_finite()
                        || !(0. ..=86_400.).contains(before_endpoint_seconds)
                    {
                        return Err(invalid(
                            "invalid_arguments",
                            "Finish offset must be finite in 0..86400 seconds",
                        ));
                    }
                    let closing=plan.closing.as_ref().filter(|c|c.session_id.as_ref().is_some_and(|id|!id.is_empty()))
                        .ok_or_else(||invalid("finish_requires_session","Finish needs closing bound to the original session_id and endpoint"))?;
                    let at = closing.endpoint_seconds
                        - elapsed
                        - before_endpoint_seconds
                        - finish_span(a, s)?;
                    (at, at)
                }
                Timing::In { duration: d } => {
                    let at = duration(s, *d, -elapsed)? - elapsed;
                    (at, at)
                }
                Timing::After { offset, .. } => {
                    let early = deps
                        .iter()
                        .map(|id| earliest_ends[index[id.as_str()]])
                        .fold(0., f64::max);
                    let late = deps
                        .iter()
                        .map(|id| resolved[index[id.as_str()]].unwrap().1)
                        .fold(0., f64::max);
                    (
                        early
                            + offset
                                .map(|d| duration(s, d, early))
                                .transpose()?
                                .unwrap_or(0.),
                        late + offset
                            .map(|d| duration(s, d, late))
                            .transpose()?
                            .unwrap_or(0.),
                    )
                }
            };
            if start < 0.02 {
                return Err(invalid(
                    "arming_margin",
                    "Score starts need at least 20ms of arming lead",
                ));
            }
            if !matches!(a.when, Timing::After { .. }) {
                for dep in deps {
                    let end = resolved[index[dep.as_str()]].unwrap().1;
                    if end > start + 1e-7 {
                        return Err(invalid("dependency_deadline",format!("{} starts at {start:.9}s before {dep} can be verified at {end:.9}s",a.id)).with_detail("action_id",a.id.clone()).with_detail("dependency",dep).with_detail("start_seconds",start).with_detail("dependency_end_seconds",end));
                    }
                }
            }
            let mut end = latest_start;
            let mut earliest_end = start;
            if a.commands.is_empty() && a.curves.is_empty() {
                return Err(invalid(
                    "invalid_arguments",
                    "Every action requires commands or curves",
                ));
            }
            for command in &a.commands {
                command
                    .validate()
                    .map_err(|e| invalid("invalid_arguments", e))?;
                if let EngineCommand::Seek { deck, seconds } = command {
                    let duration = s.decks[*deck]
                        .track
                        .as_ref()
                        .map(|t| t.duration_seconds)
                        .ok_or_else(|| {
                            invalid("source_missing", "Seek requires a loaded source")
                        })?;
                    if *seconds < 0. || *seconds > duration {
                        return Err(invalid("seek_out_of_range","A score seek must lie inside the actual source; implicit native clamping is not a timeline"));
                    }
                }
                if matches!(command, EngineCommand::PlayQuantized { .. }) {
                    return Err(invalid("nested_timing","Put launch timing on Action.when and use Play; direct deck_control retains PlayQuantized"));
                }
                let mut command = command.clone();
                let len = if let EngineCommand::Ramp {
                    seconds,
                    clock,
                    beats,
                    quantize,
                    ..
                } = &mut command
                {
                    if quantize.is_some() {
                        return Err(invalid(
                            "ambiguous_units",
                            "Put timing on the action; per-ramp quantization is not allowed",
                        ));
                    }
                    let len = if let Some(beats) = beats {
                        duration(
                            s,
                            Duration::Beats {
                                clock: clock.ok_or_else(|| {
                                    invalid("invalid_arguments", "Beat ramp needs clock")
                                })?,
                                value: *beats,
                            },
                            start,
                        )?
                    } else {
                        *seconds
                    };
                    *seconds = len;
                    *clock = None;
                    *beats = None;
                    len
                } else if let EngineCommand::ScrubTo { duration_ms, .. } = &command {
                    duration_ms / 1000.
                } else {
                    0.
                };
                let margin = completion_margin(&command, s.sample_rate);
                end = end.max(latest_start + len + margin);
                earliest_end = earliest_end.max(start + len);
                members.push(Member {
                    command,
                    start,
                    latest_start,
                    end: latest_start + len,
                    action: i,
                    curve: None,
                });
            }
            for (curve_index, curve) in a.curves.iter().enumerate() {
                if curve.points.is_empty() || curve.points.len() > 32 {
                    return Err(invalid(
                        "invalid_arguments",
                        "Curves require 1..32 increasing endpoints",
                    ));
                }
                let mut prior = 0.;
                for point in &curve.points {
                    let at = duration(s, point.at, start)?;
                    if at <= prior || at - prior < 0.001 {
                        return Err(invalid(
                            "curve_order",
                            "Curve endpoints must increase by at least 1ms",
                        ));
                    }
                    let command = EngineCommand::Ramp {
                        target: curve.target,
                        value: point.value,
                        seconds: at - prior,
                        clock: None,
                        beats: None,
                        quantize: None,
                    };
                    command
                        .validate()
                        .map_err(|e| invalid("invalid_arguments", e))?;
                    members.push(Member {
                        command,
                        start: start + prior,
                        latest_start: latest_start + prior,
                        end: latest_start + at,
                        action: i,
                        curve: Some(curve_index),
                    });
                    prior = at;
                }
                end = end.max(latest_start + prior + smoothing_margin(s.sample_rate));
                earliest_end = earliest_end.max(start + prior);
            }
            resolved[i] = Some((start, end));
            earliest_ends[i] = earliest_end;
            progress = true;
        }
        if !progress {
            break;
        }
    }
    if resolved.iter().any(Option::is_none) {
        return Err(invalid(
            "dependency_cycle",
            "Score contains a dependency cycle",
        ));
    }
    if members.len() > 64 {
        return Err(invalid(
            "capacity_exceeded",
            "Expanded score exceeds 64 native commands; submit bounded passages",
        ));
    }
    members.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.action.cmp(&b.action)));
    for (i, action) in plan.actions.iter().enumerate() {
        if let Timing::Event { event } = &action.when {
            let complete: Vec<_> = members
                .iter()
                .filter(|m| m.action != i)
                .map(|m| Member {
                    command: m.command.clone(),
                    start: m.start,
                    latest_start: m.latest_start,
                    end: m.end,
                    action: m.action,
                    curve: m.curve,
                })
                .collect();
            let recomputed = event_start(s, event, &complete, false)?;
            if (recomputed - resolved[i].unwrap().0).abs() > 1e-7 {
                return Err(invalid("clock_dependency_missing","A later-listed earlier control changes this event's clock; explicitly link/reorder clock preparation before the event"));
            }
        }
    }
    let mut beat_clocks = Vec::new();
    for action in &plan.actions {
        let mut note = |d: Duration| {
            if let Duration::Beats { clock, .. } = d {
                beat_clocks.push(clock)
            }
        };
        match action.when {
            Timing::In { duration } => note(duration),
            Timing::After {
                offset: Some(duration),
                ..
            } => note(duration),
            _ => {}
        }
        for curve in &action.curves {
            for point in &curve.points {
                note(point.at);
            }
        }
        for command in &action.commands {
            if let EngineCommand::Ramp {
                clock: Some(clock),
                beats: Some(_),
                ..
            } = command
            {
                beat_clocks.push(*clock);
            }
        }
    }
    if beat_clocks
        .iter()
        .any(|clock| members.iter().any(|m| changes_clock(&m.command, *clock)))
    {
        return Err(invalid("beat_timing_unproven","A beat duration clock changes rate/position inside the score; prepare it first or use a stable independent clock"));
    }
    for (i, m) in members.iter().enumerate() {
        for previous in &members[..i] {
            if command_targets(&m.command) & command_targets(&previous.command) != 0
                && !(m.action == previous.action && m.curve.is_some() && m.curve == previous.curve)
                && (m.start < previous.end - 1e-8 || (m.start - previous.start).abs() < 1e-8)
            {
                return Err(invalid(
                    "control_conflict",
                    format!(
                        "{} overlaps {} on a native control",
                        plan.actions[m.action].id, plan.actions[previous.action].id
                    ),
                ));
            }
        }
    }
    // Equal triggers are one explicit atomic group, including coupled bass moves.
    let mut commands = Vec::new();
    let mut starts: Vec<f64> = Vec::new();
    let mut native_ids = vec![Vec::new(); plan.actions.len()];
    for (i, m) in members.iter().enumerate() {
        let group = if let Some(n) = starts.iter().position(|t| (*t - m.start).abs() < 1e-9) {
            n
        } else {
            starts.push(m.start);
            starts.len() - 1
        };
        native_ids[m.action].push(i as u64 + 1);
        commands.push(QueuedCommand{action_id:i as u64+1,command:m.command.clone(),when:match &plan.actions[m.action].when{Timing::Event{event}=>{if (m.start-resolved[m.action].unwrap().0).abs()<1e-9{QueueTiming::AtSeconds{clock:event.deck,seconds:event.source_seconds}}else{if s.decks[event.deck].sync_enabled||members.iter().any(|m|m.start<resolved[m.action].unwrap().1&&changes_clock(&m.command,event.deck)){return Err(invalid("event_curve_offset","Multi-segment source-event curves require a stable unlooped clock without tempo/position edits"));}QueueTiming::AtSeconds{clock:event.deck,seconds:event.source_seconds+(m.start-resolved[m.action].unwrap().0)*s.decks[event.deck].playback_rate}}},_=>QueueTiming::AfterSeconds{seconds:m.start}},group_id:(members.iter().filter(|other|other.action==m.action&&(other.start-m.start).abs()<1e-9).count()>1).then_some((m.action as u64+1)*64+group as u64)});
    }
    // Completion-relative starts remain linked to verified native completion.
    // Fixed events keep their authored deadlines and were checked above.
    for (i, m) in members.iter().enumerate() {
        if let Timing::After { action, offset } = &plan.actions[m.action].when {
            let parent = index[action.as_str()];
            let parent_member = members
                .iter()
                .enumerate()
                .filter(|(_, p)| p.action == parent)
                .max_by(|(_, a), (_, b)| a.end.total_cmp(&b.end))
                .unwrap()
                .0;
            let parent_end = resolved[parent].unwrap().1;
            let base_offset = offset
                .map(|d| duration(s, d, parent_end))
                .transpose()?
                .unwrap_or(0.);
            let local_offset = m.start - resolved[m.action].unwrap().0;
            commands[i].when = QueueTiming::AfterAction {
                action_id: parent_member as u64 + 1,
                seconds: Some(base_offset + local_offset),
                clock: None,
                beats: None,
                quantize: None,
            };
        }
    }
    let recovery = &plan.recovery;
    if recovery.carrier > 1
        || !(0.02..=5.).contains(&recovery.seconds)
        || recovery.commands.is_empty()
        || recovery.commands.len() > 8
    {
        return Err(invalid(
            "invalid_arguments",
            "Recovery requires a carrier, 0.02..5 seconds and 1..8 controls",
        ));
    }
    let mut recovery_endpoints = [None; crate::CONTROL_TARGET_COUNT];
    let mut recovery_targets = 0;
    for command in &recovery.commands {
        command
            .validate()
            .map_err(|e| invalid("invalid_arguments", e))?;
        let targets = command_targets(command);
        if targets == 0 || targets & ((1 << 17) | (1 << 18)) != 0 {
            return Err(invalid(
                "recovery_coverage",
                "Recovery accepts deck fader, gain, EQ, filter and FX controls",
            ));
        }
        if let EngineCommand::Ramp {
            seconds,
            clock,
            beats,
            quantize,
            ..
        } = command
        {
            if *seconds > recovery.seconds
                || clock.is_some()
                || beats.is_some()
                || quantize.is_some()
            {
                return Err(invalid(
                    "recovery_coverage",
                    "Recovery ramps must finish within recovery seconds on the renderer clock",
                ));
            }
        }
        endpoints(command, &mut recovery_endpoints);
        recovery_targets |= targets;
    }
    if recovery_endpoints[ControlTarget::ChannelFader {
        deck: recovery.carrier,
    }
    .index()]
        != Some(1.)
        || recovery_endpoints[ControlTarget::ChannelFader {
            deck: 1 - recovery.carrier,
        }
        .index()]
            != Some(0.)
    {
        return Err(invalid(
            "recovery_coverage",
            "Recovery must fully open its carrier and isolate the other deck",
        ));
    }
    for target in [
        ControlTarget::EqLow {
            deck: recovery.carrier,
        },
        ControlTarget::EqMid {
            deck: recovery.carrier,
        },
        ControlTarget::EqHigh {
            deck: recovery.carrier,
        },
    ] {
        if recovery_endpoints[target.index()].is_none() {
            return Err(invalid(
                "recovery_coverage",
                "Recovery must explicitly declare all three carrier EQ baselines",
            ));
        }
    }
    let coverage = closing_coverage(plan, s, &members, elapsed)?;
    let mut synthetic = s.clone();
    synthetic.scheduled_batch = ScheduledBatchSnapshot {
        id,
        command_queue: true,
        revision: 1,
        state: BatchLifecycle::Armed,
        ..Default::default()
    };
    let patch = compile_command_patch(
        id,
        CommandQueuePatch {
            rebind_dependencies: vec![],
            patch_id: 1,
            expected_revision: 1,
            append: commands,
            invalidate: vec![],
        },
        &synthetic,
    )
    .map_err(|e| invalid("preflight_failed", e))?;
    let mut batch = compile_command_queue(id, s).map_err(|e| invalid("resource_conflict", e))?;
    batch.clock = s
        .decks
        .iter()
        .position(|d| d.playing && !d.sync_enabled)
        .or_else(|| s.decks.iter().position(|d| d.playing))
        .unwrap_or(plan.recovery.carrier);
    if let Some(closing) = &plan.closing {
        batch.expires_frame = plan
            .generation
            .observed_rendered_frames
            .checked_add((closing.endpoint_seconds * s.sample_rate as f64).floor() as u64)
            .ok_or_else(|| {
                invalid(
                    "invalid_arguments",
                    "Closing renderer deadline exceeds the native frame range",
                )
            })?;
    }
    batch.open = false;
    batch.len = patch.len;
    batch.steps = patch.steps;
    batch.carrier = recovery.carrier;
    batch.recovery_seconds = recovery.seconds;
    batch.recovery_targets = recovery_targets;
    batch.targets = recovery_targets;
    batch.arm_before = s.decks[batch.clock].position_seconds
        + starts.iter().copied().fold(f64::INFINITY, f64::min) * s.decks[batch.clock].playback_rate;
    for (i, m) in members.iter().enumerate() {
        if let Some(curve) = m.curve {
            batch.steps[i].as_mut().unwrap().curve_predecessor = members[..i]
                .iter()
                .rposition(|previous| previous.action == m.action && previous.curve == Some(curve));
        }
        let mut dependencies = plan.actions[m.action].after.clone();
        if let Timing::After { action, .. } = &plan.actions[m.action].when {
            dependencies.push(action.clone());
        }
        let mut requirements = 0u64;
        for dependency in &dependencies {
            let action = index[dependency.as_str()];
            for (j, member) in members
                .iter()
                .enumerate()
                .filter(|(_, member)| member.action == action)
            {
                let superseded = members[j + 1..].iter().any(|other| {
                    other.action == action
                        && command_targets(&member.command) & command_targets(&other.command) != 0
                });
                if !superseded {
                    requirements |= 1u64 << j;
                }
            }
        }
        batch.steps[i].as_mut().unwrap().completion_requirements = requirements;
        batch.steps[i].as_mut().unwrap().guard_decks |= 3;
        batch.targets |= command_targets(&m.command);
        batch.receipts[i].step = i;
        batch.receipts[i].action_id = i as u64 + 1;
        if let Some(alignment) = plan.actions[m.action].alignment.filter(|a| a.maintain) {
            if alignment.deck > 1 {
                return Err(invalid("invalid_arguments", "Alignment deck must be A/B"));
            }
            let follower = &s.decks[alignment.deck];
            let limits=follower.sync_rate_limits.filter(|limits|limits.validate().is_ok())
                .filter(|_|follower.sync_enabled&&!follower.is_master&&s.decks[1-alignment.deck].is_master)
                .ok_or_else(||invalid("rhythm_not_prepared","Maintained alignment requires the selected follower's prepared native Sync, explicit rate/slew bounds, and the other deck as master"))?;
            if s.decks.iter().any(|d| {
                !d.beatgrid_valid
                    || d.loop_start_seconds.is_some()
                    || d.jog_touch
                    || d.preparation_pending
            }) {
                return Err(invalid(
                    "rhythm_not_prepared",
                    "Maintained alignment needs ready unlooped, unheld grids on both decks",
                ));
            }
            let (begin, end) = resolved[m.action].unwrap();
            let mut master_playing = s.decks[1 - alignment.deck].playing;
            for prior in members.iter().filter(|other| other.start < begin) {
                match prior.command {
                    EngineCommand::Play { deck } if deck == 1 - alignment.deck => {
                        master_playing = true
                    }
                    EngineCommand::Pause { deck } if deck == 1 - alignment.deck => {
                        master_playing = false
                    }
                    EngineCommand::TogglePlay { deck } if deck == 1 - alignment.deck => {
                        master_playing = !master_playing
                    }
                    _ => {}
                }
            }
            if !master_playing {
                return Err(invalid("rhythm_not_prepared","The maintained overlap requires its prepared master to be playing before its first group"));
            }

            if members.iter().any(|other| {
                other.start < begin
                    && match &other.command {
                        EngineCommand::SetSync {
                            deck,
                            enabled: false,
                        }
                        | EngineCommand::SetTempo { deck, .. }
                        | EngineCommand::SetPlaybackBpm { deck, .. }
                        | EngineCommand::Ramp {
                            target: ControlTarget::TempoPercent { deck },
                            ..
                        } => *deck == alignment.deck,
                        EngineCommand::SetMaster { deck } => *deck != 1 - alignment.deck,
                        EngineCommand::SetSyncRateLimits { deck, limits: next } => {
                            *deck == alignment.deck && *next != Some(limits)
                        }
                        _ => false,
                    }
            }) {
                return Err(invalid("rhythm_control_conflict","Earlier score controls invalidate prepared Sync ownership; prepare and observe before submitting the maintained overlap"));
            }
            if members.iter().any(|other| {
                other.start >= begin - 1e-9
                    && other.start < end
                    && (changes_clock(&other.command, 0)
                        || changes_clock(&other.command, 1)
                        || matches!(other.command, EngineCommand::SetSyncRateLimits { .. }))
            }) {
                return Err(invalid("rhythm_control_conflict","A maintained overlap cannot change clock/Sync/loop state inside its scope; prepare first or place transport edits after actual action completion"));
            }
            if !members[..i].iter().any(|other| other.action == m.action) {
                batch.rhythm[i] = Some(MaintainedAlignment {
                    require: BatchPrerequisite {
                        deck: alignment.deck,
                        playing: Some(true),
                        ready: true,
                        phase_tolerance_beats: Some(alignment.phase_tolerance_beats),
                        bar_beats: alignment.bar_beats,
                        ..Default::default()
                    },
                    limits,
                    members: members
                        .iter()
                        .enumerate()
                        .filter(|(_, other)| other.action == m.action)
                        .fold(0u64, |mask, (j, _)| mask | (1u64 << j)),
                });
            }
        }
        if let Some(alignment) = plan.actions[m.action]
            .alignment
            .filter(|_| (m.start - resolved[m.action].unwrap().0).abs() < 1e-9)
        {
            if alignment.deck > 1
                || !(0.001..=0.5).contains(&alignment.phase_tolerance_beats)
                || alignment.bar_beats.is_some_and(|n| !(2..=16).contains(&n))
            {
                return Err(invalid(
                    "invalid_arguments",
                    "Invalid alignment tolerance or bar length",
                ));
            }
            if alignment.bar_beats.is_some() && s.decks.iter().any(|d| !d.beatgrid_downbeat) {
                return Err(invalid(
                    "bar_origin_unknown",
                    "Bar alignment requires credible bar origins on both decks",
                ));
            }
            if s.decks
                .iter()
                .all(|d| d.playing && !d.sync_enabled && d.loop_start_seconds.is_none())
                && !members.iter().any(|other| {
                    other.start < m.start
                        && (changes_clock(&other.command, 0) || changes_clock(&other.command, 1))
                })
            {
                let a = &s.decks[alignment.deck];
                let b = &s.decks[1 - alignment.deck];
                if let Some((a, b)) = a
                    .grid_position(a.position_seconds + m.start * a.playback_rate)
                    .zip(b.grid_position(b.position_seconds + m.start * b.playback_rate))
                {
                    let period = alignment.bar_beats.map_or(1., f64::from);
                    let offset = (a - b).rem_euclid(period);
                    if offset.min(period - offset) > alignment.phase_tolerance_beats {
                        return Err(invalid(
                            "alignment_failed",
                            format!(
                                "Action {} has predicted {} offset {:.9} beats",
                                plan.actions[m.action].id,
                                if alignment.bar_beats.is_some() {
                                    "bar"
                                } else {
                                    "beat"
                                },
                                offset
                            ),
                        ));
                    }
                }
            }
            batch.steps[i].as_mut().unwrap().require = Some(BatchPrerequisite {
                deck: alignment.deck,
                ready: true,
                // Every atomic member is checked before any command runs. A
                // guarded launch compares the paused entry's phase with the
                // running clock, then starts it in that same group. Alignment
                // gates the action start, not each later curve segment.
                playing: (!members.iter().any(|other| {
                    other.action == m.action
                        && (other.start - m.start).abs() < 1e-9
                        && matches!(other.command, EngineCommand::Play { deck } if deck == alignment.deck)
                }))
                .then_some(true),
                phase_tolerance_beats: Some(alignment.phase_tolerance_beats),
                bar_beats: alignment.bar_beats,
                ..Default::default()
            });
        }
    }
    for (i, c) in recovery.commands.iter().enumerate() {
        batch.recovery[i] = Some(c.clone());
    }
    let actions = plan
        .actions
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let (start, end) = resolved[i].unwrap();
            ResolvedAction {
                id: a.id.clone(),
                start_seconds: start,
                end_seconds: end,
                native_action_ids: native_ids[i].clone(),
            }
        })
        .collect();
    Ok(CompiledPerformance {
        validation: Validation {
            valid: true,
            operation_id: plan.operation_id.clone(),
            actions,
            native_steps: members.len(),
            coverage_seconds: coverage,
            ticket: None,
            arm_within_seconds: Some(
                (starts.iter().copied().fold(f64::INFINITY, f64::min) - 0.02).max(0.),
            ),
            forecast: Some(forecast::build(plan, s, &members)),
        },
        batch,
    })
}
fn finish_span(action: &Action, s: &EngineSnapshot) -> Result<f64> {
    let seconds = |d: Duration| {
        match d {
        Duration::Seconds {value}=>duration(s,Duration::Seconds {value},0.),
        Duration::Beats {..}=>Err(invalid("finish_timing_unproven","Finish actions use explicit second durations; beat clocks do not define a fixed renderer deadline")),
    }
    };
    let mut span = 0_f64;
    for command in &action.commands {
        command
            .validate()
            .map_err(|e| invalid("invalid_arguments", e))?;
        let length = match command {
            EngineCommand::Ramp {
                seconds: value,
                beats: None,
                quantize: None,
                ..
            } => duration(s, Duration::Seconds { value: *value }, 0.)?,
            EngineCommand::Ramp { .. } => {
                return Err(invalid(
                    "finish_timing_unproven",
                    "Finish ramps use second durations without nested timing",
                ))
            }
            EngineCommand::ScrubTo { duration_ms, .. } => duration_ms / 1000.,
            _ => 0.,
        };
        span = span.max(length + completion_margin(command, s.sample_rate));
    }
    for curve in &action.curves {
        for point in &curve.points {
            span = span.max(seconds(point.at)? + smoothing_margin(s.sample_rate));
        }
    }
    Ok(span)
}
fn smoothing_margin(sample_rate: u32) -> f64 {
    // Every smoothed target spans at most two linear units. Share the renderer's
    // time constant and endpoint tolerance, plus two frames for verification order.
    crate::dsp::CONTROL_SMOOTH_SECONDS * (2. / crate::dsp::CONTROL_ENDPOINT_TOLERANCE as f64).ln()
        + 2. / sample_rate as f64
}
fn completion_margin(command: &EngineCommand, sample_rate: u32) -> f64 {
    let smoothed = command_targets(command) & !((1 << 17) | (1 << 18));
    if smoothed != 0 {
        smoothing_margin(sample_rate)
    } else {
        2. / sample_rate as f64
    }
}
fn event_start(
    s: &EngineSnapshot,
    event: &EventHandle,
    members: &[Member],
    latest: bool,
) -> Result<f64> {
    let exact = event_handle(s, event.deck, event.raw_index)?;
    if exact.source != event.source
        || exact.load_revision != event.load_revision
        || exact.grid_revision != event.grid_revision
    {
        return Err(invalid("stale_event", "Event source/grid changed"));
    }
    if (exact.source_seconds - event.source_seconds).abs() > 1e-9
        || (exact.effective_beat - event.effective_beat).abs() > 1e-9
    {
        return Err(invalid(
            "event_mismatch",
            "Use the exact event handle, without rounded seconds",
        ));
    }
    let deck = &s.decks[event.deck];
    let mut position = deck.position_seconds;
    let mut playing = deck.playing;
    let mut looped = deck.loop_start_seconds.is_some();
    let mut rate = if deck.sync_enabled {
        deck.sync_rate_limits.map(|r|if latest{r.min_rate}else{r.max_rate}).ok_or_else(||invalid("event_timing_unproven","A follower event needs explicit native Sync rate bounds for dependency preflight"))?
    } else {
        deck.playback_rate
    };
    let mut time = 0.;
    let mut known: Vec<_> = members
        .iter()
        .filter(|m| m.command.deck() == Some(event.deck))
        .collect();
    known.sort_by(|a, b| a.start.total_cmp(&b.start));
    for next in known.into_iter().map(Some).chain(std::iter::once(None)) {
        if looped {
            return Err(invalid("event_unreachable","A source event across an active/planned loop requires a fresh observation after loop release"));
        }
        if playing {
            let at = time + (event.source_seconds - position) / rate;
            if at < time - 1e-7 {
                return Err(invalid(
                    "past_event",
                    "Source event has already passed or is skipped by a planned position change",
                ));
            }
            if next.is_none_or(|m| at <= if latest { m.latest_start } else { m.start }) {
                if at <= 0.02 {
                    return Err(invalid(
                        "past_event",
                        "Source event lacks a future arming margin",
                    ));
                }
                return Ok(at);
            }
        }
        let Some(m) = next else { break };
        if playing {
            position += ((if latest { m.latest_start } else { m.start }) - time) * rate;
        }
        time = if latest { m.latest_start } else { m.start };
        match &m.command{
            EngineCommand::Play{..}|EngineCommand::PlayQuantized{..}=>playing=true,
            EngineCommand::Pause{..}=>playing=false,
            EngineCommand::Seek{seconds,..}=>position=*seconds,
            EngineCommand::SetTempo{percent,..}=>rate=1.+percent/100.,
            EngineCommand::SetLoop{beats,..}=>looped=beats.is_some(),
            command if changes_clock(command,event.deck)=>return Err(invalid("event_timing_unproven","A source event crosses an unmodelled position/rate edit; prepare it first or use a completion-relative gesture")),
            _=>{}
        }
    }
    Err(invalid(
        "event_unreachable",
        "Clock is paused and no earlier authored launch makes this source event reachable",
    ))
}
fn changes_clock(c: &EngineCommand, clock: usize) -> bool {
    if matches!(c, EngineCommand::SetMaster { .. }) {
        return true;
    }
    if c.deck() != Some(clock) {
        return false;
    }
    matches!(
        c,
        EngineCommand::Ramp {
            target: ControlTarget::TempoPercent { .. },
            ..
        } | EngineCommand::SetTempo { .. }
            | EngineCommand::SetPlaybackBpm { .. }
            | EngineCommand::SetSync { .. }
            | EngineCommand::Sync { .. }
            | EngineCommand::Seek { .. }
            | EngineCommand::BeatJump { .. }
            | EngineCommand::SetLoop { .. }
            | EngineCommand::LoopIn { .. }
            | EngineCommand::LoopOut { .. }
            | EngineCommand::SetMaster { .. }
            | EngineCommand::TogglePlay { .. }
            | EngineCommand::CuePress { .. }
            | EngineCommand::ScrubTo { .. }
            | EngineCommand::ScrubEnd { .. }
            | EngineCommand::MemoryCueNext { .. }
            | EngineCommand::MemoryCuePrevious { .. }
            | EngineCommand::ToggleActiveLoop { .. }
            | EngineCommand::Pause { .. }
            | EngineCommand::SetBpm { .. }
            | EngineCommand::SetBeatgrid { .. }
            | EngineCommand::Jog { .. }
            | EngineCommand::TriggerHotCue { .. }
            | EngineCommand::Cue { .. }
    )
}
fn closing_coverage(
    plan: &Plan,
    s: &EngineSnapshot,
    members: &[Member],
    elapsed: f64,
) -> Result<Option<f64>> {
    let Some(closing) = &plan.closing else {
        return Ok(None);
    };
    if closing.carrier > 1
        || !closing.endpoint_seconds.is_finite()
        || closing.endpoint_seconds <= 0.
        || closing.endpoint_seconds > 86400.
    {
        return Err(invalid(
            "invalid_arguments",
            "Closing endpoint must be within 0..86400 renderer seconds and carrier 0/A or 1/B",
        ));
    }
    let end = closing.endpoint_seconds - elapsed;
    if end <= 0. {
        return Err(invalid(
            "closing_deadline",
            "The observed closing endpoint has already passed",
        ));
    }
    if members
        .iter()
        .any(|m| m.end + completion_margin(&m.command, s.sample_rate) > end + 1e-7)
    {
        return Err(invalid(
            "closing_deadline",
            "An action or fade finishes after the declared endpoint",
        ));
    }
    let music_until = closing.silence_from_seconds.map_or(end, |n| n - elapsed);
    if !(0. ..=end).contains(&music_until) {
        return Err(invalid(
            "invalid_arguments",
            "Intentional silence must fall within the closing endpoint",
        ));
    }
    if members.iter().any(|m| {
        m.start < music_until - 1e-7
            && m.latest_start - m.start > 1e-7
            && changes_clock(&m.command, closing.carrier)
    }) {
        return Err(invalid("coverage_unproven","Closing carrier rate/position edits have uncertain source-event arrival; prepare the edit and observe again before proving endpoint coverage"));
    }
    let deck = &s.decks[closing.carrier];
    let coverage_rate = if deck.sync_enabled {
        deck.sync_rate_limits.map(|b|b.max_rate).ok_or_else(||invalid("coverage_unproven","Declare native Sync rate limits or settle Sync before requesting bounded closing coverage"))?
    } else {
        deck.playback_rate
    };
    let mut position = deck.position_seconds;
    let mut rate = coverage_rate;
    let mut playing = deck.playing;
    let mut looped = deck.loop_start_seconds.is_some();
    let mut loop_end = deck.loop_end_seconds;
    let mut time = 0.;
    let track = deck
        .track
        .as_ref()
        .ok_or_else(|| invalid("coverage_missing", "Closing carrier is empty"))?;
    for m in members
        .iter()
        .filter(|m| m.start <= music_until && m.command.deck() == Some(closing.carrier))
    {
        if !playing && m.start > time && time > 0. {
            return Err(invalid(
                "closing_coverage",
                "Closing carrier has an unplanned paused interval",
            ));
        }
        if playing && !looped {
            position += (m.start - time) * rate;
        }
        if position > track.duration_seconds + 1e-7 {
            return Err(invalid(
                "closing_coverage",
                "Closing carrier reaches EOF before its next planned action",
            ));
        }
        time = m.start;
        match &m.command {
            EngineCommand::Play{..}|EngineCommand::PlayQuantized{..}=>playing=true,
            EngineCommand::Pause{..}=>playing=false,
            EngineCommand::Seek{seconds,..}=>position=*seconds,
            EngineCommand::SetTempo{percent,..}=>rate=1.+percent/100.,
            EngineCommand::SetLoop{beats,..}=>{
                if looped{position=loop_end.ok_or_else(||invalid("coverage_unproven","Active loop has no known source endpoint"))?;}
                if let Some(beats)=beats{
                    let beat=deck.grid_position(position).ok_or_else(||invalid("coverage_unproven","Loop coverage requires a prepared grid"))?.floor();
                    loop_end=Some(deck.grid_time(beat+beats).ok_or_else(||invalid("coverage_unproven","Loop endpoint cannot be projected"))?.min(track.duration_seconds));
                    looped=true;
                }else{looped=false;loop_end=None;}
            },
            EngineCommand::SetSync{enabled:true,..}=>{rate=deck.sync_rate_limits.map(|b|b.max_rate).ok_or_else(||invalid("coverage_unproven","Closing Sync requires native rate bounds"))?;},
            EngineCommand::Ramp{target:ControlTarget::TempoPercent{..},..}|EngineCommand::Sync{..}|EngineCommand::ToggleActiveLoop{..}|EngineCommand::BeatJump{..}|EngineCommand::Jog{..}|EngineCommand::SetPlaybackBpm{..}=>return Err(invalid("coverage_unproven","Closing coverage with variable-rate Sync, tempo curves or implicit position changes needs a new observation after preparation")),
            _=>{}
        }
    }
    if !playing && music_until > time {
        return Err(invalid(
            "closing_coverage",
            "Closing carrier is paused before required coverage ends",
        ));
    }
    if playing && !looped {
        position += (music_until - time) * rate;
    }
    if position > track.duration_seconds + 1e-7 {
        return Err(invalid(
            "closing_coverage",
            format!(
                "Closing carrier is short by {:.6} renderer seconds",
                (position - track.duration_seconds) / rate
            ),
        ));
    }
    let mut fader = deck.channel_fader;
    for m in members
        .iter()
        .filter(|m| m.command.deck() == Some(closing.carrier) && m.start < music_until)
    {
        match &m.command {
            EngineCommand::SetChannelFader { value, .. } => {
                if *value == 0. && m.start < music_until - 1e-7 {
                    return Err(invalid(
                        "closing_coverage",
                        "Carrier fader is zero before required musical coverage ends",
                    ));
                }
                fader = *value;
            }
            EngineCommand::Ramp {
                target: ControlTarget::ChannelFader { .. },
                value,
                ..
            } => {
                if fader == 0. && *value == 0. && m.end < music_until {
                    return Err(invalid(
                        "closing_coverage",
                        "Carrier fader remains zero during required coverage",
                    ));
                }
                let fade_complete = if matches!(plan.actions[m.action].when, Timing::Finish { .. })
                {
                    m.end + completion_margin(&m.command, s.sample_rate)
                } else {
                    m.end
                };
                if *value == 0. && fade_complete < music_until - 1e-7 {
                    return Err(invalid(
                        "closing_coverage",
                        "Fade reaches silence before the declared silence/endpoint",
                    ));
                }
                fader = *value;
            }
            _ => {}
        }
    }
    if deck.channel_fader==0.&&members.iter().all(|m|!matches!(&m.command,
        EngineCommand::Ramp{target:ControlTarget::ChannelFader{deck},value,..}|EngineCommand::SetChannelFader{deck,value}
        if *deck==closing.carrier&&*value>0.&&m.start<music_until)){
        return Err(invalid("closing_coverage","Closing carrier is isolated for required coverage"));
    }
    Ok(Some(music_until))
}
