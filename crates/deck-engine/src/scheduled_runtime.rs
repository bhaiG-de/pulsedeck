// Included into dsp.rs to keep access to the existing ramp/control internals.
impl Mixer {
    fn batch_rate_changed(&self, batch: &crate::scheduled::NativeBatch) -> bool {
        self.decks.iter().enumerate().any(|(i, d)| {
            // The native sync servo owns a follower's changing local slope on
            // nonuniform grids. External tempo/sync/master edits still take
            // over through command() and its control revision guard.
            let native_follower =
                i != batch.clock && self.master_deck == batch.clock && d.sync_enabled;
            !native_follower && (d.rate() - batch.rates[i]).abs() > 1e-6
        })
    }
    pub fn arm_batch(
        &mut self,
        mut batch: Box<crate::scheduled::NativeBatch>,
    ) -> Option<Box<crate::scheduled::NativeBatch>> {
        use crate::scheduled::{BatchLifecycle as L, BatchReason as R};
        let reason = if self.scheduled.as_ref().is_some_and(|b| {
            b.state.active()
                && !(b.command_queue && b.receipts[..b.len].iter().all(|r| r.state >= 2))
        }) || !batch.command_queue && self.ramps.iter().any(Option::is_some)
            || self.launches.iter().any(Option::is_some)
        {
            R::ResourceConflict
        } else if !batch.command_queue && self.load_revisions != batch.sources {
            R::SourceChanged
        } else if !batch.command_queue
            && self.decks.each_ref().map(|d| d.position_revision) != batch.positions
        {
            R::PositionChanged
        } else if !batch.command_queue && self.grid_revisions != batch.grids {
            R::GridChanged
        } else if !batch.command_queue && self.control_revision != batch.control_revision {
            R::ManualTakeover
        } else if !batch.command_queue
            && (self.batch_rate_changed(&batch)
                || self
                    .decks
                    .iter()
                    .any(|d| d.jog_touch || d.loop_region.is_some() || d.preparation_pending()))
        {
            R::ResourceConflict
        } else if !batch.command_queue && !self.decks[batch.clock].playing {
            R::ClockStopped
        } else if self.engine_epoch != batch.engine_epoch || self.sample_rate != batch.sample_rate {
            R::AudioStopped
        } else if self.decks[batch.clock].seconds() >= batch.arm_before {
            R::DeadlineExpired
        } else {
            R::None
        };
        batch.frame = self.rendered_frames;
        if reason != R::None {
            batch.state = L::Failed;
            batch.reason = reason;
        }
        // Only one native execution slot. Rejected overlapping arms must not
        // displace the first owner's controls or history.
        if reason == R::ResourceConflict
            && self.scheduled.as_ref().is_some_and(|b| {
                b.state.active()
                    && !(b.command_queue && b.receipts[..b.len].iter().all(|r| r.state >= 2))
            })
        {
            return Some(batch);
        }
        self.scheduled.replace(batch)
    }
    fn validate_dependency_rebinds(
        &self,
        b: &crate::scheduled::NativeBatch,
        p: &crate::scheduled::CompiledQueuePatch,
    ) -> Result<(), crate::BatchReason> {
        use crate::scheduled::{BatchReason as R, QueueTiming, MAX_SCHEDULED_STEPS};
        let invalid = &p.invalidate[..p.invalidate_len];
        let rebinds = &p.rebind_dependencies[..p.rebind_len];
        for (i, &(old, new)) in rebinds.iter().enumerate() {
            if old == new
                || rebinds[..i].iter().any(|&(prior, _)| prior == old)
                || !invalid.contains(&old)
                || !b.receipts[..b.len]
                    .iter()
                    .any(|r| r.action_id == old && r.state == 0)
                || !p.steps[..p.len]
                    .iter()
                    .flatten()
                    .any(|step| step.action_id == new)
            {
                return Err(R::InvalidPatch);
            }
            for j in 0..b.len {
                let step = b.steps[j].as_ref().unwrap();
                if step.dependency_id == old
                    && !invalid.contains(&step.action_id)
                    && (b.receipts[j].state != 0
                        || !matches!(step.dependency, Some(QueueTiming::AfterAction { .. })))
                {
                    return Err(R::InvalidPatch);
                }
            }
        }
        // Construct the prospective dependency graph on the stack. Removed
        // actions are absent, completed/resolved actions are terminal anchors.
        let mut ids = [0; MAX_SCHEDULED_STEPS];
        let mut dependencies = [0; MAX_SCHEDULED_STEPS];
        let mut len = 0;
        for i in 0..b.len {
            let step = b.steps[i].as_ref().unwrap();
            if invalid.contains(&step.action_id) || b.receipts[i].state >= 3 {
                continue;
            }
            ids[len] = step.action_id;
            if b.receipts[i].state == 0 && step.dependency.is_some() {
                dependencies[len] = rebinds
                    .iter()
                    .find(|&&(old, _)| old == step.dependency_id)
                    .map_or(step.dependency_id, |&(_, new)| new);
            }
            len += 1;
        }
        for step in p.steps[..p.len].iter().flatten() {
            ids[len] = step.action_id;
            dependencies[len] = step.dependency_id;
            len += 1;
        }
        for i in 0..len {
            let mut next = dependencies[i];
            let mut hops = 0;
            while next != 0 {
                if next == ids[i] || hops >= len {
                    return Err(R::InvalidPatch);
                }
                let Some(index) = ids[..len].iter().position(|&id| id == next) else {
                    return Err(R::InvalidPatch);
                };
                next = dependencies[index];
                hops += 1;
            }
        }
        Ok(())
    }
    pub fn patch_batch(
        &mut self,
        mut p: Box<crate::scheduled::CompiledQueuePatch>,
    ) -> Box<crate::scheduled::CompiledQueuePatch> {
        use crate::scheduled::{
            BatchLifecycle as L, BatchPatchReceipt, BatchReason as R, MAX_QUEUE_UPDATES,
            MAX_SCHEDULED_STEPS,
        };
        let Some(mut b) = self.scheduled.take() else {
            return p;
        };
        if b.updates
            .iter()
            .zip(&b.patch_identities)
            .any(|(r, identity)| {
                r.patch_id == p.patch_id && identity.as_deref() == p.identity.as_deref()
            })
        {
            self.scheduled = Some(b);
            return p;
        }
        // All checks precede mutation: rejected CAS/guards preserve pending work.
        let result = (|| -> Result<(), R> {
            if b.id != p.id
                || !(matches!(b.state, L::Armed | L::Running)
                    || b.command_queue && b.state == L::Completed)
            {
                return Err(R::QueueInactive);
            }
            if !b.open {
                return Err(R::QueueSealed);
            }
            if p.patch_id <= b.last_patch_id {
                return Err(R::IdConflict);
            }
            if b.revision != p.expected_revision {
                return Err(R::RevisionConflict);
            }
            if self.engine_epoch != p.engine_epoch || self.sample_rate != p.sample_rate {
                return Err(R::AudioStopped);
            }
            if !b.command_queue && (self.load_revisions != p.sources || b.sources != p.sources) {
                return Err(R::SourceChanged);
            }
            if !b.command_queue && (self.grid_revisions != p.grids || b.grids != p.grids) {
                return Err(R::GridChanged);
            }
            if !b.command_queue
                && (self.decks.each_ref().map(|d| d.position_revision) != p.positions
                    || b.positions != p.positions)
            {
                return Err(R::PositionChanged);
            }
            if !b.command_queue
                && (self.control_revision != p.control_revision
                    || b.control_revision != p.control_revision)
            {
                return Err(R::ManualTakeover);
            }
            if !b.command_queue && self.batch_rate_changed(&b) {
                return Err(R::TempoChanged);
            }
            if b.command_queue {
                let reason = self.active_command_guard_reason(&b);
                if reason != R::None {
                    return Err(reason);
                }
                let invalid = &p.invalidate[..p.invalidate_len];
                for id in invalid {
                    let Some(i) = b.receipts[..b.len]
                        .iter()
                        .position(|r| r.action_id == *id && r.state == 0)
                    else {
                        return Err(R::InvalidPatch);
                    };
                    for j in 0..b.len {
                        if b.receipts[j].state == 0
                            && (Self::same_command_group(
                                b.steps[i].as_ref().unwrap(),
                                b.steps[j].as_ref().unwrap(),
                            ) || b.steps[j].as_ref().unwrap().dependency_id == *id
                                && !p.rebind_dependencies[..p.rebind_len]
                                    .iter()
                                    .any(|&(old, _)| old == *id))
                            && !invalid.contains(&b.receipts[j].action_id)
                        {
                            return Err(R::InvalidPatch);
                        }
                    }
                }
                if b.len + p.len > MAX_SCHEDULED_STEPS {
                    return Err(R::CapacityExceeded);
                }
                let owned_targets = crate::scheduled::active_ownership(&b).1;
                for (index, step) in p.steps[..p.len].iter().flatten().enumerate() {
                    let reason = self.command_step_guard_reason(step);
                    if reason != R::None {
                        return Err(reason);
                    }
                    if let Some(group_id) = step.group_id {
                        if b.steps[..b.len]
                            .iter()
                            .flatten()
                            .any(|old| old.group_id == Some(group_id))
                        {
                            return Err(R::IdConflict);
                        }
                    }
                    let targets = crate::scheduled::command_targets(&step.command);
                    if self.ramps.iter().enumerate().any(|(i, r)| {
                        r.is_some() && targets & (1 << i) != 0 && owned_targets & (1 << i) == 0
                    }) {
                        return Err(R::ResourceConflict);
                    }
                    if step.action_id == 0
                        || b.receipts[..b.len]
                            .iter()
                            .any(|receipt| receipt.action_id == step.action_id)
                        || p.steps[..index]
                            .iter()
                            .flatten()
                            .any(|prior| prior.action_id == step.action_id)
                    {
                        return Err(R::IdConflict);
                    }
                }
                self.validate_dependency_rebinds(&b, &p)?;
                return Ok(());
            }
            let now = self.decks[b.clock].seconds();
            if !self.decks[b.clock].playing
                || self.rendered_frames >= b.expires_frame
                || now >= b.complete_at
            {
                return Err(R::DeadlineExpired);
            }
            if b.len + p.len > MAX_SCHEDULED_STEPS {
                return Err(R::CapacityExceeded);
            }
            let invalid = &p.invalidate[..p.invalidate_len];
            for id in invalid {
                let Some(i) = b.receipts[..b.len]
                    .iter()
                    .position(|r| r.action_id == *id && r.state == 0)
                else {
                    return Err(R::InvalidPatch);
                };
                let at = b.steps[i].as_ref().unwrap().at;
                // Same-event groups are immutable units. Removing a launch must
                // also remove every downstream pending action in this queue.
                let launch = matches!(
                    b.steps[i].as_ref().unwrap().command,
                    crate::EngineCommand::PlayQuantized { .. }
                );
                for j in 0..b.len {
                    if b.receipts[j].state == 0
                        && (b.steps[j].as_ref().unwrap().at == at || launch && j > i)
                        && !invalid.contains(&b.receipts[j].action_id)
                    {
                        return Err(R::InvalidPatch);
                    }
                }
            }
            let mut last_at = now;
            let mut last_id = b.receipts[..b.len]
                .iter()
                .map(|r| r.action_id)
                .max()
                .unwrap_or(0);
            let mut occupied = [f64::NEG_INFINITY; crate::CONTROL_TARGET_COUNT];
            let mut expected = [None; crate::CONTROL_TARGET_COUNT];
            for i in 0..b.len {
                if b.receipts[i].state == 3 || invalid.contains(&b.receipts[i].action_id) {
                    continue;
                }
                let step = b.steps[i].as_ref().unwrap();
                last_at = last_at.max(step.at);
                let targets = crate::scheduled::command_targets(&step.command);
                for (j, until) in occupied.iter_mut().enumerate() {
                    if targets & (1 << j) != 0 {
                        *until = step.end;
                    }
                }
                crate::scheduled::endpoints(&step.command, &mut expected);
            }
            for step in p.steps[..p.len].iter().flatten() {
                if step.action_id <= last_id {
                    return Err(R::IdConflict);
                }
                if step.at < last_at
                    || step.at <= now + 2. * b.rates[b.clock] / self.sample_rate as f64
                {
                    return Err(R::DeadlineExpired);
                }
                last_at = step.at;
                last_id = step.action_id;
                let targets = crate::scheduled::command_targets(&step.command);
                if targets & !b.recovery_targets != 0 {
                    return Err(R::InvalidPatch);
                }
                for (j, until) in occupied.iter_mut().enumerate() {
                    if targets & (1 << j) != 0 {
                        if step.at + 1e-8 < *until {
                            return Err(R::ResourceConflict);
                        }
                        *until = step.end;
                    }
                }
                crate::scheduled::endpoints(&step.command, &mut expected);
            }
            if let Some((_, at)) = p.seal {
                if at <= now || at > b.complete_at {
                    return Err(R::DeadlineExpired);
                }
                if (0..b.len).any(|i| {
                    b.receipts[i].state != 3
                        && !invalid.contains(&b.receipts[i].action_id)
                        && b.steps[i].as_ref().unwrap().end >= at
                }) || p.steps[..p.len].iter().flatten().any(|s| s.end >= at)
                {
                    return Err(R::InvalidPatch);
                }
                let carrier = crate::ControlTarget::ChannelFader {
                    deck: b.completion_carrier,
                }
                .index();
                let other = crate::ControlTarget::ChannelFader {
                    deck: 1 - b.completion_carrier,
                }
                .index();
                if expected[carrier] != Some(1.) || expected[other] != Some(0.) {
                    return Err(R::EndpointFailed);
                }
            }
            Ok(())
        })();
        let reason = match result {
            Err(reason) => reason,
            Ok(()) => {
                for step in b.steps[..b.len].iter_mut().flatten() {
                    if let Some(&(_, new)) = p.rebind_dependencies[..p.rebind_len]
                        .iter()
                        .find(|&&(old, _)| old == step.dependency_id)
                    {
                        step.dependency_id = new;
                        if let Some(crate::scheduled::QueueTiming::AfterAction {
                            action_id, ..
                        }) = &mut step.dependency
                        {
                            *action_id = new;
                        }
                    }
                }
                for i in 0..b.len {
                    if p.invalidate[..p.invalidate_len].contains(&b.receipts[i].action_id) {
                        b.receipts[i].state = 3;
                        b.receipts[i].completed_frame = self.rendered_frames;
                    }
                }
                for i in 0..p.len {
                    let step = p.steps[i].take().unwrap();
                    b.targets |= crate::scheduled::command_targets(&step.command);
                    if let crate::EngineCommand::PlayQuantized { deck, .. } = step.command {
                        b.launched[deck] = true;
                    }
                    let slot = b.len;
                    b.receipts[slot] = crate::scheduled::BatchEventReceipt {
                        step: slot,
                        action_id: step.action_id,
                        ..Default::default()
                    };
                    b.steps[slot] = Some(step);
                    b.len += 1;
                    self.capture_command_resolution(&mut b, slot);
                }
                b.expected_endpoints = [None; crate::CONTROL_TARGET_COUNT];
                for i in 0..b.len {
                    if b.receipts[i].state != 3 {
                        crate::scheduled::endpoints(
                            &b.steps[i].as_ref().unwrap().command,
                            &mut b.expected_endpoints,
                        );
                    }
                }
                if let Some((event, at)) = p.seal {
                    b.open = false;
                    b.complete_at = at;
                    b.completion_event = event;
                }
                b.revision += 1;
                if b.command_queue {
                    b.state = L::Armed;
                    b.reason = R::None;
                }
                R::None
            }
        };
        // Bounded ring retains recent patch outcomes; reusing old IDs never
        // executes again, including after an acknowledgement was lost.
        b.last_patch_id = b.last_patch_id.max(p.patch_id);
        let slot = b.update_count % MAX_QUEUE_UPDATES;
        let old_identity = std::mem::replace(&mut b.patch_identities[slot], p.identity.take());
        p.identity = old_identity;
        b.updates[slot] = BatchPatchReceipt {
            patch_id: p.patch_id,
            revision: b.revision,
            accepted: result.is_ok(),
            reason,
            frame: self.rendered_frames,
        };
        b.update_count += 1;
        b.frame = self.rendered_frames;
        self.scheduled = Some(b);
        p
    }
    fn verify_batch_actions(&mut self, b: &mut crate::scheduled::NativeBatch) {
        for i in 0..b.completed {
            if b.receipts[i].state != 1 {
                continue;
            }
            let step = b.steps[i].as_ref().unwrap();
            let verified = if let crate::EngineCommand::PlayQuantized { deck, .. } = step.command {
                self.decks[deck].playing && self.launches[deck].is_none()
            } else {
                let mut expected = [None; crate::CONTROL_TARGET_COUNT];
                crate::scheduled::endpoints(&step.command, &mut expected);
                self.batch_endpoints(&expected)
            };
            if verified {
                b.receipts[i].state = 2;
                b.receipts[i].completed_frame = self.rendered_frames;
                b.receipts[i].completed_clock_seconds = self.decks.each_ref().map(|d| d.seconds());
                b.receipts[i].completed_load_revisions = self.load_revisions;
                b.receipts[i].completed_grid_revisions = self.grid_revisions;
                b.receipts[i].completed_position_revisions =
                    self.decks.each_ref().map(|d| d.position_revision);
                b.frame = self.rendered_frames;
            }
        }
    }
    fn cancel_owned(&mut self, b: &crate::scheduled::NativeBatch) {
        let targets = crate::scheduled::active_ownership(b).1;
        for i in 0..crate::CONTROL_TARGET_COUNT {
            if targets & (1 << i) != 0 {
                self.ramps[i] = None;
            }
        }
        for i in 0..2 {
            if b.launched[i] {
                self.launches[i] = None;
            }
        }
    }
    fn cancel_batch_receipts(&self, b: &mut crate::scheduled::NativeBatch) {
        for receipt in &mut b.receipts[..b.len] {
            if receipt.state == 0 || receipt.state == 1 {
                receipt.state = 3;
                receipt.completed_frame = self.rendered_frames;
            }
        }
    }
    pub fn cancel_batch(&mut self, id: u64, stabilize: bool) {
        use crate::scheduled::{BatchLifecycle as L, BatchReason as R};
        let Some(mut b) = self.scheduled.take() else {
            return;
        };
        if b.id == id && b.state.active() {
            if stabilize {
                self.recover_batch(&mut b, R::CancelRequested);
            } else {
                self.cancel_owned(&b);
                self.cancel_batch_receipts(&mut b);
                b.state = L::Cancelled;
                b.reason = R::CancelRequested;
                b.frame = self.rendered_frames;
            }
        }
        self.scheduled = Some(b);
    }
    pub fn interrupt_batch(&mut self) {
        use crate::scheduled::{BatchLifecycle as L, BatchReason as R};
        if let Some(mut b) = self.scheduled.take() {
            if b.state.active() {
                self.cancel_owned(&b);
                self.cancel_batch_receipts(&mut b);
                b.state = L::Interrupted;
                b.reason = R::AudioStopped;
                b.frame = self.rendered_frames;
            }
            self.scheduled = Some(b);
        }
    }
    fn manual_batch_takeover(&mut self, listening_only: bool, targets: u32, decks: u8) {
        use crate::scheduled::{BatchLifecycle as L, BatchReason as R};
        // Headphone listening is independent of source/transport ownership.
        // Its exact target still takes over a queue that actually owns it.
        let decks = if listening_only { 0 } else { decks };
        self.control_revision = self.control_revision.wrapping_add(1);
        for (i, revision) in self.deck_control_revisions.iter_mut().enumerate() {
            if decks & (1 << i) != 0 {
                *revision = revision.wrapping_add(1);
            }
        }
        for (i, revision) in self.target_control_revisions.iter_mut().enumerate() {
            if targets & (1 << i) != 0 {
                *revision = revision.wrapping_add(1);
            }
        }
        if let Some(mut b) = self.scheduled.take() {
            if b.state.active() || b.command_queue && b.state == L::Completed {
                let (owned_decks, owned_targets) = crate::scheduled::active_ownership(&b);
                let unrelated = if b.command_queue {
                    owned_decks & decks == 0 && owned_targets & targets == 0
                } else {
                    listening_only && b.targets & targets == 0
                };
                if unrelated {
                    b.control_revision = self.control_revision;
                } else {
                    self.cancel_owned(&b);
                    self.cancel_batch_receipts(&mut b);
                    b.state = L::Yielded;
                    b.reason = R::ManualTakeover;
                    b.frame = self.rendered_frames;
                }
            }
            self.scheduled = Some(b);
        }
    }
    fn batch_endpoints(&mut self, expected: &[Option<f32>; crate::CONTROL_TARGET_COUNT]) -> bool {
        for (i, e) in expected.iter().enumerate() {
            if let Some(e) = e {
                let t = crate::ControlTarget::from_index(i);
                let target = self.target_value(t);
                let settled = match t {
                    crate::ControlTarget::TempoPercent { .. } => true,
                    _ => (self.target_mut(t).value - e).abs() <= 0.002,
                };
                if (target - e).abs() > 1e-4 || !settled || self.ramps[i].is_some() {
                    return false;
                }
            }
        }
        true
    }
    fn recover_batch(
        &mut self,
        b: &mut crate::scheduled::NativeBatch,
        reason: crate::scheduled::BatchReason,
    ) {
        use crate::scheduled::{BatchLifecycle as L, BatchReason as R};
        self.cancel_owned(b);
        b.reason = reason;
        b.frame = self.rendered_frames;
        for r in &mut b.receipts[..b.len] {
            if r.state == 0 || r.state == 1 {
                r.state = 3;
                r.completed_frame = self.rendered_frames;
            }
        }
        // A source/position/grid/manual change always yields; a restoration
        // from the old plan must never override the DJ's new state.
        if matches!(
            reason,
            R::SourceChanged
                | R::PositionChanged
                | R::GridChanged
                | R::TempoChanged
                | R::ManualTakeover
        ) {
            b.state = L::Yielded;
            return;
        }
        if b.command_queue {
            b.state = L::Failed;
            return;
        }
        let carrier = &self.decks[b.carrier];
        if !carrier.playing || carrier.preparation_pending() || carrier.track.is_none() {
            // Before an isolated start there is nothing to restore. Preserve
            // the healthy outgoing track and make failure explicit.
            b.state = L::Failed;
            return;
        }
        b.state = L::Recovering;
        b.expected_endpoints = [None; crate::CONTROL_TARGET_COUNT];
        for c in b.recovery.iter().flatten() {
            crate::scheduled::endpoints(c, &mut b.expected_endpoints);
            self.command(c.clone());
        }
        b.recovery_end = self.rendered_frames
            + (b.recovery_seconds * self.sample_rate as f64).ceil() as u64
            + (self.sample_rate as f64 * 0.08) as u64;
    }
    fn batch_prerequisite(&self, r: crate::scheduled::BatchPrerequisite) -> bool {
        let d = &self.decks[r.deck];
        if r.playing.is_some_and(|v| d.playing != v) || r.ready && d.preparation_pending() {
            return false;
        }
        if let Some(from) = r.advanced_from_seconds {
            if d.grid_position(d.seconds()) - d.grid_position(from) < r.min_advanced_beats {
                return false;
            }
        }
        if let Some(tol) = r.phase_tolerance_beats {
            let other = &self.decks[1 - r.deck];
            let n = r.bar_beats.map_or(1., |n| n as f64);
            let p =
                (d.grid_position(d.seconds()) - other.grid_position(other.seconds())).rem_euclid(n);
            if p.min(n - p) > tol {
                return false;
            }
        }
        if let Some(event) = r.arrival_before_event {
            let beat = if d.grid_uniform {
                event as f64
            } else {
                event as f64 - d.grid_anchor_index as f64
            };
            if d.seconds() >= d.grid_time(beat) {
                return false;
            }
        }
        true
    }
    fn advance_batch(&mut self) {
        use crate::scheduled::{BatchLifecycle as L, BatchReason as R};
        let Some(mut b) = self.scheduled.take() else {
            return;
        };
        if !b.state.active() {
            self.scheduled = Some(b);
            return;
        }
        self.batch_internal = true;
        if b.command_queue {
            self.advance_command_queue(&mut b);
            self.batch_internal = false;
            self.scheduled = Some(b);
            return;
        }
        let reason = if self.load_revisions != b.sources {
            R::SourceChanged
        } else if self.decks.each_ref().map(|d| d.position_revision) != b.positions {
            R::PositionChanged
        } else if self.grid_revisions != b.grids {
            R::GridChanged
        } else if self.batch_rate_changed(&b) {
            R::TempoChanged
        } else if self.sample_rate != b.sample_rate {
            R::AudioStopped
        } else {
            R::None
        };
        if reason != R::None {
            self.recover_batch(&mut b, reason);
        } else if b.state == L::Recovering {
            if !self.decks[b.carrier].playing {
                self.cancel_owned(&b);
                b.state = L::Failed;
                b.reason = R::RecoveryUnavailable;
            } else if self.rendered_frames >= b.recovery_end {
                b.state = if self.batch_endpoints(&b.expected_endpoints) {
                    L::Recovered
                } else {
                    L::Failed
                };
                if b.state == L::Failed {
                    b.reason = R::EndpointFailed;
                }
            }
        } else if self.rendered_frames >= b.expires_frame {
            self.recover_batch(&mut b, R::DeadlineExpired);
        } else if !self.decks[b.clock].playing {
            self.recover_batch(&mut b, R::ClockStopped);
        } else {
            let now = self.decks[b.clock].seconds();
            self.advance_ramps();
            self.verify_batch_actions(&mut b);
            while b.completed < b.len && b.receipts[b.completed].state == 3 {
                b.completed += 1;
            }
            if b.completed < b.len && b.steps[b.completed].as_ref().is_some_and(|s| now >= s.at) {
                let start = b.completed;
                let at = b.steps[start].as_ref().unwrap().at;
                let mut end = start;
                while end < b.len && b.steps[end].as_ref().unwrap().at == at {
                    end += 1;
                }
                let mut valid = true;
                for i in start..end {
                    let s = b.steps[i].as_ref().unwrap();
                    if matches!(s.command,EngineCommand::Pause{deck} if !self.decks[b.carrier].playing || self.decks[b.carrier].channel_fader.value<0.98 || deck==b.carrier)
                        || now > s.deadline + 2. * b.rates[b.clock] / self.sample_rate as f64
                        || s.require.is_some_and(|r| !self.batch_prerequisite(r))
                    {
                        valid = false;
                        break;
                    }
                }
                if !valid {
                    let reason = if now
                        > b.steps[start].as_ref().unwrap().deadline
                            + 2. * b.rates[b.clock] / self.sample_rate as f64
                    {
                        R::DeadlineExpired
                    } else {
                        R::PrerequisiteFailed
                    };
                    self.recover_batch(&mut b, reason);
                } else {
                    b.state = L::Running;
                    for i in start..end {
                        let s = b.steps[i].as_ref().unwrap();
                        let mut command = s.command.clone();
                        if let EngineCommand::PlayQuantized { at_seconds, .. } = &mut command {
                            *at_seconds = Some(now);
                        }
                        self.command(command);
                        b.receipts[i] = crate::scheduled::BatchEventReceipt {
                            step: i,
                            action_id: s.action_id,
                            completed_frame: 0,
                            completed_clock_seconds: [0.; 2],
                            completed_load_revisions: [0; 2],
                            completed_grid_revisions: [0; 2],
                            completed_position_revisions: [0; 2],
                            state: 1,
                            frame: self.rendered_frames,
                            clock_seconds: now,
                            ..Default::default()
                        };
                    }
                    b.positions = self.decks.each_ref().map(|d| d.position_revision);
                    b.completed = end;
                    b.frame = self.rendered_frames;
                    if !valid {
                        self.recover_batch(&mut b, R::PrerequisiteFailed);
                    }
                }
            }
            if b.open && (now >= b.complete_at || self.rendered_frames >= b.expires_frame) {
                self.recover_batch(&mut b, R::DeadlineExpired);
            }
            // Verify successful controls independently of inactivity; a ramp
            // cancelled at EOF cannot masquerade as completed.
            if !b.open
                && b.state != L::Recovering
                && b.state.active()
                && now >= b.complete_at
                && b.completed == b.len
            {
                if self.batch_endpoints(&b.expected_endpoints)
                    && self.decks[b.completion_carrier].playing
                    && !self.decks[b.completion_carrier].preparation_pending()
                {
                    b.state = L::Completed;
                    b.frame = self.rendered_frames;
                    if b.pause_outgoing
                        && b.completion_carrier != b.clock
                        && self.decks[b.clock].channel_fader.value < 0.002
                    {
                        self.command(EngineCommand::Pause { deck: b.clock });
                    }
                } else {
                    self.recover_batch(&mut b, R::EndpointFailed);
                }
            }
        }
        self.batch_internal = false;
        self.scheduled = Some(b);
    }
}

impl Mixer {
    fn command_step_guard_reason(
        &self,
        step: &crate::scheduled::CompiledStep,
    ) -> crate::BatchReason {
        use crate::BatchReason as R;
        for deck in 0..2 {
            if step.guard_decks & (1 << deck) != 0 {
                if step.source_revisions[deck] != self.load_revisions[deck] {
                    return R::SourceChanged;
                }
                if step.grid_revisions[deck] != self.grid_revisions[deck] {
                    return R::GridChanged;
                }
                if step.position_revisions[deck] != self.decks[deck].position_revision {
                    return R::PositionChanged;
                }
                if step.deck_control_revisions[deck] != self.deck_control_revisions[deck] {
                    return R::ManualTakeover;
                }
            }
        }
        let targets = crate::scheduled::command_targets(&step.command);
        for i in 0..crate::COMMAND_TARGET_COUNT {
            if targets & (1 << i) != 0
                && step.target_control_revisions[i] != self.target_control_revisions[i]
            {
                return R::ManualTakeover;
            }
        }
        R::None
    }
    fn active_command_guard_reason(&self, b: &crate::scheduled::NativeBatch) -> crate::BatchReason {
        for i in 0..b.len {
            if b.receipts[i].state <= 1 {
                let reason = self.command_step_guard_reason(b.steps[i].as_ref().unwrap());
                if reason != crate::BatchReason::None {
                    return reason;
                }
            }
        }
        crate::BatchReason::None
    }
    fn same_command_group(
        a: &crate::scheduled::CompiledStep,
        b: &crate::scheduled::CompiledStep,
    ) -> bool {
        a.action_id == b.action_id
            || a.group_id.is_some() && a.group_id == b.group_id
            || a.group_id.is_none()
                && b.group_id.is_none()
                && a.grouped
                && b.grouped
                && a.clock == b.clock
                && a.at_frame == b.at_frame
                && a.at == b.at
                && a.dependency == b.dependency
    }
    fn fail_command_queue(
        &mut self,
        b: &mut crate::scheduled::NativeBatch,
        reason: crate::BatchReason,
        group: Option<usize>,
    ) {
        if let Some(i) = group {
            for j in 0..b.len {
                if b.receipts[j].state == 0
                    && Self::same_command_group(
                        b.steps[i].as_ref().unwrap(),
                        b.steps[j].as_ref().unwrap(),
                    )
                {
                    b.receipts[j].state = 4;
                    b.receipts[j].completed_frame = self.rendered_frames;
                }
            }
        }
        self.cancel_owned(b);
        self.cancel_batch_receipts(b);
        b.state = crate::BatchLifecycle::Failed;
        b.reason = reason;
        b.frame = self.rendered_frames;
    }
    fn command_prerequisite_failure(
        &self,
        command: &crate::EngineCommand,
        action_id: u64,
    ) -> Option<crate::scheduled::CommandFailure> {
        use crate::{scheduled::CommandFailureCause as C, EngineCommand::*};
        let failure = |cause, deck| {
            Some(crate::scheduled::CommandFailure {
                cause,
                deck,
                action_id,
            })
        };
        let clock_failure = |deck: usize| {
            if self.decks[deck].natural_eof() {
                C::ClockEof
            } else {
                C::ClockNotPlaying
            }
        };
        match command {
            SetSyncRateLimits {
                deck,
                limits: Some(limits),
            } if !limits.contains(self.decks[*deck].actual_rate()) => {
                failure(C::SyncRateOutsideLimits, Some(*deck))
            }
            Play { deck }
            | Seek { deck, .. }
            | Cue { deck }
            | CuePress { deck, .. }
            | TriggerHotCue { deck, .. }
                if self.decks[*deck].track.is_none() =>
            {
                failure(C::SourceMissing, Some(*deck))
            }
            PlayQuantized {
                deck,
                clock,
                clock_revision,
                ..
            } => {
                let a = &self.decks[*clock];
                let d = &self.decks[*deck];
                if a.track.is_none() {
                    failure(C::SourceMissing, Some(*clock))
                } else if d.track.is_none() {
                    failure(C::SourceMissing, Some(*deck))
                } else if self.load_revisions[*clock] != *clock_revision {
                    failure(C::ClockSourceChanged, Some(*clock))
                } else if !a.playing {
                    failure(clock_failure(*clock), Some(*clock))
                } else if d.playing {
                    failure(C::IncomingAlreadyPlaying, Some(*deck))
                } else if !a.grid_valid {
                    failure(C::GridMissing, Some(*clock))
                } else if !d.grid_valid {
                    failure(C::GridMissing, Some(*deck))
                } else if a.jog_touch {
                    failure(C::JogHeld, Some(*clock))
                } else if d.jog_touch {
                    failure(C::JogHeld, Some(*deck))
                } else if a.loop_region.is_some() {
                    failure(C::LoopActive, Some(*clock))
                } else if d.loop_region.is_some() {
                    failure(C::LoopActive, Some(*deck))
                } else if d.preparation_pending() {
                    failure(C::PreparationPending, Some(*deck))
                } else {
                    None
                }
            }
            Ramp {
                clock: Some(clock), ..
            } => {
                let d = &self.decks[*clock];
                if d.track.is_none() {
                    failure(C::SourceMissing, Some(*clock))
                } else if !d.playing {
                    failure(clock_failure(*clock), Some(*clock))
                } else if !d.grid_valid {
                    failure(C::GridMissing, Some(*clock))
                } else {
                    None
                }
            }
            SetPlaybackBpm { deck, bpm } => {
                if self.decks[*deck].local_grid_bpm().is_none() {
                    failure(C::PlaybackBpmReferenceMissing, Some(*deck))
                } else if crate::playback_tempo_percent(self.decks[*deck].local_grid_bpm(), *bpm)
                    .is_err()
                {
                    failure(C::PlaybackBpmUnreachable, Some(*deck))
                } else {
                    None
                }
            }
            SetSync {
                deck,
                enabled: true,
            }
            | Sync { deck } => {
                let clock = self.master_deck;
                let d = &self.decks[*deck];
                let a = &self.decks[clock];
                if *deck == clock {
                    failure(C::SyncMasterSelf, Some(*deck))
                } else if d.track.is_none() {
                    failure(C::SourceMissing, Some(*deck))
                } else if a.track.is_none() {
                    failure(C::SourceMissing, Some(clock))
                } else if !d.grid_valid {
                    failure(C::GridMissing, Some(*deck))
                } else if !a.grid_valid {
                    failure(C::GridMissing, Some(clock))
                } else if d.playing && !a.playing {
                    failure(clock_failure(clock), Some(clock))
                } else if d.playing && a.jog_touch {
                    failure(C::JogHeld, Some(clock))
                } else {
                    None
                }
            }
            TriggerHotCue { deck, index } if self.decks[*deck].hot_cues[*index].is_none() => {
                failure(C::HotCueMissing, Some(*deck))
            }
            _ => None,
        }
    }
    fn capture_command_resolution(&self, b: &mut crate::scheduled::NativeBatch, i: usize) {
        let step = b.steps[i].as_ref().unwrap();
        if step.dependency.is_some() {
            return;
        }
        let receipt = &mut b.receipts[i];
        receipt.resolved_clock = step.clock;
        receipt.resolved_clock_seconds = step.clock.map(|_| step.at);
        receipt.resolved_frame = step.at_frame.map(|frame| {
            if step.strict_time {
                frame
            } else {
                frame.max(self.rendered_frames)
            }
        });
        receipt.clock_source_headroom_seconds = step.clock.and_then(|clock| {
            self.decks[clock]
                .track
                .as_ref()
                .map(|track| track.analysis.duration_seconds - step.at)
        });
    }
    fn set_command_group_failure(
        &self,
        b: &mut crate::scheduled::NativeBatch,
        i: usize,
        failure: crate::scheduled::CommandFailure,
    ) {
        for j in 0..b.len {
            if b.receipts[j].state <= 1
                && Self::same_command_group(
                    b.steps[i].as_ref().unwrap(),
                    b.steps[j].as_ref().unwrap(),
                )
            {
                b.receipts[j].failure = Some(failure);
            }
        }
    }
    fn verify_command_actions(&mut self, b: &mut crate::scheduled::NativeBatch) {
        use crate::EngineCommand::*;
        for i in 0..b.len {
            if b.receipts[i].state != 1 {
                continue;
            }
            let command = &b.steps[i].as_ref().unwrap().command;
            let verified = match command {
                Play { deck } | PlayQuantized { deck, .. } => {
                    self.decks[*deck].playing && self.launches[*deck].is_none()
                }
                Pause { deck } => !self.decks[*deck].playing,
                Seek { deck, seconds }
                | ScrubEnd { deck, seconds }
                | ScrubTo { deck, seconds, .. } => {
                    (self.decks[*deck].seconds() - seconds).abs() <= 0.005
                        && self.decks[*deck].scratch_remaining == 0
                }
                SetPlaybackBpm { deck, bpm } => {
                    self.decks[*deck]
                        .target_playback_bpm()
                        .is_some_and(|target| (target - bpm).abs() < 0.001)
                        && !self.decks[*deck].sync_enabled
                        && self.ramps[crate::ControlTarget::TempoPercent { deck: *deck }.index()]
                            .is_none()
                }
                SetFx { deck, echo, reverb } => {
                    let d = &self.decks[*deck];
                    (d.echo.value - echo).abs() < 0.002 && (d.reverb.value - reverb).abs() < 0.002
                }
                _ => {
                    let mut expected = [None; crate::CONTROL_TARGET_COUNT];
                    crate::scheduled::endpoints(command, &mut expected);
                    self.batch_endpoints(&expected)
                }
            };
            if verified {
                b.receipts[i].state = 2;
                b.receipts[i].completed_frame = self.rendered_frames;
                b.receipts[i].completed_clock_seconds = self.decks.each_ref().map(|d| d.seconds());
                b.receipts[i].completed_load_revisions = self.load_revisions;
                b.receipts[i].completed_grid_revisions = self.grid_revisions;
                b.receipts[i].completed_position_revisions =
                    self.decks.each_ref().map(|d| d.position_revision);
                b.frame = self.rendered_frames;
            } else if matches!(command,PlayQuantized{deck,..} if self.launches[*deck].is_none()) {
                let failure = self
                    .command_prerequisite_failure(command, b.receipts[i].action_id)
                    .unwrap_or(crate::scheduled::CommandFailure {
                        cause: crate::scheduled::CommandFailureCause::LaunchAborted,
                        deck: command.deck(),
                        action_id: b.receipts[i].action_id,
                    });
                b.receipts[i].failure = Some(failure);
                b.receipts[i].state = 4;
                b.receipts[i].completed_frame = self.rendered_frames;
                self.fail_command_queue(b, crate::BatchReason::PrerequisiteFailed, None);
                return;
            } else if let Ramp { target, value, .. } = command {
                if self.ramps[target.index()].is_none()
                    && (self.target_value(*target) - value).abs() > 0.001
                {
                    b.receipts[i].state = 4;
                    b.receipts[i].completed_frame = self.rendered_frames;
                    self.fail_command_queue(b, crate::BatchReason::ClockStopped, None);
                    return;
                }
            }
        }
    }
    fn resolve_command_dependency(
        &self,
        b: &mut crate::scheduled::NativeBatch,
        i: usize,
    ) -> Result<bool, crate::BatchReason> {
        use crate::scheduled::QueueTiming;
        let Some(QueueTiming::AfterAction {
            action_id,
            seconds,
            clock,
            beats,
            quantize,
        }) = b.steps[i].as_ref().unwrap().dependency
        else {
            return Ok(true);
        };
        let Some(receipt) = b.receipts[..b.len]
            .iter()
            .find(|r| r.action_id == action_id)
        else {
            return Err(crate::BatchReason::InvalidPatch);
        };
        if receipt.state == 3 || receipt.state == 4 {
            return Err(crate::BatchReason::PrerequisiteFailed);
        }
        if receipt.state != 2 {
            return Ok(false);
        }
        // A reference to any explicit group member means completion of the
        // entire group. Use the last actual receipt and its captured clock.
        let dependency_group = b.steps[receipt.step].as_ref().unwrap().group_id;
        let receipt = if let Some(group_id) = dependency_group {
            let mut latest = receipt;
            for j in 0..b.len {
                if b.steps[j].as_ref().unwrap().group_id != Some(group_id) {
                    continue;
                }
                let member = &b.receipts[j];
                if member.state == 3 || member.state == 4 {
                    return Err(crate::BatchReason::PrerequisiteFailed);
                }
                if member.state != 2 {
                    return Ok(false);
                }
                if member.completed_frame > latest.completed_frame {
                    latest = member;
                }
            }
            latest
        } else {
            receipt
        };
        let step = b.steps[i].as_mut().unwrap();
        if let (Some(clock), Some(beats)) = (clock, beats) {
            if receipt.completed_load_revisions[clock] != self.load_revisions[clock] {
                return Err(crate::BatchReason::SourceChanged);
            }
            if receipt.completed_grid_revisions[clock] != self.grid_revisions[clock] {
                return Err(crate::BatchReason::GridChanged);
            }
            if receipt.completed_position_revisions[clock] != self.decks[clock].position_revision {
                return Err(crate::BatchReason::PositionChanged);
            }
            let d = &self.decks[clock];
            if !d.grid_valid {
                return Err(crate::BatchReason::GridChanged);
            }
            if beats == 0. && quantize.is_none() {
                step.clock = None;
                step.at_frame = Some(self.rendered_frames);
                step.strict_time = false;
            } else {
                let mut position = d.grid_position(receipt.completed_clock_seconds[clock]) + beats;
                if let Some(q) = quantize {
                    position = (position / q).ceil() * q;
                }
                step.clock = Some(clock);
                step.at = d.grid_time(position);
                let launch = match step.command {
                    crate::EngineCommand::Play { deck } if deck != clock => {
                        Some((deck, self.load_revisions[clock]))
                    }
                    crate::EngineCommand::PlayQuantized {
                        deck,
                        clock: launch_clock,
                        clock_revision,
                        ..
                    } => {
                        if launch_clock != clock {
                            return Err(crate::BatchReason::InvalidPatch);
                        }
                        Some((deck, clock_revision))
                    }
                    _ => None,
                };
                if let Some((deck, clock_revision)) = launch {
                    step.at = d.grid_time(crate::scheduled::launch_beat(d.grid_position(step.at)));
                    step.command = crate::EngineCommand::PlayQuantized {
                        deck,
                        clock,
                        clock_revision,
                        at_seconds: Some(step.at),
                        quantize: 1.,
                    };
                }
            }
        } else {
            let delay = seconds.unwrap_or(0.);
            step.at_frame = Some(if delay == 0. {
                self.rendered_frames
            } else {
                receipt.completed_frame + (delay * self.sample_rate as f64).ceil() as u64
            });
        }
        step.dependency = None;
        let group_id = step.group_id;
        let (clock, mut at, at_frame) = (step.clock, step.at, step.at_frame);
        if let Some(group_id) = group_id {
            if let Some(clock) = clock {
                if b.steps[..b.len].iter().flatten().any(|member| member.group_id == Some(group_id)
                    && (matches!(member.command, crate::EngineCommand::Play {deck} if deck != clock)
                        || matches!(member.command, crate::EngineCommand::PlayQuantized {clock: launch_clock, ..} if launch_clock == clock))) {
                    let d = &self.decks[clock];
                    at = d.grid_time(crate::scheduled::launch_beat(d.grid_position(at)));
                }
            }
            for member in b.steps[..b.len].iter_mut().flatten() {
                if member.group_id != Some(group_id) {
                    continue;
                }
                member.clock = clock;
                member.at = at;
                member.at_frame = at_frame;
                member.dependency = None;
                if let (crate::EngineCommand::Play { deck }, Some(clock)) = (&member.command, clock)
                {
                    if *deck != clock {
                        member.command = crate::EngineCommand::PlayQuantized {
                            deck: *deck,
                            clock,
                            clock_revision: self.load_revisions[clock],
                            at_seconds: Some(at),
                            quantize: 1.,
                        };
                    }
                }
            }
        }
        Ok(true)
    }
    fn advance_command_queue(&mut self, b: &mut crate::scheduled::NativeBatch) {
        use crate::scheduled::{BatchLifecycle as L, BatchReason as R};
        let reason = if self.engine_epoch != b.engine_epoch || self.sample_rate != b.sample_rate {
            R::AudioStopped
        } else {
            self.active_command_guard_reason(b)
        };
        if reason != R::None {
            self.cancel_owned(b);
            self.cancel_batch_receipts(b);
            b.state = L::Yielded;
            b.reason = reason;
            b.frame = self.rendered_frames;
            return;
        }
        self.advance_ramps();
        self.verify_command_actions(b);
        if !b.state.active() {
            return;
        }
        for i in 0..b.len {
            if b.receipts[i].state != 0 {
                continue;
            }
            match self.resolve_command_dependency(b, i) {
                Ok(false) => continue,
                Err(reason) => {
                    if reason == R::PrerequisiteFailed {
                        self.set_command_group_failure(
                            b,
                            i,
                            crate::scheduled::CommandFailure {
                                cause: crate::scheduled::CommandFailureCause::DependencyFailed,
                                deck: b.steps[i].as_ref().unwrap().clock,
                                action_id: b.receipts[i].action_id,
                            },
                        );
                    }
                    self.fail_command_queue(b, reason, Some(i));
                    return;
                }
                Ok(true) => {
                    if b.receipts[i].resolved_clock.is_none()
                        && b.receipts[i].resolved_frame.is_none()
                    {
                        self.capture_command_resolution(b, i);
                    }
                }
            }
        }
        for i in 0..b.len {
            if b.receipts[i].state != 0 || b.steps[i].as_ref().unwrap().dependency.is_some() {
                continue;
            }
            let step = b.steps[i].as_ref().unwrap();
            if step.clock.is_some_and(|clock| {
                !self.decks[clock].playing
                    && self.decks[clock].track.as_ref().is_some_and(|t| {
                        self.decks[clock].position >= t.samples.len().saturating_sub(1) as f64
                    })
            }) {
                self.set_command_group_failure(
                    b,
                    i,
                    crate::scheduled::CommandFailure {
                        cause: crate::scheduled::CommandFailureCause::ClockEof,
                        deck: step.clock,
                        action_id: step.action_id,
                    },
                );
                self.fail_command_queue(b, R::ClockStopped, Some(i));
                return;
            }
            let due = step
                .at_frame
                .is_some_and(|frame| self.rendered_frames >= frame)
                || step
                    .clock
                    .is_some_and(|clock| self.decks[clock].seconds() >= step.at);
            if !due {
                continue;
            }
            if step.strict_time
                && (step
                    .at_frame
                    .is_some_and(|frame| self.rendered_frames > frame + 2)
                    || step.clock.is_some_and(|clock| {
                        self.decks[clock].seconds()
                            > step.at + 2. * self.decks[clock].rate() / self.sample_rate as f64
                    }))
            {
                self.set_command_group_failure(
                    b,
                    i,
                    crate::scheduled::CommandFailure {
                        cause: crate::scheduled::CommandFailureCause::DeadlineMissed,
                        deck: step.clock,
                        action_id: step.action_id,
                    },
                );
                self.fail_command_queue(b, R::DeadlineExpired, Some(i));
                return;
            }
            let mut failure = None;
            for j in 0..b.len {
                if b.receipts[j].state == 0
                    && Self::same_command_group(step, b.steps[j].as_ref().unwrap())
                {
                    let member = b.steps[j].as_ref().unwrap();
                    failure = self.command_prerequisite_failure(&member.command, member.action_id);
                    if failure.is_some() {
                        break;
                    }
                }
            }
            if let Some(failure) = failure {
                self.set_command_group_failure(b, i, failure);
                self.fail_command_queue(b, R::PrerequisiteFailed, Some(i));
                return;
            }
            b.state = L::Running;
            let before_positions = self.decks.each_ref().map(|d| d.position_revision);
            let before_grids = self.grid_revisions;
            for j in 0..b.len {
                if b.receipts[j].state != 0
                    || !Self::same_command_group(
                        b.steps[i].as_ref().unwrap(),
                        b.steps[j].as_ref().unwrap(),
                    )
                {
                    continue;
                }
                let step = b.steps[j].as_ref().unwrap();
                let mut command = step.command.clone();
                if let crate::EngineCommand::PlayQuantized {
                    deck,
                    clock,
                    at_seconds,
                    ..
                } = &mut command
                {
                    b.launched[*deck] = true;
                    // Retain the resolved event, rather than validating the
                    // first sample after it as if that were a new grid event.
                    // Untimed explicit launches retain their own native timing.
                    if step.clock == Some(*clock) {
                        *at_seconds = Some(step.at);
                    }
                }
                let targets = crate::scheduled::command_targets(&command);
                for old in 0..b.len {
                    if b.receipts[old].state == 1
                        && crate::scheduled::command_targets(
                            &b.steps[old].as_ref().unwrap().command,
                        ) & targets
                            != 0
                    {
                        b.receipts[old].state = 3;
                        b.receipts[old].completed_frame = self.rendered_frames;
                    }
                }
                self.command(command);
                b.receipts[j] = crate::scheduled::BatchEventReceipt {
                    step: j,
                    action_id: step.action_id,
                    state: 1,
                    frame: self.rendered_frames,
                    clock_seconds: step.clock.map_or(0., |c| self.decks[c].seconds()),
                    ramp_end_frame: match &step.command {
                        crate::EngineCommand::Ramp { target, .. } => {
                            self.ramps[target.index()].map(|r| r.end)
                        }
                        _ => None,
                    },
                    ..b.receipts[j]
                };
            }
            for k in 0..b.len {
                if b.receipts[k].state > 1 {
                    continue;
                }
                let guard = b.steps[k].as_mut().unwrap();
                for deck in 0..2 {
                    if guard.guard_decks & (1 << deck) != 0 {
                        if guard.position_revisions[deck] == before_positions[deck] {
                            guard.position_revisions[deck] = self.decks[deck].position_revision;
                        }
                        if guard.grid_revisions[deck] == before_grids[deck] {
                            guard.grid_revisions[deck] = self.grid_revisions[deck];
                        }
                    }
                }
            }
            b.positions = self.decks.each_ref().map(|d| d.position_revision);
            b.grids = self.grid_revisions;
            b.rates = self.decks.each_ref().map(|d| d.rate());
            b.control_revision = self.control_revision;
            b.frame = self.rendered_frames;
            self.verify_command_actions(b);
        }
        if b.state.active() && b.len > 0 && b.receipts[..b.len].iter().all(|r| r.state >= 2) {
            b.state = L::Completed;
            b.frame = self.rendered_frames;
        }
    }
}
