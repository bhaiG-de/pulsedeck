// App-owned repertoire sequencing; every transition uses the shared move executor.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct QueuePolicy {
    playlist: Option<String>,
    provider: Option<String>,
    energy_direction: String,
    max_stretch_percent: f64,
    techniques: Option<Vec<String>>,
    review_confirmed: bool,
    replace_prepared: bool,
    duration_seconds: Option<f64>,
    max_skips: usize,
}
impl Default for QueuePolicy {
    fn default() -> Self {
        Self {
            playlist: None,
            provider: None,
            energy_direction: "keep".into(),
            max_stretch_percent: crate::next_move::DEFAULT_MAX_STRETCH_PERCENT,
            techniques: None,
            review_confirmed: false,
            replace_prepared: false,
            duration_seconds: None,
            max_skips: 0,
        }
    }
}
impl QueuePolicy {
    fn validate(&self, ordered: bool) -> Result<()> {
        if !ordered && self.playlist.is_none() && self.provider.is_none() {
            return Err(mix_error(
                "invalid_arguments",
                "Choose explicit ordered sources, a playlist or a provider for the queue",
            ));
        }
        if self
            .provider
            .as_deref()
            .is_some_and(|p| !["local", "spotify"].contains(&p))
            || !["keep", "raise", "lower"].contains(&self.energy_direction.as_str())
            || !self.max_stretch_percent.is_finite()
            || !(0. ..=100.).contains(&self.max_stretch_percent)
            || self.max_skips > 512
            || self
                .playlist
                .as_ref()
                .is_some_and(|p| p.is_empty() || p.len() > 256)
        {
            return Err(mix_error(
                "invalid_arguments",
                "Invalid queue scope, energy, tempo range or skip limit",
            ));
        }
        if let Some(seconds) = self.duration_seconds {
            mix_session_duration(seconds)?;
        }
        if let Some(techniques) = &self.techniques {
            let capabilities = crate::next_move::capabilities();
            if techniques.is_empty()
                || techniques
                    .iter()
                    .any(|t| !capabilities.iter().any(|c| c.technique == t && c.available))
            {
                return Err(mix_error(
                    "invalid_arguments",
                    "Choose currently supported queue techniques",
                ));
            }
        }
        Ok(())
    }
    fn planner_request(&self, source: Option<&str>) -> Result<crate::next_move::OptionsRequest> {
        serde_json::from_value(json!({"playlist":self.playlist,"provider":self.provider,
            "energy_direction":self.energy_direction,"max_stretch_percent":self.max_stretch_percent,
            "techniques":self.techniques,"sources":source.map(|s|vec![s]),"limit":30}))
        .map_err(failure)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct QueueItem {
    id: u64,
    source: String,
    title: String,
    artist: String,
    state: String,
    availability: String,
    reason: Option<String>,
    operation_id: Option<String>,
    technique: Option<String>,
}
impl QueueItem {
    fn pending(id: u64, source: String) -> Self {
        Self {
            id,
            title: source.clone(),
            source,
            artist: String::new(),
            state: "pending".into(),
            availability: "unchecked".into(),
            reason: None,
            operation_id: None,
            technique: None,
        }
    }
    fn in_progress(&self) -> bool {
        ![
            "pending",
            "completed",
            "skipped",
            "cancelled",
            "interrupted",
        ]
        .contains(&self.state.as_str())
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct QueueRecord {
    queue_id: String,
    revision: u64,
    state: String,
    reason: Option<String>,
    mode: String,
    policy: QueuePolicy,
    items: Vec<QueueItem>,
    current_item: Option<u64>,
    operation_id: Option<String>,
    completed_moves: usize,
    consecutive_skips: usize,
    started_unix_ms: u64,
    finish_unix_ms: Option<u64>,
    coverage: Value,
    restored: bool,
}
impl QueueRecord {
    fn active(&self) -> bool {
        matches!(self.state.as_str(), "running" | "finishing")
    }
    fn summary(&self) -> Value {
        let mut value = serde_json::to_value(self).expect("Queue record serialization");
        let ready = self.completed_moves > 0
            || self
                .items
                .iter()
                .any(|item| matches!(item.state.as_str(), "armed" | "transitioning" | "completed"));
        value["readiness"] = json!(if !self.active() {
            self.state.as_str()
        } else if ready {
            "ready"
        } else {
            "preparing"
        });
        value["handoff"] = json!({"owner":"application","agent_can_finish":self.active() && ready,
            "notification_event":"mix.queue.notice","status_command":"mix.queue.status",
            "instruction":if !self.active() { "Queue ended; inspect its outcome and recover when attention is required." } else if ready { "First successor armed. The app owns execution; observe or hand off as requested by the user." } else { "Wait for the first armed successor or an actionable failure before handing off." }});
        value["terminal"] = json!(!self.active());
        value["decision_required"] = json!(matches!(
            self.state.as_str(),
            "needs_attention" | "interrupted" | "yielded"
        ));
        value["event_sequence"] = Value::Null;
        value
    }
    fn finish(&mut self, state: &str, reason: &str) {
        self.state = state.into();
        self.reason = Some(reason.into());
        for item in self.items.iter_mut().filter(|i| i.in_progress()) {
            item.state = if state == "interrupted" {
                "interrupted"
            } else {
                "cancelled"
            }
            .into();
            item.reason = Some(reason.into());
        }
        self.current_item = None;
        self.revision += 1;
    }
    fn skip(&mut self, reason: String) {
        if spotify_playback::is_playback_key_failure(&reason)
            || reason.starts_with("Spotify native playback is cooling down")
        {
            self.finish("needs_attention", &reason);
            return;
        }
        if let Some(item) = self
            .items
            .iter_mut()
            .find(|i| Some(i.id) == self.current_item)
        {
            item.state = "skipped".into();
            item.availability = "unavailable".into();
            item.reason = Some(reason.clone());
        }
        self.operation_id = None;
        self.current_item = None;
        self.consecutive_skips += 1;
        self.revision += 1;
        if self.policy.max_skips > 0 && self.consecutive_skips >= self.policy.max_skips {
            self.finish("needs_attention", &format!("skip_limit_reached: {reason}"));
        }
    }
    fn edit_pending(&mut self, expected: u64, sources: Vec<String>) -> Result<()> {
        if !self.active() || self.mode != "ordered" {
            return Err(mix_error(
                "not_ready",
                "Only a running ordered queue has editable pending sources",
            ));
        }
        if self.revision != expected {
            return Err(mix_error(
                "revision_conflict",
                "Queue changed; refresh its revision before editing",
            ));
        }
        if sources.len() > 128 || self.items.len() + sources.len() > 512 {
            return Err(mix_error(
                "invalid_arguments",
                "Queue supports at most 128 pending sources and 512 history entries",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        if sources.iter().any(|s| !seen.insert(s)) {
            return Err(mix_error(
                "invalid_arguments",
                "Pending queue sources must be unique",
            ));
        }
        let next = self.items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        self.items.retain(|i| i.state != "pending");
        self.items.extend(
            sources
                .into_iter()
                .enumerate()
                .map(|(i, s)| QueueItem::pending(next + i as u64, s)),
        );
        self.revision += 1;
        Ok(())
    }
}
struct SuccessorQueue {
    record: QueueRecord,
    engine_epoch: u64,
    takeover_revision: u64,
    endpoint_session_id: Option<String>,
    preparation: Option<QueueSourcePreparation>,
}
struct QueueSourcePreparation {
    deck: usize,
    owner: String,
    deadline: Instant,
    reference_deck: usize,
    snapshot: deck_engine::EngineSnapshot,
}

fn queue_event_feedback(summary: &Value) -> Value {
    let current = summary["items"]
        .as_array()
        .and_then(|items| items.iter().find(|i| i["id"] == summary["current_item"]));
    let item = current.or_else(|| {
        summary["items"]
            .as_array()
            .and_then(|items| items.iter().rev().find(|i| i["state"] != "pending"))
    });
    json!({"queue_id":summary["queue_id"],"revision":summary["revision"],"state":summary["state"],
        "readiness":summary["readiness"],"handoff":summary["handoff"],
        "reason":summary["reason"],"terminal":summary["terminal"],"decision_required":summary["decision_required"],
        "operation_id":summary["operation_id"],"current_item":current,"item":item,"completed_moves":summary["completed_moves"],
        "consecutive_skips":summary["consecutive_skips"],"event_sequence":summary["event_sequence"]})
}
fn publish_queue(app: &tauri::AppHandle, summary: Value) -> Result<()> {
    event(app, "mix.queue.updated", summary.clone());
    // Only actionable outcomes need to wake an external subscriber; healthy progress stays native.
    if summary["terminal"] == true && summary["state"] != "stopped" {
        event(app, "mix.queue.notice", queue_event_feedback(&summary));
    }
    persist_control(app)
}
fn queue_sources(app: &tauri::AppHandle, sources: &[String]) -> Result<Vec<String>> {
    if sources.len() > 128 {
        return Err(mix_error(
            "invalid_arguments",
            "Queue supports at most 128 pending sources",
        ));
    }
    let library = app.state::<AppState>();
    let library = library.library.lock().map_err(failure)?;
    let mut normalized = Vec::new();
    for source in sources {
        let resolved = library
            .normalize_suggestion_sources(std::slice::from_ref(source))
            .map_err(failure)?;
        let source = resolved
            .into_iter()
            .next()
            .ok_or_else(|| mix_error("invalid_arguments", "Empty source"))?;
        if normalized.contains(&source) {
            return Err(mix_error(
                "invalid_arguments",
                "Queue sources must be unique",
            ));
        }
        normalized.push(source);
    }
    Ok(normalized)
}
fn queue_item_metadata(app: &tauri::AppHandle, items: &mut [QueueItem]) -> Result<()> {
    app.state::<AppState>()
        .library
        .lock()
        .map_err(failure)?
        .with_features(|pool| {
            for item in items {
                if let Some(track) = pool.get(&item.source) {
                    item.title = track.title.clone();
                    item.artist = track.artist.clone();
                    item.availability = if track.energy.is_some() {
                        "analysis_cached"
                    } else {
                        "needs_analysis"
                    }
                    .into();
                }
            }
        })
        .map_err(failure)
}

fn queue_analysis_candidate<'a>(
    coverage: &'a crate::library::PlannerCoverage,
    reference_artist: Option<&str>,
) -> Option<&'a crate::library::PlannerMember> {
    // Metadata chooses what to prepare, never a beatmatch or a musical passage.
    coverage.needs_analysis.iter().rev().max_by_key(|member| {
        (
            member.duration_seconds >= 180.,
            reference_artist.is_some_and(|artist| !artist.is_empty() && member.artist == artist),
        )
    })
}
fn start_successor_queue(
    app: &tauri::AppHandle,
    request: &Request,
) -> Result<(&'static str, Value)> {
    let mut policy_args = request.args.clone();
    let sources = optional::<Vec<String>>(&request.args, "sources")?;
    policy_args
        .as_object_mut()
        .ok_or_else(|| mix_error("invalid_arguments", "Queue arguments must be an object"))?
        .remove("sources");
    let policy: QueuePolicy = serde_json::from_value(policy_args).map_err(failure)?;
    if sources.as_ref().is_some_and(Vec::is_empty) {
        return Err(mix_error(
            "invalid_arguments",
            "Ordered queue sources cannot be empty",
        ));
    }
    policy.validate(sources.is_some())?;
    let sources = sources.map(|s| queue_sources(app, &s)).transpose()?;
    if sources.is_none() && policy.playlist.is_some() {
        let coverage = app
            .state::<AppState>()
            .library
            .lock()
            .map_err(failure)?
            .planner_coverage(policy.playlist.as_deref(), None, policy.provider.as_deref())
            .map_err(failure)?;
        if !coverage.complete {
            return Err(mix_error("scope_incomplete","Index the complete collection before starting this queue; the latest cached page is not the full repertoire").with_detail("coverage",serialized(coverage)?));
        }
    }
    let control = app.state::<Control>();
    if control
        .mix_queue
        .lock()
        .map_err(failure)?
        .as_ref()
        .is_some_and(|q| q.record.active())
        || control
            .mix_prepared
            .lock()
            .map_err(failure)?
            .values()
            .any(|p| !mix_terminal(&p.state) && p.automatic)
        || control
            .mix_session
            .lock()
            .map_err(failure)?
            .as_ref()
            .is_some_and(MixSession::active)
    {
        return Err(mix_error(
            "busy",
            "Stop the existing queue, move or duration session first",
        ));
    }
    let snapshot = app
        .state::<AppState>()
        .engine
        .scheduled_snapshot()
        .map_err(scheduled_failure)?;
    if !snapshot.running
        || !snapshot
            .decks
            .iter()
            .any(|d| d.playing && d.channel_fader > 0.001 && !d.preparation_pending)
        || snapshot.scheduled_batch.state.active()
        || snapshot.active_ramps.iter().any(|r| *r)
    {
        return Err(mix_error("not_ready", "Start a healthy audible carrier and let existing automation settle before starting a successor queue"));
    }
    let id = format!(
        "mix-queue-{}-{}",
        std::process::id(),
        control.job_sequence.fetch_add(1, Ordering::Relaxed) + 1
    );
    let now = Instant::now();
    let endpoint = policy.duration_seconds.map(|seconds| MixSession {
        id: format!("{id}-endpoint"),
        prepared_id: id.clone(),
        started: now,
        duration: Duration::from_secs_f64(seconds),
        started_unix_ms: unix_ms(),
        engine_epoch: snapshot.engine_epoch,
        sample_rate: snapshot.sample_rate,
        underruns: snapshot.underruns,
        last_rendered_frames: snapshot.rendered_frames,
        last_audio_progress: now,
        native_batch_id: None,
        state: "running",
        reason: None,
        endpoint_sequence: None,
        endpoint_ticket: None,
        successor: None,
        notice: None,
    });
    let endpoint_session_id = endpoint.as_ref().map(|s| s.id.clone());
    let mut items: Vec<_> = sources
        .clone()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(i, s)| QueueItem::pending(i as u64 + 1, s))
        .collect();
    queue_item_metadata(app, &mut items)?;
    let record = QueueRecord {
        queue_id: id.clone(),
        revision: 1,
        state: "running".into(),
        reason: None,
        mode: if sources.is_some() {
            "ordered"
        } else {
            "collection"
        }
        .into(),
        started_unix_ms: unix_ms(),
        finish_unix_ms: policy
            .duration_seconds
            .map(|s| unix_ms() + (s * 1000.) as u64),
        policy,
        items,
        current_item: None,
        operation_id: None,
        completed_moves: 0,
        consecutive_skips: 0,
        coverage: Value::Null,
        restored: false,
    };
    let summary = record.summary();
    *control.mix_queue.lock().map_err(failure)? = Some(SuccessorQueue {
        record,
        engine_epoch: snapshot.engine_epoch,
        takeover_revision: control.mix_takeover_revision.load(Ordering::Acquire),
        endpoint_session_id,
        preparation: None,
    });
    if let Some(endpoint) = endpoint {
        let summary = endpoint.summary(now);
        *control.mix_session.lock().map_err(failure)? = Some(endpoint);
        event(app, "mix.session.updated", summary);
    }
    if let Err(error) = publish_queue(app, summary.clone()) {
        if let Some(q) = control.mix_queue.lock().map_err(failure)?.as_mut() {
            q.record.finish("interrupted", "journal_unavailable");
        }
        if let Some(s) = control
            .mix_session
            .lock()
            .map_err(failure)?
            .as_mut()
            .filter(|s| s.prepared_id == id)
        {
            s.finish("interrupted", "journal_unavailable");
        }
        return Err(error);
    }
    // Capture before the worker starts so a fast startup failure cannot be skipped.
    let event_sequence = control.event_sequence.load(Ordering::Acquire);
    let runtime = app.clone();
    std::thread::spawn(move || run_successor_queue(&runtime, &id));
    let mut summary = summary;
    summary["event_sequence"] = json!(event_sequence);
    Ok(("accepted", summary))
}
fn status_successor_queue(
    app: &tauri::AppHandle,
    request: &Request,
) -> Result<(&'static str, Value)> {
    let control = app.state::<Control>();
    let stored = control.mix_queue.lock().map_err(failure)?;
    let Some(queue) = stored.as_ref() else {
        return Ok((
            "applied",
            json!({"queue_id":null,"state":"idle","items":[],"revision":0,"terminal":true}),
        ));
    };
    if optional::<String>(&request.args, "queue_id")?.is_some_and(|id| id != queue.record.queue_id)
    {
        return Err(mix_error("not_found", "Successor queue not found"));
    }
    let mut summary = queue.record.summary();
    summary["event_sequence"] = json!(control.event_sequence.load(Ordering::Acquire));
    Ok(("applied", summary))
}
fn edit_successor_queue(
    app: &tauri::AppHandle,
    request: &Request,
) -> Result<(&'static str, Value)> {
    let sources = queue_sources(app, &arg::<Vec<String>>(&request.args, "sources")?)?;
    let control = app.state::<Control>();
    let mut stored = control.mix_queue.lock().map_err(failure)?;
    let queue = stored
        .as_mut()
        .ok_or_else(|| mix_error("not_found", "Successor queue not found"))?;
    if queue.record.queue_id != arg::<String>(&request.args, "queue_id")? {
        return Err(mix_error("not_found", "Successor queue not found"));
    }
    queue
        .record
        .edit_pending(arg(&request.args, "expected_revision")?, sources)?;
    let summary = queue.record.summary();
    drop(stored);
    publish_queue(app, summary.clone())?;
    Ok(("applied", summary))
}
fn stop_successor_queue(
    app: &tauri::AppHandle,
    request: &Request,
) -> Result<(&'static str, Value)> {
    let control = app.state::<Control>();
    let submission = control.scheduled_submission.lock().map_err(failure)?;
    let mut stored = control.mix_queue.lock().map_err(failure)?;
    let queue = stored
        .as_mut()
        .ok_or_else(|| mix_error("not_found", "Successor queue not found"))?;
    if optional::<String>(&request.args, "queue_id")?.is_some_and(|id| id != queue.record.queue_id)
    {
        return Err(mix_error("not_found", "Successor queue not found"));
    }
    if queue.record.active() {
        queue.record.finish("stopped", "explicit_stop");
    }
    let summary = queue.record.summary();
    let operation = queue.record.operation_id.clone();
    let endpoint = queue.endpoint_session_id.clone();
    let preparation = queue
        .preparation
        .as_ref()
        .map(|p| (p.deck, p.owner.clone()));
    drop(stored);
    drop(submission);
    if let Some((deck, owner)) = preparation {
        app.state::<AppState>()
            .source_jobs
            .cancel_owner_source(deck, &owner, &app.state::<AppState>().engine)
            .map_err(failure)?;
    }
    if let Some(operation) = operation {
        cancel_mix(
            app,
            &Request::new("mix.cancel", json!({"operation_id":operation})),
        )?;
    }
    if let Some(endpoint) = endpoint {
        stop_mix_session(
            app,
            &Request::new("mix.session.stop", json!({"session_id":endpoint})),
        )?;
    }
    publish_queue(app, summary.clone())?;
    Ok(("applied", summary))
}

fn successor_queue_owner_cancelled(app: &tauri::AppHandle, id: &str, owner: &str) -> bool {
    app.state::<Control>()
        .mix_queue
        .lock()
        .ok()
        .is_none_or(|stored| {
            !stored.as_ref().is_some_and(|q| {
                q.record.queue_id == id
                    && q.record.state == "running"
                    && q.preparation
                        .as_ref()
                        .is_some_and(|p| p.owner == owner && Instant::now() < p.deadline)
            })
        })
}
fn queue_preparation_changed(
    preparation: &QueueSourcePreparation,
    current: &deck_engine::EngineSnapshot,
) -> bool {
    let old = &preparation.snapshot;
    let d = preparation.reference_deck;
    !current.running
        || old.engine_epoch != current.engine_epoch
        || old.load_revisions[d] != current.load_revisions[d]
        || old.decks[d].grid_revision != current.decks[d].grid_revision
        || old.decks[d].position_revision != current.decks[d].position_revision
        || old.deck_control_revisions[d] != current.deck_control_revisions[d]
        || old.target_control_revisions[..2] != current.target_control_revisions[..2]
        || !current.decks[d].playing
        || current.decks[preparation.deck].playing
        || current.decks[preparation.deck].channel_fader > 0.001
}
fn apply_queue_endpoint(record: &mut QueueRecord, state: &str, reason: Option<&str>) -> bool {
    if !record.active() || state == "running" {
        return false;
    }
    if state == "finishing" {
        if record.state == "finishing" {
            return false;
        }
        record.state = "finishing".into();
        record.revision += 1;
    } else {
        record.finish(
            if state == "completed" {
                "completed"
            } else {
                "interrupted"
            },
            reason.unwrap_or("duration_session_ended"),
        );
    }
    true
}
fn poll_successor_preparation(app: &tauri::AppHandle) -> Result<()> {
    let control = app.state::<Control>();
    let mut stored = control.mix_queue.lock().map_err(failure)?;
    let Some(queue) = stored.as_mut() else {
        return Ok(());
    };
    let Some(preparation) = queue.preparation.as_ref() else {
        return Ok(());
    };
    let endpoint = queue
        .endpoint_session_id
        .as_ref()
        .map(|id| {
            control.mix_session.lock().map_err(failure).map(|session| {
                session
                    .as_ref()
                    .filter(|s| s.id == *id)
                    .map(|s| (s.state, s.reason.clone()))
                    .unwrap_or(("interrupted", Some("duration_session_missing".into())))
            })
        })
        .transpose()?;
    if let Some((state, reason)) = endpoint.filter(|(state, _)| *state != "running") {
        let (deck, owner) = (preparation.deck, preparation.owner.clone());
        let changed = apply_queue_endpoint(&mut queue.record, state, reason.as_deref());
        let summary = changed.then(|| queue.record.summary());
        drop(stored);
        app.state::<AppState>()
            .source_jobs
            .cancel_owner_source(deck, &owner, &app.state::<AppState>().engine)
            .map_err(failure)?;
        if let Some(summary) = summary {
            publish_queue(app, summary)?;
        }
        return Ok(());
    }
    let current = app.state::<AppState>().engine.transport_snapshot();
    let reason = if !queue.record.active() {
        "queue_stopped"
    } else if Instant::now() >= preparation.deadline {
        "source_preparation_deadline"
    } else if queue_preparation_changed(preparation, &current) {
        "source_preparation_ownership_changed"
    } else {
        return Ok(());
    };
    let (deck, owner) = (preparation.deck, preparation.owner.clone());
    let summary = if queue.record.active() {
        queue.record.finish("needs_attention", reason);
        Some(queue.record.summary())
    } else {
        None
    };
    drop(stored);
    app.state::<AppState>()
        .source_jobs
        .cancel_owner_source(deck, &owner, &app.state::<AppState>().engine)
        .map_err(failure)?;
    if let Some(summary) = summary {
        publish_queue(app, summary)?;
    }
    Ok(())
}
fn prepare_queue_source(
    app: &tauri::AppHandle,
    id: &str,
    source: &str,
    policy: &QueuePolicy,
) -> Result<()> {
    let control = app.state::<Control>();
    let state = app.state::<AppState>();
    let submission = control.scheduled_submission.lock().map_err(failure)?;
    let snapshot = state
        .engine
        .scheduled_snapshot()
        .map_err(scheduled_failure)?;
    let reference = snapshot
        .decks
        .iter()
        .position(|d| d.playing && d.channel_fader > 0.5)
        .ok_or_else(|| mix_error("not_ready", "No healthy carrier for source preparation"))?;
    let target = 1 - reference;
    validate_mix_target_availability(&snapshot.decks[target])?;
    if snapshot.decks[target]
        .track
        .as_ref()
        .is_some_and(|t| t.path != source)
        && !policy.replace_prepared
    {
        let owned = control
            .mix_queue
            .lock()
            .map_err(failure)?
            .as_ref()
            .is_some_and(|q| {
                q.record.completed_moves > 0
                    || q.record.items.iter().any(|item| {
                        snapshot.decks[target]
                            .track
                            .as_ref()
                            .is_some_and(|track| item.source == track.path)
                            && item.state == "skipped"
                    })
            });
        if !owned {
            return Err(mix_error(
                "deck_protected",
                "Confirm replacement of the paused prepared source before starting this queue",
            ));
        }
    }
    if state
        .play_history
        .lock()
        .map_err(failure)?
        .played()
        .contains(source)
    {
        return Err(mix_error(
            "already_played",
            "This ordered source was already heard during this session",
        ));
    }
    let remaining = (crate::next_move::useful_end_seconds(&snapshot.decks[reference])
        - snapshot.decks[reference].position_seconds)
        / snapshot.decks[reference].actual_playback_rate;
    let reserve = 15. + 20. * 60. / snapshot.decks[reference].local_bpm.unwrap_or(120.);
    if !remaining.is_finite() || remaining <= reserve + 8. {
        return Err(mix_error(
            "insufficient_runway",
            "The carrier has too little useful audio for source analysis and a safe transition",
        ));
    }
    if snapshot.decks[target].channel_fader > 0.001 {
        let mut isolate = Request::new(
            "engine.command",
            json!({"type":"set_channel_fader","deck":target,"value":0.}),
        );
        isolate.expected_load_revision = Some(snapshot.load_revisions[target]);
        apply_with_control_guard(
            app,
            &isolate,
            EngineCommand::SetChannelFader {
                deck: target,
                value: 0.,
            },
            Some(snapshot.control_revision),
        )?;
    }
    let snapshot = state
        .engine
        .scheduled_snapshot()
        .map_err(scheduled_failure)?;
    let (owner, summary) = {
        let mut stored = control.mix_queue.lock().map_err(failure)?;
        let queue = stored
            .as_mut()
            .filter(|q| q.record.queue_id == id && q.record.active())
            .ok_or_else(|| mix_error("cancelled", "Queue stopped before preparation"))?;
        let owner = format!("{id}:source-{}", queue.record.current_item.unwrap());
        queue.preparation = Some(QueueSourcePreparation {
            deck: target,
            owner: owner.clone(),
            deadline: Instant::now() + Duration::from_secs_f64((remaining - reserve).min(60.)),
            reference_deck: reference,
            snapshot: snapshot.clone(),
        });
        if let Some(item) = queue
            .record
            .items
            .iter_mut()
            .find(|i| Some(i.id) == queue.record.current_item)
        {
            item.state = "analysing".into();
            item.availability = "analysing".into();
        }
        queue.record.revision += 1;
        (owner, queue.record.summary())
    };
    drop(submission);
    publish_queue(app, summary)?;
    let mut load = Request::new(
        "deck.load",
        if source.starts_with("spotify:") {
            json!({"deck":target,"uri":source})
        } else {
            json!({"deck":target,"path":source})
        },
    );
    load.request_id = owner;
    load.expected_load_revision = Some(snapshot.load_revisions[target]);
    load.timeout_ms = ((remaining - reserve).min(60.) * 1000.) as u64;
    let result = dispatch(app, &load);
    let mut stored = control.mix_queue.lock().map_err(failure)?;
    let queue = stored
        .as_mut()
        .filter(|q| q.record.queue_id == id)
        .ok_or_else(|| mix_error("cancelled", "Queue replaced during preparation"))?;
    queue.preparation = None;
    if !queue.record.active() {
        return Err(mix_error("cancelled", "Queue source preparation stopped"));
    }
    drop(stored);
    result?;
    let loaded = state
        .engine
        .scheduled_snapshot()
        .map_err(scheduled_failure)?;
    if !loaded.decks[target]
        .track
        .as_ref()
        .is_some_and(|t| t.path == source)
        || loaded.decks[target].playing
        || loaded.decks[target].channel_fader > 0.001
    {
        return Err(mix_error(
            "source_changed",
            "Prepared source or isolation changed during analysis",
        ));
    }
    if !loaded.decks[target].beatgrid_valid {
        return Err(mix_error("needs_analysis","The prepared source has no usable grid; inspect or correct its analysis before queueing it"));
    }
    Ok(())
}
fn manual_successor_queue_takeover(app: &tauri::AppHandle) {
    let Some(control) = app.try_state::<Control>() else {
        return;
    };
    let summary = control.mix_queue.lock().ok().and_then(|mut stored| {
        let queue = stored.as_mut().filter(|q| q.record.active())?;
        queue.record.finish("yielded", "manual_takeover");
        Some(queue.record.summary())
    });
    if let Some(summary) = summary {
        event(app, "mix.queue.updated", summary);
    }
}
fn restore_successor_queue(app: &tauri::AppHandle, record: &Value) -> Result<()> {
    if record.is_null() {
        return Ok(());
    }
    let mut record: QueueRecord = serde_json::from_value(record.clone()).map_err(failure)?;
    if record.items.len() > 512 {
        return Err(mix_error(
            "journal_invalid",
            "Successor queue history exceeds limit",
        ));
    }
    if record.active() {
        record.finish("interrupted", "app_restarted_no_replay");
    }
    record.restored = true;
    let summary = record.summary();
    *app.state::<Control>().mix_queue.lock().map_err(failure)? = Some(SuccessorQueue {
        record,
        engine_epoch: 0,
        takeover_revision: 0,
        endpoint_session_id: None,
        preparation: None,
    });
    event(app, "mix.queue.updated", summary);
    Ok(())
}

fn run_successor_queue(app: &tauri::AppHandle, id: &str) {
    loop {
        match successor_queue_tick(app, id) {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => {
                let control = app.state::<Control>();
                let summary = control.mix_queue.lock().ok().and_then(|mut stored| {
                    let queue = stored
                        .as_mut()
                        .filter(|q| q.record.queue_id == id && q.record.active())?;
                    queue.record.finish("needs_attention", &error.message);
                    Some(queue.record.summary())
                });
                if let Some(summary) = summary {
                    let _ = publish_queue(app, summary);
                }
                break;
            }
        }
        let control = app.state::<Control>();
        if let Ok(events) = control.events.lock() {
            let _guard = control
                .event_ready
                .wait_timeout(events, Duration::from_millis(250));
        };
    }
    // Cancel only this queue's outstanding work. Ordinary music keeps playing.
    let control = app.state::<Control>();
    let owned = control.mix_queue.lock().ok().and_then(|stored| {
        stored
            .as_ref()
            .filter(|q| q.record.queue_id == id)
            .map(|q| {
                (
                    q.record.operation_id.clone(),
                    q.endpoint_session_id.clone(),
                    q.record.state.clone(),
                )
            })
    });
    if let Some((operation, endpoint, state)) = owned {
        if let Some(operation) = operation {
            let _ = cancel_mix(
                app,
                &Request::new("mix.cancel", json!({"operation_id":operation})),
            );
        }
        if state == "stopped" {
            if let Some(endpoint) = endpoint {
                let _ = stop_mix_session(
                    app,
                    &Request::new("mix.session.stop", json!({"session_id":endpoint})),
                );
            }
        }
    }
}

fn successor_queue_tick(app: &tauri::AppHandle, id: &str) -> Result<bool> {
    let control = app.state::<Control>();
    let (record, epoch, takeover, endpoint) = {
        let stored = control.mix_queue.lock().map_err(failure)?;
        let Some(queue) = stored
            .as_ref()
            .filter(|q| q.record.queue_id == id && q.record.active())
        else {
            return Ok(false);
        };
        (
            queue.record.clone(),
            queue.engine_epoch,
            queue.takeover_revision,
            queue.endpoint_session_id.clone(),
        )
    };
    if let Some(endpoint) = endpoint {
        let endpoint = control
            .mix_session
            .lock()
            .map_err(failure)?
            .as_ref()
            .filter(|s| s.id == endpoint)
            .map(|s| (s.state, s.reason.clone()));
        if let Some((state, reason)) = endpoint.filter(|(state, _)| !matches!(*state, "running")) {
            let mut stored = control.mix_queue.lock().map_err(failure)?;
            let queue = stored.as_mut().unwrap();
            let changed = apply_queue_endpoint(&mut queue.record, state, reason.as_deref());
            let active = queue.record.active();
            let summary = changed.then(|| queue.record.summary());
            drop(stored);
            if let Some(summary) = summary {
                publish_queue(app, summary)?;
            }
            return Ok(active);
        }
    }
    let snapshot = app.state::<AppState>().engine.transport_snapshot();
    if snapshot.engine_epoch != epoch
        || !snapshot.running
        || control.mix_takeover_revision.load(Ordering::Acquire) != takeover
    {
        return Err(mix_error(
            "manual_takeover",
            "Audio engine or external deck ownership changed",
        ));
    }
    if let Some(operation) = record.operation_id.as_ref() {
        let summary = control
            .mix_prepared
            .lock()
            .map_err(failure)?
            .get(operation)
            .map(mix_summary)
            .ok_or_else(|| mix_error("not_found", "Queue move receipt disappeared"))?;
        let state = summary["state"].as_str().unwrap_or("interrupted");
        let mut stored = control.mix_queue.lock().map_err(failure)?;
        let queue = stored.as_mut().unwrap();
        if !queue.record.active() || queue.record.operation_id.as_ref() != Some(operation) {
            return Ok(queue.record.active());
        }
        let item = queue
            .record
            .items
            .iter_mut()
            .find(|i| Some(i.id) == queue.record.current_item)
            .unwrap();
        if item.state == state {
            return Ok(true);
        }
        item.state = state.into();
        item.availability = if matches!(
            state,
            "ready" | "arming" | "armed" | "transitioning" | "completed"
        ) {
            "ready"
        } else {
            "loading"
        }
        .into();
        queue.record.revision += 1;
        if mix_terminal(state) {
            if state == "completed" {
                queue.record.completed_moves += 1;
                queue.record.consecutive_skips = 0;
                queue.record.operation_id = None;
                queue.record.current_item = None;
            } else if matches!(state, "failed" | "recovered") {
                queue
                    .record
                    .skip(summary["error"]["message"].as_str().unwrap_or(state).into());
            } else {
                queue.record.finish(
                    "needs_attention",
                    summary["error"]["message"].as_str().unwrap_or(state),
                );
            }
        }
        let active = queue.record.active();
        let summary = queue.record.summary();
        drop(stored);
        publish_queue(app, summary)?;
        return Ok(active);
    }
    if record.items.len() >= 512 {
        return Err(mix_error(
            "queue_full",
            "Queue reached its bounded history limit",
        ));
    }
    let mut source = if record.mode == "ordered" {
        let Some(item) = record.items.iter().find(|i| i.state == "pending") else {
            let mut stored = control.mix_queue.lock().map_err(failure)?;
            let queue = stored.as_mut().unwrap();
            queue.record.finish(
                if record.policy.duration_seconds.is_some() {
                    "needs_attention"
                } else {
                    "completed"
                },
                "ordered_queue_exhausted",
            );
            let summary = queue.record.summary();
            drop(stored);
            publish_queue(app, summary)?;
            return Ok(false);
        };
        let mut stored = control.mix_queue.lock().map_err(failure)?;
        let queue = stored.as_mut().unwrap();
        if !queue.record.active() {
            return Ok(false);
        }
        queue.record.current_item = Some(item.id);
        if let Some(item) = queue.record.items.iter_mut().find(|i| i.id == item.id) {
            item.state = "planning".into();
        }
        queue.record.revision += 1;
        let summary = queue.record.summary();
        drop(stored);
        publish_queue(app, summary)?;
        Some(item.source.clone())
    } else {
        None
    };
    let mut list = tauri::async_runtime::block_on(crate::next_move::options(
        app.clone(),
        record.policy.planner_request(source.as_deref())?,
    ))
    .map_err(failure)?;
    list.options.retain(|option| {
        !record.items.iter().any(|item| {
            item.source == option.source && ["completed", "skipped"].contains(&item.state.as_str())
        })
    });
    if record.mode == "collection"
        && list.options.is_empty()
        && list.coverage.needs_analysis_members > 0
    {
        let state = app.state::<AppState>();
        let mut excluded = state.play_history.lock().map_err(failure)?.played().clone();
        excluded.extend(record.items.iter().map(|item| item.source.clone()));
        let snapshot = state
            .engine
            .scheduled_snapshot()
            .map_err(scheduled_failure)?;
        let reference = list
            .reference_deck
            .and_then(|deck| snapshot.decks[deck].track.as_ref());
        if let Some(track) = reference {
            excluded.insert(track.path.clone());
        }
        let coverage = state
            .library
            .lock()
            .map_err(failure)?
            .planner_coverage_excluding(
                record.policy.playlist.as_deref(),
                None,
                record.policy.provider.as_deref(),
                &excluded,
            )
            .map_err(failure)?;
        if let Some(candidate) = queue_analysis_candidate(
            &coverage,
            reference.and_then(|track| track.artist.as_deref()),
        ) {
            let mut stored = control.mix_queue.lock().map_err(failure)?;
            let Some(queue) = stored
                .as_mut()
                .filter(|q| q.record.queue_id == id && q.record.active())
            else {
                return Ok(false);
            };
            let next = queue
                .record
                .items
                .iter()
                .map(|item| item.id)
                .max()
                .unwrap_or(0)
                + 1;
            let mut item = QueueItem::pending(next, candidate.source.clone());
            item.title = candidate.title.clone();
            item.artist = candidate.artist.clone();
            item.state = "planning".into();
            queue.record.items.push(item);
            queue.record.current_item = Some(next);
            queue.record.coverage = serialized(&coverage)?;
            queue.record.revision += 1;
            source = Some(candidate.source.clone());
        }
    }
    if let Some(source) = source
        .as_deref()
        .filter(|_| list.coverage.needs_analysis_members > 0)
    {
        if let Err(error) = prepare_queue_source(app, id, source, &record.policy) {
            {
                let stored = control.mix_queue.lock().map_err(failure)?;
                let Some(queue) = stored
                    .as_ref()
                    .filter(|q| q.record.queue_id == id && q.record.active())
                else {
                    return Ok(false);
                };
                if queue.record.state == "finishing" {
                    return Ok(true);
                }
            }
            if [
                "operation_failed",
                "track_not_found",
                "timeout",
                "source_unavailable",
                "needs_analysis",
            ]
            .contains(&error.code.as_str())
            {
                let mut stored = control.mix_queue.lock().map_err(failure)?;
                let queue = stored.as_mut().unwrap();
                if !queue.record.active() {
                    return Ok(false);
                }
                queue.record.skip(error.message);
                let active = queue.record.active();
                let summary = queue.record.summary();
                drop(stored);
                publish_queue(app, summary)?;
                return Ok(active);
            }
            return Err(error);
        }
        list = tauri::async_runtime::block_on(crate::next_move::options(
            app.clone(),
            record.policy.planner_request(Some(source))?,
        ))
        .map_err(failure)?;
    }
    let option = list
        .options
        .iter()
        .filter(|o| {
            !record.items.iter().any(|i| {
                i.source == o.source && ["completed", "skipped"].contains(&i.state.as_str())
            })
        })
        .min_by(|a, b| b.score.total_cmp(&a.score))
        .cloned();
    // Stop/edit and submission are serialized, while slow planning remains cancellable.
    let submission = control.scheduled_submission.lock().map_err(failure)?;
    {
        let mut stored = control.mix_queue.lock().map_err(failure)?;
        let queue = stored.as_mut().unwrap();
        if queue.record.queue_id != id
            || !queue.record.active()
            || control.mix_takeover_revision.load(Ordering::Acquire) != takeover
        {
            return Ok(false);
        }
        queue.record.coverage = serialized(&list.coverage)?;
        if queue.record.mode == "collection" {
            if let Some(option) = &option {
                if queue.record.current_item.is_none() {
                    let next = queue.record.items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
                    queue.record.current_item = Some(next);
                    queue
                        .record
                        .items
                        .push(QueueItem::pending(next, option.source.clone()));
                }
            } else if queue.record.current_item.is_some() {
                queue.record.skip(list.reasons.join("; "));
            } else {
                queue
                    .record
                    .finish("needs_attention", &list.reasons.join("; "));
            }
        }
        if option.is_none() && queue.record.mode == "ordered" {
            queue.record.skip(list.reasons.join("; "));
        }
    }
    let Some(option) = option else {
        let stored = control.mix_queue.lock().map_err(failure)?;
        let queue = stored.as_ref().unwrap();
        let active = queue.record.active();
        let summary = queue.record.summary();
        drop(stored);
        drop(submission);
        publish_queue(app, summary)?;
        return Ok(active);
    };
    let result = execute_mix(
        app,
        &Request::new(
            "mix.execute",
            json!({"option_id":option.id,"review_confirmed":record.policy.review_confirmed,"replace_prepared":record.policy.replace_prepared || record.completed_moves>0,"allow_fallback":true}),
        ),
    );
    let mut stored = control.mix_queue.lock().map_err(failure)?;
    let queue = stored.as_mut().unwrap();
    match result {
        Ok((_, summary)) => {
            let operation = summary["operation_id"]
                .as_str()
                .ok_or_else(|| {
                    mix_error(
                        "protocol_error",
                        "Move acceptance is missing operation identity",
                    )
                })?
                .to_owned();
            queue.record.operation_id = Some(operation.clone());
            if let Some(item) = queue
                .record
                .items
                .iter_mut()
                .find(|i| Some(i.id) == queue.record.current_item)
            {
                item.title = option.title;
                item.artist = option.artist;
                item.state = "preparing".into();
                item.availability = "loading".into();
                item.operation_id = Some(operation);
                item.technique = Some(option.technique);
            }
            queue.record.revision += 1;
        }
        Err(error) => {
            if [
                "busy",
                "resource_conflict",
                "deck_protected",
                "review_required",
                "manual_takeover",
            ]
            .contains(&error.code.as_str())
            {
                queue.record.finish("needs_attention", &error.message);
            } else {
                queue.record.skip(error.message);
            }
        }
    }
    let active = queue.record.active();
    let summary = queue.record.summary();
    drop(stored);
    drop(submission);
    publish_queue(app, summary)?;
    Ok(active)
}

#[cfg(test)]
mod successor_queue_tests {
    use super::*;
    fn record() -> QueueRecord {
        QueueRecord {
            queue_id: "queue-test".into(),
            revision: 1,
            state: "running".into(),
            reason: None,
            mode: "ordered".into(),
            policy: QueuePolicy {
                review_confirmed: true,
                ..Default::default()
            },
            items: vec![
                QueueItem::pending(1, "first".into()),
                QueueItem::pending(2, "second".into()),
            ],
            current_item: None,
            operation_id: None,
            completed_moves: 0,
            consecutive_skips: 0,
            started_unix_ms: 1,
            finish_unix_ms: None,
            coverage: Value::Null,
            restored: false,
        }
    }
    #[test]
    fn queue_scope_and_engine_ranges_are_validated_without_musical_review_gate() {
        let mut policy = QueuePolicy::default();
        assert!(policy.validate(false).is_err());
        assert!(policy.validate(true).is_ok());
        policy.review_confirmed = true;
        assert!(policy.validate(true).is_ok());
        policy.provider = Some("local".into());
        assert!(policy.validate(false).is_ok());
        policy.max_stretch_percent = 101.;
        assert!(policy.validate(true).is_err());
        policy.max_stretch_percent = 2.;
        policy.techniques = Some(vec!["stem_blend".into()]);
        assert!(policy.validate(true).is_err());
        policy.techniques = Some(vec!["echo_out".into(), "reverb_exit".into()]);
        assert!(policy.validate(true).is_ok());
        policy.duration_seconds = Some(f64::NAN);
        assert!(policy.validate(true).is_err());
    }
    #[test]
    fn handoff_requires_an_armed_successor_not_just_acceptance_or_loading() {
        let mut queue = record();
        for state in [
            "pending",
            "planning",
            "analysing",
            "preparing",
            "ready",
            "arming",
        ] {
            queue.items[0].state = state.into();
            assert_eq!(
                queue.summary()["handoff"]["agent_can_finish"],
                false,
                "{state}"
            );
            assert_eq!(queue.summary()["readiness"], "preparing");
        }
        queue.items[0].state = "armed".into();
        assert_eq!(queue.summary()["handoff"]["agent_can_finish"], true);
        assert_eq!(queue_event_feedback(&queue.summary())["readiness"], "ready");
        queue.finish("needs_attention", "launch failed");
        assert_eq!(queue.summary()["handoff"]["agent_can_finish"], false);
        assert_eq!(queue.summary()["readiness"], "needs_attention");
    }
    #[test]
    fn collection_can_select_unanalysed_members_without_treating_metadata_as_a_grid() {
        let coverage = crate::library::PlannerCoverage {
            scope: Some("spotify:liked".into()),
            total_members: Some(2),
            indexed_members: 2,
            analysed_members: 0,
            needs_analysis_members: 2,
            complete: true,
            needs_analysis: vec![
                crate::library::PlannerMember {
                    source: "short".into(),
                    title: "Short".into(),
                    artist: "Other".into(),
                    duration_seconds: 60.,
                },
                crate::library::PlannerMember {
                    source: "long".into(),
                    title: "Long".into(),
                    artist: "Other".into(),
                    duration_seconds: 240.,
                },
            ],
            needs_analysis_truncated: false,
        };
        let candidate = queue_analysis_candidate(&coverage, Some("Current artist")).unwrap();
        assert_eq!(candidate.source, "long");
        let request = record()
            .policy
            .planner_request(Some(&candidate.source))
            .unwrap();
        assert_eq!(request.sources, Some(vec!["long".into()]));
        assert!(queue_analysis_candidate(
            &crate::library::PlannerCoverage {
                needs_analysis: vec![],
                ..coverage
            },
            None
        )
        .is_none());
    }
    #[test]
    fn editing_preserves_the_active_move_and_rejects_stale_or_ambiguous_changes_atomically() {
        let mut queue = record();
        queue.items[0].state = "preparing".into();
        queue.items[0].operation_id = Some("move-1".into());
        queue.current_item = Some(1);
        queue.operation_id = Some("move-1".into());
        let before = queue.summary();
        assert_eq!(
            queue
                .edit_pending(0, vec!["third".into()])
                .unwrap_err()
                .code,
            "revision_conflict"
        );
        assert_eq!(queue.summary(), before);
        assert!(queue
            .edit_pending(1, vec!["third".into(), "third".into()])
            .is_err());
        assert_eq!(queue.summary(), before);
        queue
            .edit_pending(1, vec!["third".into(), "second".into()])
            .unwrap();
        assert_eq!(
            queue
                .items
                .iter()
                .map(|i| i.source.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "third", "second"]
        );
        assert_eq!(queue.items[0].state, "preparing");
        assert_eq!(queue.operation_id.as_deref(), Some("move-1"));
        assert_eq!(queue.revision, 2);
    }
    #[test]
    fn default_queue_keeps_searching_after_musical_rejections() {
        let mut queue = record();
        assert_eq!(queue.policy.max_skips, 0);
        assert_eq!(queue.policy.max_stretch_percent, 4.);
        for _ in 0..12 {
            queue.skip("No suitable passage for this source".into());
        }
        assert!(queue.active());
        assert_eq!(queue.consecutive_skips, 12);
        assert_eq!(queue.completed_moves, 0);
    }
    #[test]
    fn provider_key_refusal_stops_search_without_blacklisting_other_songs() {
        let mut queue = record();
        queue.current_item = Some(1);
        queue.skip("Spotify track loading failed after native sign-in: the provider rejected or could not complete the playback key request. No partial audio was loaded.".into());
        assert_eq!(queue.state, "needs_attention");
        assert_eq!(queue.consecutive_skips, 0);
        assert_eq!(queue.completed_moves, 0);
        assert_eq!(queue.items[1].state, "pending");
        assert!(!queue.active());
    }
    #[test]
    fn failed_sources_have_a_finite_skip_budget_and_never_claim_completed_handovers() {
        let mut queue = record();
        queue.policy.max_skips = 2;
        queue.current_item = Some(1);
        queue.skip("decode failed".into());
        assert!(queue.active());
        assert_eq!(queue.items[0].state, "skipped");
        queue.current_item = Some(2);
        queue.skip("source unavailable".into());
        assert_eq!(queue.state, "needs_attention");
        assert_eq!(queue.completed_moves, 0);
        assert!(queue.summary()["decision_required"].as_bool().unwrap());
    }
    #[test]
    fn restart_interruption_preserves_pending_order_without_replaying_preparation() {
        let mut queue = record();
        queue.items[0].state = "arming".into();
        queue.current_item = Some(1);
        queue.operation_id = Some("move-1".into());
        let serialized = serde_json::to_value(&queue).unwrap();
        let mut restored: QueueRecord = serde_json::from_value(serialized).unwrap();
        if restored.active() {
            restored.finish("interrupted", "app_restarted_no_replay");
        }
        assert!(!restored.active());
        assert_eq!(restored.items[0].state, "interrupted");
        assert_eq!(restored.items[1].state, "pending");
        assert_eq!(restored.completed_moves, 0);
    }
    #[test]
    fn skipped_source_feedback_retains_its_reason_without_a_status_read() {
        let mut queue = record();
        queue.current_item = Some(1);
        queue.skip("Source unavailable".into());
        let event = queue_event_feedback(&queue.summary());
        assert!(event["current_item"].is_null());
        assert_eq!(event["item"]["source"], "first");
        assert_eq!(event["item"]["reason"], "Source unavailable");
        assert_eq!(event["state"], "running");
    }
    #[test]
    fn duration_endpoint_cancels_preparation_without_claiming_a_handover() {
        let mut queue = record();
        queue.current_item = Some(1);
        queue.items[0].state = "analysing".into();
        assert!(apply_queue_endpoint(&mut queue, "finishing", None));
        assert_eq!(queue.state, "finishing");
        assert!(!apply_queue_endpoint(&mut queue, "finishing", None));
        assert!(apply_queue_endpoint(
            &mut queue,
            "completed",
            Some("duration_reached")
        ));
        assert_eq!(queue.state, "completed");
        assert_eq!(queue.items[0].state, "cancelled");
        assert_eq!(queue.completed_moves, 0);
    }
    #[test]
    #[ignore = "Requires captured actual local-song snapshot; no audio hardware"]
    fn actual_local_queue_scope_options_and_takeover_guards() {
        let input = std::env::var("PULSEDECK_REAL_MOVE_INPUT").unwrap();
        let value: Value = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
        let snapshot: deck_engine::EngineSnapshot =
            serde_json::from_value(value["snapshot"].clone()).unwrap();
        let selected: crate::next_move::MoveOption =
            serde_json::from_value(value["option"].clone()).unwrap();
        let mut queue = record();
        queue.items = vec![QueueItem::pending(1, selected.source.clone())];
        for deck in &snapshot.decks {
            assert!(std::path::Path::new(&deck.track.as_ref().unwrap().path).is_file());
        }
        let request = queue
            .policy
            .planner_request(Some(&selected.source))
            .unwrap();
        assert_eq!(
            request.sources.as_deref(),
            Some(std::slice::from_ref(&selected.source))
        );
        let preparation = QueueSourcePreparation {
            deck: selected.target_deck,
            reference_deck: selected.reference_deck,
            owner: "queue:source-1".into(),
            deadline: Instant::now() + Duration::from_secs(60),
            snapshot: snapshot.clone(),
        };
        let mut transport = snapshot.clone();
        for deck in &mut transport.decks {
            deck.track = None;
        }
        assert!(!queue_preparation_changed(&preparation, &transport));
        transport.target_control_revisions[0] += 1;
        assert!(queue_preparation_changed(&preparation, &transport));
        transport.target_control_revisions[0] -= 1;
        transport.decks[selected.reference_deck].position_revision += 1;
        assert!(queue_preparation_changed(&preparation, &transport));
        queue.current_item = Some(1);
        queue.items[0].state = "preparing".into();
        queue.finish("stopped", "explicit_stop");
        assert_eq!(queue.items[0].state, "cancelled");
        assert!(!queue.active());
    }
}
