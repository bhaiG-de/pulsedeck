//! Read selection shared by UI/CLI; never removes receipt/error envelopes.
use crate::{ControlError, Request, Response};
use serde_json::{json, Value};
pub fn is_read(command: &str) -> bool {
    matches!(
        command,
        "performance.observe"
            | "performance.validate"
            | "performance.receipts"
            | "status"
            | "state.get"
            | "engine.transport"
            | "engine.snapshot"
            | "engine.metadata"
            | "app.state"
            | "app.activity"
            | "events.get"
            | "jobs.get"
            | "jobs.list"
            | "commands.list"
            | "commands.describe"
            | "capabilities"
            | "track.inspect"
            | "library.tracks"
            | "library.track"
            | "library.details"
            | "library.tag_categories"
            | "library.suggest_next"
            | "mix.options"
            | "mix.openers"
            | "mix.status"
            | "mix.session.status"
            | "mix.queue.status"
            | "playlist.list"
            | "cues.get"
            | "controller.state"
            | "midi.history"
            | "midi.ports"
            | "audio.devices"
            | "recording.status"
            | "spotify.status"
            | "spotify.native.status"
            | "spotify.liked"
            | "spotify.playlists"
            | "spotify.playlist_tracks"
            | "spotify.album_tracks"
            | "spotify.analysis.get"
            | "spotify.analysis.status"
            | "spotify.devices"
            | "sampler.state"
            | "ui.state"
    )
}
pub fn validate(request: &Request) -> Result<(), ControlError> {
    if (request.fields.is_some() || request.view.is_some()) && !is_read(&request.command) {
        return Err(ControlError::new(
            "invalid_arguments",
            "Read field masks/views are not valid on mutations",
        ));
    }
    if let Some(fields) = &request.fields {
        if fields.is_empty()
            || fields.len() > 32
            || fields
                .iter()
                .any(|s| !s.starts_with('/') || s.len() > 256 || s.contains("//"))
        {
            return Err(ControlError::new(
                "invalid_arguments",
                "fields needs 1..32 result JSON pointers starting with /",
            ));
        }
    }
    if request.view.as_deref().is_some_and(|v| {
        !matches!(v, "handover" | "resume" | "compact")
            || !matches!(request.command.as_str(), "status" | "state.get")
    }) {
        return Err(ControlError::new(
            "invalid_arguments",
            "Views handover/resume/compact are supported on status/state.get",
        ));
    }
    Ok(())
}
fn insert(out: &mut Value, path: &[String], value: Value) -> Result<(), ControlError> {
    if path.is_empty() {
        *out = value;
        return Ok(());
    }
    if path[0].parse::<usize>().is_ok() && (out.is_null() || out.is_array()) {
        let index = path[0].parse::<usize>().unwrap();
        if index > 512 {
            return Err(ControlError::new(
                "invalid_arguments",
                "Array selection index exceeds 512",
            ));
        }
        if out.is_null() {
            *out = json!([]);
        }
        let array = out.as_array_mut().unwrap();
        array.resize_with(array.len().max(index + 1), || Value::Null);
        insert(&mut array[index], &path[1..], value)
    } else {
        if out.is_null() {
            *out = json!({});
        }
        let object = out.as_object_mut().ok_or_else(|| {
            ControlError::new(
                "invalid_arguments",
                "Overlapping field masks have incompatible shapes",
            )
        })?;
        insert(
            object.entry(path[0].clone()).or_insert(Value::Null),
            &path[1..],
            value,
        )
    }
}
pub fn apply(request: &Request, mut response: Response) -> Response {
    if !response.ok {
        return response;
    }
    if let (Some(fields), Some(original)) = (&request.fields, response.result.as_ref()) {
        let mut out = Value::Null;
        for field in fields {
            let Some(value) = original.pointer(field) else {
                return Response::failure(
                    request.request_id.clone(),
                    ControlError::new("field_unknown", format!("Unknown result pointer {field}")),
                );
            };
            let path: Vec<_> = field
                .split('/')
                .skip(1)
                .map(|s| s.replace("~1", "/").replace("~0", "~"))
                .collect();
            if let Err(error) = insert(&mut out, &path, value.clone()) {
                return Response::failure(request.request_id.clone(), error);
            }
        }
        response.result = Some(out);
    }
    response
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn planner_refreshes_are_reads_and_preserve_field_selection() {
        for command in ["mix.options", "mix.status", "mix.session.status"] {
            assert!(
                is_read(command),
                "Polling must not create mutation receipts: {command}"
            );
            let mut request = Request::new(command, json!({}));
            request.fields = Some(vec!["/state".into()]);
            validate(&request).unwrap();
            let response = apply(
                &request,
                Response::success(
                    &request,
                    "applied",
                    json!({"state":"ready","evidence":[1,2,3]}),
                ),
            );
            assert_eq!(response.result, Some(json!({"state":"ready"})));
        }
        for command in [
            "mix.execute",
            "mix.prepare",
            "mix.preview",
            "mix.arm",
            "mix.cancel",
            "mix.index_scope",
            "mix.index_scope.cancel",
            "mix.session.start",
            "mix.session.stop",
        ] {
            assert!(
                !is_read(command),
                "Control actions remain mutations: {command}"
            );
        }
    }
    #[test]
    fn masks_preserve_envelope_and_array_shape() {
        let mut r = Request::new("status", json!({}));
        r.fields = Some(vec!["/engine/decks/1/grid_revision".into()]);
        let response = apply(
            &r,
            Response::success(
                &r,
                "applied",
                json!({"engine":{"decks":[{}, {"grid_revision":8,"large":[]}]}}),
            ),
        );
        assert_eq!(response.stage, "applied");
        assert_eq!(
            response.result.unwrap()["engine"]["decks"],
            json!([null,{"grid_revision":8}])
        );
    }
    #[test]
    fn mutations_and_unknown_masks_fail() {
        let mut r = Request::new("engine.command", json!({"type":"play","deck":0}));
        r.fields = Some(vec!["/engine".into()]);
        assert!(validate(&r).is_err());
        r.command = "status".into();
        let response = apply(&r, Response::success(&r, "applied", json!({})));
        assert_eq!(response.error.unwrap().code, "field_unknown");
    }
}
#[cfg(test)]
mod edit_forms {
    use crate::*;
    use serde_json::json;
    #[test]
    fn cue_forms_and_identities_are_exclusive() {
        for args in [
            json!({"source":"local:1","point":{"start_seconds":1},"bind":{"kind":"main","id":"a"}}),
            json!({"source":"local:1","deck":"A","delete_id":"a"}),
            json!({"deck":"A","point":{"start_seconds":1},"seconds":2}),
        ] {
            assert!(validate_arguments(&Request::new("cues.set", args)).is_err());
        }
        assert!(validate_arguments(&Request::new(
            "cues.set",
            json!({"source":"local:1","point":{"start_seconds":1},"expected_revision":2})
        ))
        .is_ok());
    }
    #[test]
    fn energy_schema_is_canonical_while_legacy_aliases_remain_valid() {
        let d = catalog()
            .into_iter()
            .find(|x| x["command"] == "library.suggest_next")
            .unwrap();
        assert_eq!(
            d["input_schema"]["properties"]["energy_direction"]["enum"],
            json!(["keep", "raise", "lower"])
        );
        for direction in ["keep", "raise", "lower", "up", "down"] {
            assert!(validate_arguments(&Request::new(
                "library.suggest_next",
                json!({"energy_direction":direction})
            ))
            .is_ok());
        }
    }
}

#[cfg(test)]
mod library_read_tests {
    use super::*;
    #[test]
    fn metadata_getters_are_reads_and_accept_field_masks() {
        for command in ["library.details", "library.tag_categories"] {
            assert!(is_read(command));
            let mut request = Request::new(command, serde_json::json!({}));
            request.fields = Some(vec!["/fields".into()]);
            assert!(validate(&request).is_ok());
        }
        assert!(!is_read("library.edit_details"));
        assert!(!is_read("library.edit_taxonomy"));
        assert!(!is_read("library.write_metadata"));
    }
}
