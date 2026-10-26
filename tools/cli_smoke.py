#!/usr/bin/env python3
"""Exercise a running isolated desktop through the actual CLI. No credentials needed."""
import argparse
import json
import math
import pathlib
import struct
import subprocess
import time
import wave

parser = argparse.ArgumentParser()
parser.add_argument("--socket", required=True)
parser.add_argument("--cli", default="target/debug/pulsedeckctl")
parser.add_argument("--work-dir", required=True)
parser.add_argument("--audio", action="store_true")
parser.add_argument("--restart-check", action="store_true")
opts = parser.parse_args()
work = pathlib.Path(opts.work_dir).resolve()
work.mkdir(parents=True, exist_ok=True)
checks = []

def run(*args, expected=None):
    command = [opts.cli, *map(str, args), "--socket", opts.socket, "--json"]
    completed = subprocess.run(command, capture_output=True, text=True, timeout=45)
    try:
        response = json.loads(completed.stdout)
    except Exception as error:
        raise AssertionError(f"Invalid stdout for {args}: {completed.stdout!r}; {completed.stderr}") from error
    if expected:
        assert not response["ok"], response
        assert response["error"]["code"] == expected, response
        assert response["error"]["details"].get("hint"), response
        assert completed.returncode != 0
    else:
        assert response["ok"], response
        assert completed.returncode == 0, completed.stderr
    return response

def call(method, args=None, expected=None, extra=()):
    return run("call", method, "--args", json.dumps(args or {}), *extra, expected=expected)

def data(method, args=None):
    return call(method, args).get("result")

def passed(name):
    checks.append(name)
    print(f"PASS {name}", flush=True)

def until(predicate, seconds=8):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(0.05)
    raise AssertionError("Condition did not become true before deadline")

status = data("status")
assert status["isolated"], "This suite only runs against PULSEDECK_ISOLATED=1"
assert pathlib.Path(opts.socket).parent.resolve() == work, "Socket and fixture profile must share the explicit temporary directory"
passed("isolated runtime owns the control socket")

if opts.restart_check:
    fixture = json.loads((work / "expected.json").read_text())
    cues = data("cues.get", {"source": fixture["source"]})
    assert cues["cues"]["hot_cues"][7] == 7.0, cues
    assert cues["cues"]["annotations"]["hot:7"] == "Outro", cues
    assert next(p for p in data("playlist.list") if p["name"] == "CLI integration set")["track_ids"] == fixture["order"]
    run("deck", "A", "load", "--track", fixture["source"])
    deck = data("engine.transport")["decks"][0]
    assert deck["hot_cues"][7] == 7.0, deck
    passed("restart restores persisted source cues, annotations, playlists and loaded markers")
else:
    assert data("library.tracks")["total"] == 0, "Use a fresh isolated library"
    diagnostic = run("deck", "A", "tempo", "set", "--precent", "2", expected="invalid_arguments")
    assert "percent" in diagnostic["error"]["details"]["suggestions"]
    call("engine.command", {"type": "set_temo", "deck": "A", "percent": 2}, expected="invalid_arguments")
    call("engine.command", {"type": "set_tempo", "deck": "A", "percent": 999}, expected="invalid_arguments")
    call("engine.command", {"type": "set_tempo", "deck": "A", "precent": 2}, expected="invalid_arguments")
    run("deck", "B", "play", expected="deck_empty")
    passed("typo, unknown argument, range and empty-deck errors explain repair")
    catalog = data("commands.list", {"detail": True})
    native_types = {c["type"] for c in catalog["engine_commands"]}
    assert len(native_types) == len(catalog["engine_commands"])
    assert {"play_quantized", "ramp", "scrub_to", "set_eq", "restore_memory_cues"} <= native_types
    schedule = next(c for c in catalog["commands"] if c["command"] == "batch.run")["input_schema"]["properties"]["schedule"]
    assert {"clock", "recovery", "completion_event"} <= set(schedule["required"])
    assert "expected_grid_revisions" in schedule["properties"]
    # Mechanical guards can be captured coherently by the runtime. Explicit
    # overrides remain discoverable and are covered by the backend guard tests.
    assert "expected_grid_revisions" not in schedule["required"]
    assert any(c["command"] == "engine.command" and "native_input_schema" in c for c in catalog["commands"])
    passed("runtime command discovery includes native schemas and unsupported capabilities")
    sources = []
    for index in range(2):
        path = work / f"song-{index + 1}.wav"
        with wave.open(str(path), "wb") as output:
            output.setnchannels(2)
            output.setsampwidth(2)
            output.setframerate(48000)
            frames = bytearray()
            for frame in range(12 * 48000):
                value = int(7000 * math.sin(2 * math.pi * (220 + index * 110) * frame / 48000))
                frames.extend(struct.pack("<hh", value, value))
            output.writeframes(frames)
        imported = data("library.import", {"paths": [str(path)]})
        sources.append(f"local:{imported[0]['id']}")
    assert data("library.tracks", {"limit": 1})["next_offset"] == 1
    assert data("library.tracks", {"search": "song-1"})["total"] == 1
    passed("actual WAV import, analysis, bounded search and pagination")
    playlist = data("playlist.create", {"name": "CLI integration set"})
    for source in sources:
        data("playlist.add", {"playlist": playlist["id"], "track": source})
    order = [int(sources[1].split(":")[1]), int(sources[0].split(":")[1])]
    data("playlist.edit", {"playlist": playlist["id"], "action": "replace", "tracks": order})
    assert data("playlist.list")[0]["track_ids"] == order
    passed("playlist creation, membership and transactional reorder")
    loaded = run("deck", "A", "load", "--track", sources[0])["result"]
    revision = loaded["load_revision"]
    run("deck", "B", "load", "--track", sources[0])
    run("deck", "A", "seek", "--at", "2s")
    run("deck", "A", "hotcue", "H", "set", "--at", "7s", "--expected-load-revision", revision)
    snapshot = data("engine.transport")
    assert snapshot["decks"][0]["position_seconds"] == 2.0
    assert not snapshot["decks"][0]["playing"]
    assert snapshot["decks"][1]["hot_cues"][7] == 7.0
    cues = data("cues.get", {"source": sources[0]})
    data("cues.set", {"source": sources[0], "kind": "hot", "index": 7, "seconds": 7.0, "label": "Outro", "expected_revision": cues["revision"]})
    data("cues.set", {"source": sources[0], "kind": "memory", "index": 0, "seconds": 6.0, "label": "Break"})
    data("cues.set", {"source": sources[0], "kind": "memory", "index": 1, "seconds": 3.0, "label": "Intro"})
    cues = data("cues.get", {"source": sources[0]})
    assert cues["cues"]["memory_cues"][:2] == [3.0, 6.0]
    assert cues["cues"]["annotations"]["memory:1"] == "Break"
    call("cues.set", {"source": sources[0], "kind": "hot", "index": 0, "seconds": 1.0, "expected_revision": 0}, expected="revision_conflict")
    passed("non-disruptive durable cues, labels, chronological memory points and shared decks")
    run("deck", "A", "load", "--demo")
    run("deck", "A", "hotcue", "A", "set", "--at", "1s", "--expected-load-revision", revision, expected="source_changed")
    assert data("engine.transport")["decks"][0]["hot_cues"][0] is None
    run("deck", "A", "load", "--track", sources[0])
    passed("stale source guards prevent edits reaching another track")
    run("deck", "A", "eq", "set", "--low", "0.2")
    data("controller.action", {"action": {"type": "knob", "deck": 0, "knob": "eq_mid", "raw": 12287, "value": 0.75}})
    deck = data("engine.transport")["decks"][0]
    assert abs(deck["eq_low"] - 0.2) < 1e-5 and abs(deck["eq_mid"] - 1.5) < 1e-5
    run("deck", "A", "cue", "set", "--at", "1s")
    run("deck", "A", "cue", "press")
    assert data("engine.transport")["decks"][0]["playing"]
    run("deck", "A", "cue", "release")
    assert not data("engine.transport")["decks"][0]["playing"]
    passed("CLI and controller preserve shared EQ state and cue press/release behavior")
    first = run("deck", "A", "toggle-play", "--request-id", "smoke-toggle")
    repeat = run("deck", "A", "toggle-play", "--request-id", "smoke-toggle")
    assert first == repeat and data("engine.transport")["decks"][0]["playing"]
    run("deck", "A", "pause", "--request-id", "smoke-toggle", expected="request_id_conflict")
    run("deck", "A", "pause")
    passed("uncertain retries deduplicate mutations and reject conflicting request IDs")
    run("sampler", "slot", "16", "load", "--path", work / "song-2.wav")
    run("sampler", "slot", "16", "trigger")
    assert data("sampler.state")["slots"][15]["playing"]
    run("sampler", "slot", "16", "stop")
    passed("sampler bank two assignment and observable playback")
    ui = data("ui.state")
    assert "app" in ui
    run("ui", "theme", "set", "ivory")
    run("ui", "app", "zoom", "64")
    run("ui", "app", "browser", "true")
    run("ui", "drawer", "height", "44")
    run("ui", "deck-A", "pads", "memory")
    run("ui", "app", "sampler", "true")
    run("ui", "app", "sampler-bank", "2")
    ui = data("ui.state")
    assert ui["app"]["state"]["theme"] == "ivory"
    assert ui["app"]["state"]["zoom"] == 64
    assert ui["drawer"]["state"]["height"] == 44
    assert ui["deck-A"]["state"]["pads"] == "memory"
    invalid_zoom = run("ui", "app", "zoom", "129", expected="invalid_arguments")
    assert invalid_zoom["error"]["details"]["expected"]["maximum"] == 128
    assert '"action":"zoom"' in invalid_zoom["error"]["details"]["example"]
    run("ui", "app", "does-not-exist", "true", expected="command_unsupported")
    call("ui.call", {"target": "unmounted", "action": "open"}, expected="ui_unavailable")
    passed("real mounted UI commits and acknowledges theme, zoom, drawer, pads and sampler")
    data("app.show")
    run("ui", "deck-A", "grid-editor", "true")
    def grid_metadata_ready():
        try:
            run("ui", "deck-A", "grid-draft", "--value", json.dumps({"bpm": 125, "offset": 0.1}))
            return True
        except AssertionError as error:
            response = error.args[0]
            # Transport and track metadata arrive independently in the UI.
            if isinstance(response, dict) and response.get("error", {}).get("code") == "invalid_arguments" and response["error"].get("details", {}).get("expected", {}).get("maximum") == 0:
                return False
            raise
    until(grid_metadata_ready)
    run("ui", "deck-A", "grid-apply")
    assert data("engine.transport")["decks"][0]["bpm"] == 125
    run("ui", "app", "settings", "true")
    run("ui", "app", "settings-tab", "spotify")
    until(lambda: "spotify-settings" in data("ui.state"))
    run("ui", "spotify-settings", "client-id", "00000000000000000000000000000000")
    run("ui", "app", "settings", "false")
    passed("grid editor executes native changes and settings have explicit mounted targets")
    export = work / "library.xml"
    data("library.export_xml", {"path": str(export)})
    call("library.export_xml", {"path": str(export)}, expected="file_exists")
    data("library.export_xml", {"path": str(export), "overwrite": True})
    assert "DJ_PLAYLISTS" in export.read_text()
    passed("XML export is explicit and protects existing output")
    if opts.audio:
        run("mixer", "master-gain", "set", "0")
        run("mixer", "cue-gain", "set", "0")
        data("audio.start", {"buffer_frames": 256})
    data("deck.unload", {"deck": "B"})
    steps = [
        {"command": "playlist.create", "args": {"name": "Batch first step"}},
        {"command": "engine.command", "args": {"type": "play", "deck": "B"}},
        {"command": "playlist.create", "args": {"name": "Must never be created"}},
    ]
    assert data("batch.validate", {"steps": steps})["valid"]
    job = data("batch.run", {"steps": steps})["job_id"]
    final = until(lambda: (j := data("jobs.get", {"id": job}))["state"] != "running" and j)
    assert final["state"] == "failed", final
    assert [action["state"] for action in final["actions"][:2]] == ["executed", "failed"], final
    assert final["actions"][2]["state"] in {"cancelled", "failed"}, final
    if opts.audio:
        assert final["completed"] == 2, final
    names = [playlist["name"] for playlist in data("playlist.list")]
    assert "Batch first step" in names and "Must never be created" not in names
    passed("batch partial failure stops later mutations and returns step errors")
    if opts.audio:
        data("automation.ramp", {"target": {"type": "crossfader"}, "value": 1.0, "seconds": 1.0})
        job = data("batch.run", {"steps": [{"command": "engine.wait", "args": {"ramps": True}}]})["job_id"]
        data("jobs.cancel", {"id": job})
        final = until(lambda: (j := data("jobs.get", {"id": job}))["state"] != "running" and j)
        assert final["state"] == "cancelled", final
        data("automation.stop")
        passed("job cancellation interrupts active waits and stops native ramps")
    assert data("events.get")["events"]
    call("spotify.devices", expected="configuration_required")
    data("spotify.configure", {"client_id": "00000000000000000000000000000000"})
    call("spotify.devices", expected="auth_required")
    call("controller.action", {"action": {"type": "knob", "deck": None, "knob": "mic_gain", "raw": 0, "value": 0.5}}, expected="command_unsupported")
    passed("events, provider authentication and unsupported controls have explicit results")
    (work / "expected.json").write_text(json.dumps({"source": sources[0], "order": order}))
    if opts.audio:
        run("mixer", "master-gain", "set", "0")
        run("mixer", "cue-gain", "set", "0")
        data("audio.start", {"buffer_frames": 256})
        time.sleep(0.25)
        run("deck", "A", "play")
        before = data("engine.transport")["decks"][0]["position_seconds"]
        time.sleep(0.15)
        assert data("engine.transport")["decks"][0]["position_seconds"] > before
        data("automation.ramp", {"target": {"type": "crossfader"}, "value": -1.0, "seconds": 0.1})
        data("engine.wait", {"ramps": True})
        transport = data("engine.transport")
        assert transport["crossfader"] == -1.0, {
            key: transport[key] for key in ("crossfader", "active_ramps", "control_revision")
        }
        recording = work / "silent-mix.wav"
        data("recording.start", {"path": str(recording)})
        call("audio.start", {"buffer_frames": 512}, expected="operation_failed")
        assert data("recording.status")["active"]
        assert data("engine.transport")["running"]
        time.sleep(0.2)
        result = data("recording.stop")
        assert result["frames_written"] > 0 and result["dropped_frames"] == 0
        raw = recording.read_bytes()
        assert raw[:4] == b"RIFF" and raw[8:12] == b"WAVE"
        assert all(abs(v) < 1e-6 for (v,) in struct.iter_unpack("<f", raw[44:])), "Muted recording must be silent"
        run("deck", "A", "pause")
        data("audio.stop")
        passed("silent real CoreAudio playback, native timed ramp and finalized WAV recording")

report = {"passed": len(checks), "checks": checks, "physical_midi_tested": False, "live_spotify_tested": False, "audio_tested": opts.audio, "restart_check": opts.restart_check}
(work / ("restart-results.json" if opts.restart_check else "cli-results.json")).write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report), flush=True)
