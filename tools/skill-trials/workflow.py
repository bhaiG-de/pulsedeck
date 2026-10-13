#!/usr/bin/env python3
"""Integration trial of the skills' shell workflow; not a runtime DJ strategy."""
import json
from pathlib import Path
import struct
import subprocess
import sys
import time

manifest = json.loads(Path(sys.argv[1]).read_text())
out = Path(sys.argv[1]).parent
trace = []
checks = []
def run(*words, error=None):
    command = [manifest["cli"], *map(str, words), "--socket", manifest["socket"], "--json"]
    p = subprocess.run(command, capture_output=True, text=True, timeout=185)
    r = json.loads(p.stdout)
    trace.append({"command":command,"response":r,"exit_code":p.returncode})
    if error:
        assert not r["ok"] and r["error"]["code"] == error and p.returncode, r
    else:
        assert r["ok"] and p.returncode == 0, r
    return r
def call(command, args=None, error=None, revision=None):
    extra = [] if revision is None else ["--expected-load-revision", str(revision)]
    return run("call", command, "--args", json.dumps(args or {}), *extra, error=error)
def data(command, args=None):
    return call(command, args).get("result")
def passed(label):
    checks.append(label)
    print("PASS " + label, flush=True)
def guard(deck, words, revision):
    return run("deck",deck,*words,"--expected-load-revision",revision)

try:
    status = data("status")
    assert status["isolated"] and status["instance"] == manifest["pid"]
    assert status["engine"]["running"] and status["engine"]["master_gain"] == 0 and status["engine"]["cue_gain"] == 0
    catalog = data("commands.list")
    for name in ["track.inspect","deck.load","engine.command","cues.set","batch.run","automation.ramp","engine.wait","audio.start","midi.connect","recording.start","library.export_xml"]:
        data("commands.describe", {"name":name})
    passed("live discovery, schema availability and muted isolated runtime")
    a,b = manifest["tracks"][:2]
    inspection = data("track.inspect", {"source":b["source"],"points":48})
    intro = data("track.inspect", {"source":b["source"],"start_seconds":0,"end_seconds":12,"points":32})
    body = data("track.inspect", {"source":b["source"],"start_seconds":20,"end_seconds":32,"points":32})
    mean = lambda v,key: sum(x[key] for x in v["bins"])/len(v["bins"])
    assert mean(body,"low") > mean(intro,"low")
    assert inspection["metadata"]["duration_seconds"] == b["duration_seconds"]
    passed("existing band evidence shows a reduced-bass entry candidate with original timestamps")
    playlist = data("playlist.create", {"name":"Skill workflow demonstration"})
    data("playlist.edit", {"playlist":playlist["id"],"action":"replace","tracks":[a["id"],b["id"]]})
    assert next(p for p in data("playlist.list") if p["id"]==playlist["id"])["track_ids"] == [a["id"],b["id"]]
    passed("ordered local playlist verified by saved membership order")
    run("mixer","crossfader","set","-1")
    old = data("engine.transport")["load_revisions"]
    guard("A",["fader","set","1"],old[0])
    guard("B",["fader","set","0"],old[1])
    ra = guard("A",["load","--track",a["source"]],old[0])["result"]["load_revision"]
    guard("A",["seek","--at","80s"],ra)
    guard("A",["play"],ra)
    before = data("engine.transport")
    load = guard("B",["load","--track",b["source"]],old[1])
    assert load["stage"] == "ready"
    rb = load["result"]["load_revision"]
    snap = data("engine.transport")
    percent = 100 * (snap["decks"][0]["bpm"] * snap["decks"][0]["playback_rate"] / snap["decks"][1]["bpm"] - 1)
    for words in [["sync","off"],["tempo","set","--percent",str(percent)],["keylock","on"],["loop","off"],["seek","--at","16s"],["headphones","on"],["wait"]]:
        guard("B",words,rb)
    snap = data("engine.transport")
    assert snap["load_revisions"] == [ra,rb] and snap["decks"][0]["playing"]
    assert snap["decks"][0]["position_seconds"] > before["decks"][0]["position_seconds"]
    assert snap["decks"][1]["position_seconds"] == 16 and not snap["decks"][1]["playing"]
    assert snap["decks"][1]["channel_fader"] == 0 and not snap["decks"][1]["preparation_pending"]
    passed("guarded incoming preparation while outgoing deck advances")
    cues = data("cues.get", {"deck":"B"})
    position_revision = snap["decks"][1]["position_revision"]
    persisted = call("cues.set",{"deck":"B","kind":"hot","index":0,"seconds":8,"label":"Candidate reduced-bass intro","expected_revision":cues["revision"]},revision=rb)
    assert persisted["stage"] == "persisted"
    snap = data("engine.transport")
    assert snap["decks"][1]["position_seconds"] == 16 and snap["decks"][1]["position_revision"] == position_revision
    assert data("cues.get",{"source":b["source"]})["cues"]["hot_cues"][0] == 8
    call("engine.wait",{"deck":"B"},revision=rb-1,error="source_changed")
    call("automation.ramp",{"target":{"type":"eq_low","deck":"B"},"value":0,"seconds":0.1},revision=rb-1,error="source_changed")
    passed("persisted cue preserves position; stale readiness and ramp guards reject")
    guard("B",["eq","set","--low","0"],rb)
    guard("B",["fader","set","1"],rb)
    guard("B",["play"],rb)
    steps = [
        {"command":"automation.ramp","expected_load_revision":ra,"args":{"target":{"type":"crossfader"},"value":1,"clock":"A","beats":4}},
        {"command":"automation.ramp","expected_load_revision":ra,"args":{"target":{"type":"eq_low","deck":"A"},"value":0,"seconds":1}},
        {"command":"automation.ramp","expected_load_revision":rb,"args":{"target":{"type":"eq_low","deck":"B"},"value":1,"seconds":1}},
        {"command":"engine.wait","expected_load_revision":rb,"args":{"deck":"B","ramps":True}},
    ]
    data("batch.validate",{"steps":steps})
    job = data("batch.run",{"steps":steps})["job_id"]
    deadline = time.monotonic()+15
    while True:
        state = data("jobs.get",{"id":job})
        if state["state"] != "running": break
        assert time.monotonic()<deadline
        time.sleep(0.05)
    assert state["state"] == "completed", state
    snap = data("engine.transport")
    assert snap["crossfader"] == 1 and not any(snap["active_ramps"])
    assert abs(snap["decks"][0]["eq_low"])<1e-5 and abs(snap["decks"][1]["eq_low"]-1)<1e-5
    assert snap["decks"][1]["playing"] and snap["decks"][1]["position_seconds"] > 16
    guard("A",["fader","set","0"],ra)
    guard("A",["pause"],ra)
    passed("short native bass/fader handover completes before outgoing pause")
    recording = Path(manifest["profile"])/"skill-workflow.wav"
    data("recording.start",{"path":str(recording)})
    time.sleep(0.15)
    final = data("recording.stop")
    assert not final["active"] and final["frames_written"] > 0 and final["dropped_frames"] == 0 and final["error"] is None
    raw = recording.read_bytes()
    assert all(abs(v)<1e-6 for (v,) in struct.iter_unpack("<f",raw[44:]))
    export = Path(manifest["profile"])/"skill-library.xml"
    data("library.export_xml",{"path":str(export)})
    assert export.exists() and "DJ_PLAYLISTS" in export.read_text()
    passed("eligible mix finalizes silent WAV and explicit-path library XML")
    data("automation.ramp",{"target":{"type":"filter","deck":"B"},"value":0.2,"seconds":5})
    data("automation.stop")
    snap = data("engine.transport")
    assert not any(snap["active_ramps"]) and snap["decks"][1]["playing"]
    assert snap["master_gain"] == 0 and snap["cue_gain"] == 0
    passed("takeover cancels native ramp while preserving current playback and silence")
    report = {"passed":len(checks),"checks":checks,"plan":{"sources":[a["source"],b["source"]],"load_revisions":[ra,rb],"outgoing_region_seconds":[80,96],"incoming_entry_seconds":16,"meter_assumption":"provisional 4/4; waveform does not prove downbeat","overlap_bars":1,"overlap_beats":4,"nominal_overlap_seconds":2,"strategy":"short technical handover; reduced-bass outgoing region and stable incoming pulse hypothesis","exact_phrase_start_claimed":False},"audible_verification":False,"trace":trace}
    (out/"workflow.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps({"passed":len(checks),"report":str(out/"workflow.json")}))
except BaseException:
    (out/"failed-trace.json").write_text(json.dumps(trace,indent=2)+"\n")
    raise
