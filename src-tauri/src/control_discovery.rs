// Bounded discovery at the control boundary; full schemas/evidence remain opt-in.
fn compact_coverage(value: &mut Value) {
    if let Some(coverage) = value.as_object_mut() {
        coverage.remove("needs_analysis");
    }
}
fn compact_options(mut value: Value) -> Value {
    compact_coverage(&mut value["coverage"]);
    if let Some(options) = value["options"].as_array_mut() {
        for option in options {
            if let Some(fields) = option.as_object_mut() {
                fields.retain(|key, _| {
                    matches!(
                        key.as_str(),
                        "id" | "source"
                            | "title"
                            | "artist"
                            | "technique"
                            | "score"
                            | "native_bpm"
                            | "target_bpm"
                            | "tempo_percent"
                            | "entry_seconds"
                            | "launch_in_seconds"
                            | "prepare_within_seconds"
                            | "overlap_bars"
                            | "continuation_seconds"
                            | "review_required"
                            | "evidence"
                            | "warnings"
                    )
                });
            }
        }
    }
    value["detail_available"] = json!(true);
    value
}
fn discovery_catalog(args: &Value) -> Result<Value> {
    let prefix = optional::<String>(args, "prefix")?.unwrap_or_default();
    let search = optional::<String>(args, "search")?
        .unwrap_or_default()
        .to_lowercase();
    let detail = optional::<bool>(args, "detail")?.unwrap_or(false);
    // Avoid building native schemas just to list names and descriptions.
    let catalog = if detail {
        catalog()
    } else {
        control_protocol::catalog()
    };
    let mut commands: Vec<Value> = catalog
        .into_iter()
        .filter(|d| {
            d["command"]
                .as_str()
                .is_some_and(|name| name.starts_with(&prefix))
                && (search.is_empty()
                    || format!("{} {}", d["command"], d["description"])
                        .to_lowercase()
                        .contains(&search))
        })
        .collect();
    if optional::<bool>(args, "names_only")?.unwrap_or(false) {
        return Ok(
            json!({"protocol_version":control_protocol::VERSION,"commands":commands.iter().map(|d|d["command"].clone()).collect::<Vec<_>>()}),
        );
    }
    if !detail {
        for command in &mut commands {
            *command = json!({"command":command["command"],"description":command["description"].as_str().unwrap_or_default().chars().take(160).collect::<String>()});
        }
        return Ok(
            json!({"protocol_version":control_protocol::VERSION,"commands":commands,
            "detail_available":true,"hint":"Use commands describe METHOD for one schema, or commands list --detail for the full catalog."}),
        );
    }
    Ok(
        json!({"protocol_version":control_protocol::VERSION,"commands":commands,"engine_commands":engine_catalog()}),
    )
}
fn spotify_load_uri(args: &Value) -> Result<Option<String>> {
    let explicit = optional::<String>(args, "uri")?;
    let track = args["track"]
        .as_str()
        .filter(|s| s.starts_with("spotify:"))
        .map(str::to_owned);
    if explicit.is_some() && track.is_some() && explicit != track {
        return Err(ControlError::new(
            "invalid_arguments",
            "Conflicting track and uri sources",
        ));
    }
    let uri = explicit.or(track);
    if let Some(uri) = &uri {
        validate_control_track_uri(uri)?;
    }
    Ok(uri)
}
fn opener_candidates(app: &tauri::AppHandle, args: &Value) -> Result<Value> {
    let playlist = optional::<String>(args, "playlist")?;
    let provider = optional::<String>(args, "provider")?;
    let search = optional::<String>(args, "search")?
        .unwrap_or_default()
        .to_lowercase();
    let min = optional::<f64>(args, "min_bpm")?.unwrap_or(118.);
    let max = optional::<f64>(args, "max_bpm")?.unwrap_or(126.);
    let limit = optional::<usize>(args, "limit")?.unwrap_or(8);
    if !(30. ..=300.).contains(&min) || !(min..=300.).contains(&max) || !(1..=20).contains(&limit) {
        return Err(ControlError::new(
            "invalid_arguments",
            "Use 30..300 BPM, min_bpm <= max_bpm and limit 1..20",
        ));
    }
    let state = app.state::<AppState>();
    let library = state.library.lock().map_err(failure)?;
    let scope = playlist
        .as_ref()
        .map(|p| library.suggestion_scope(p))
        .transpose()
        .map_err(failure)?;
    let mut coverage = serialized(
        library
            .planner_coverage(playlist.as_deref(), None, provider.as_deref())
            .map_err(failure)?,
    )?;
    compact_coverage(&mut coverage);
    let candidates = library
        .with_features(|pool| {
            let mut tracks: Vec<_> = pool
                .values()
                .filter(|f| {
                    f.energy.is_some()
                        && f.bpm.is_some_and(|b| (min..=max).contains(&b))
                        && f.duration_seconds >= 180.
                        && scope
                            .as_ref()
                            .is_none_or(|(members, _)| members.contains(&f.source))
                        && provider
                            .as_deref()
                            .is_none_or(|p| (p == "spotify") == f.source.starts_with("spotify:"))
                        && (f.source.starts_with("spotify:")
                            || std::path::Path::new(&f.source).is_file())
                        && (search.is_empty()
                            || format!("{} {}", f.title, f.artist)
                                .to_lowercase()
                                .contains(&search))
                })
                .collect();
            tracks.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.source.cmp(&b.source)));
            tracks
                .into_iter()
                .take(limit)
                .map(|f| {
                    json!({"source":f.source,"title":f.title,"artist":f.artist,
            "bpm":f.bpm,"camelot":f.camelot,"duration_seconds":f.duration_seconds,"energy":f.energy,
            "analysis_cached":true})
                })
                .collect::<Vec<_>>()
        })
        .map_err(failure)?;
    Ok(
        json!({"candidates":candidates,"coverage":coverage,"read_only":true,
        "hint":"Choose an opener, load/play it, then start one detached successor queue. Index only if coverage.complete is false."}),
    )
}
#[cfg(test)]
mod discovery_tests {
    use super::*;
    #[test]
    fn discovery_is_bounded_and_full_schema_is_explicit() {
        let small = discovery_catalog(&json!({})).unwrap();
        assert!(serde_json::to_vec(&small).unwrap().len() < 30000);
        assert!(small["commands"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c.get("input_schema").is_none()));
        let selected = discovery_catalog(&json!({"prefix":"mix.queue.","search":"start"})).unwrap();
        assert_eq!(selected["commands"].as_array().unwrap().len(), 1);
        assert!(discovery_catalog(&json!({"detail":true})).unwrap()["engine_commands"].is_array());
        let small = compact_options(
            json!({"coverage":{"needs_analysis":["large"],"complete":false},"options":[{"id":"x","source":"spotify:track:x","evidence":["uncertain"],"warnings":["unknown vocals"],"incoming":{"large":[1,2]},"outgoing":{},"score_components":{}}]}),
        );
        assert_eq!(small["options"][0]["id"], "x");
        assert_eq!(small["options"][0]["warnings"][0], "unknown vocals");
        assert!(small["options"][0].get("incoming").is_none());
        assert!(small["coverage"].get("needs_analysis").is_none());
    }
    #[test]
    fn spotify_track_alias_validates_before_transport_changes() {
        let uri = "spotify:track:5ImCVtO1gvcD1ttdG5SrQT";
        assert_eq!(
            spotify_load_uri(&json!({"track":uri})).unwrap(),
            Some(uri.into())
        );
        assert!(spotify_load_uri(&json!({"track":"spotify:track:short"})).is_err());
        assert!(spotify_load_uri(
            &json!({"track":uri,"uri":"spotify:track:3XDG86sJ0K8Pr6kstpI3db"})
        )
        .is_err());
        assert!(spotify_load_uri(&json!({"track":"local:42"}))
            .unwrap()
            .is_none());
    }
}
