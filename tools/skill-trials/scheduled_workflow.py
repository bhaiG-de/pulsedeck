#!/usr/bin/env python3
"""Exercise the shared CLI in an existing, muted profile.py --audio profile.

Usage: python3 tools/skill-trials/scheduled_workflow.py artifacts/skill-trials/NAME/profile.json
Run after rebuilding the app AND CLI. This is a generated-audio integration trial,
not a DJ strategy or listening test. It creates no recording and never uses the
default socket. Evidence/plan files go beside the supplied profile manifest.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import time
import uuid
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[2]


class Trial:
    def __init__(self, manifest_path):
        self.out = manifest_path.resolve().parent
        self.manifest = json.loads(manifest_path.read_text())
        self.profile = Path(self.manifest["profile"]).resolve()
        self.socket = Path(self.manifest["socket"]).resolve()
        self.cli = Path(self.manifest["cli"]).resolve()
        self.trace, self.checks, self.plans, self.jobs = [], [], [], []
        self.token = uuid.uuid4().hex[:12]
        self.started = time.monotonic()
        self.authorized = False
        self.require(self.profile.parent == Path("/tmp").resolve()
                     and self.profile.name.startswith("pd-skill-"), "Not a profile.py temporary profile")
        self.require(self.socket == self.profile / "control.sock", "Socket is outside this profile")
        self.require(self.cli == (ROOT / "target/debug/pulsedeckctl").resolve(), "Unexpected CLI binary")
        self.require(self.manifest.get("audio") is True, "Start profile.py with --audio")
        self.require(len(self.manifest["tracks"]) >= 3, "Three generated fixtures required")
        self.hashes = {}
        for track in self.manifest["tracks"]:
            path = Path(track["path"]).resolve()
            self.require(path.parent == self.profile and path.suffix == ".wav"
                         and track["source"] == f"local:{track['id']}"
                         and "generated_bpm" in track, "Only profile.py generated WAVs are allowed")
            self.hashes[str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()

    @staticmethod
    def require(condition, message):
        if not condition:
            raise AssertionError(message)

    def run(self, *words, reject=None):
        # Before the initial safety read, no other command may reach the socket.
        self.require(self.authorized or words == ("status",), "Safety read must precede all commands")
        command = [str(self.cli), *map(str, words), "--socket", str(self.socket), "--json"]
        start = time.monotonic()
        process = subprocess.run(command, text=True, capture_output=True, timeout=35)
        envelopes = [json.loads(line) for line in process.stdout.splitlines() if line.strip()]
        self.trace.append({"argv": command, "elapsed_seconds": time.monotonic() - start,
                           "exit_code": process.returncode, "responses": envelopes,
                           "stderr": process.stderr})
        self.require(bool(envelopes), f"No JSON response: {process.stderr}")
        if reject is not None:
            failures = [r for r in envelopes if not r.get("ok")]
            self.require(bool(process.returncode) and bool(failures), "Invalid request was accepted")
            if reject != "any":
                self.require(failures[-1]["error"]["code"] == reject, str(failures[-1]))
            return failures[-1]
        self.require(process.returncode == 0 and all(r.get("ok") for r in envelopes), str(envelopes))
        return envelopes[-1].get("result")

    def call(self, method, args=None, revision=None, reject=None):
        flags = [] if revision is None else ["--expected-load-revision", revision]
        return self.run("call", method, "--args", json.dumps(args or {}), *flags, reject=reject)

    def safety(self):
        status = self.run("status")
        engine = status["engine"]
        self.require(status["isolated"] is True and status["instance"] == self.manifest["pid"],
                     "Refusing any mutation: instance does not match this isolated profile")
        self.require(engine["running"] and engine["master_gain"] == 0 and engine["cue_gain"] == 0,
                     "Refusing any mutation: renderer must be running with master AND cue muted")
        self.require(all(abs(peak) < 1e-7 for peak in engine["master_peak"] + engine["cue_peak"]),
                     "Refusing any mutation: muted output meters must also be silent")
        self.authorized = True
        return status

    def passed(self, message):
        self.checks.append(message)
        print("PASS " + message, flush=True)

    def deck(self, index, *words):
        revision = self.call("engine.transport")["load_revisions"][index]
        return self.run("deck", "AB"[index], *words, "--expected-load-revision", revision)

    def load_uniform(self, index, track):
        self.deck(index, "pause")
        loaded = self.deck(index, "load", "--track", track["source"])
        revision = loaded["load_revision"]
        for words in [("sync", "off"), ("tempo", "set", "--percent", 0),
                      ("bpm", "set", 119), ("bpm", "set", 120), ("grid", "set", "--offset", "0.25s"),
                      ("keylock", "on"), ("loop", "off"), ("wait",)]:
            self.run("deck", "AB"[index], *words, "--expected-load-revision", revision)
        return revision

    def cues(self, track):
        return self.call("cues.get", {"source": track["source"]})

    def edit_cues(self, track, **mutation):
        before = self.cues(track)
        return self.call("cues.set", {"source": track["source"],
                                     "expected_revision": before["revision"], **mutation})

    def metadata_trial(self):
        a, b, candidate = self.manifest["tracks"][:3]
        # Seed explicit known cached tempos; no neural/arrangement inference.
        self.load_uniform(0, a)
        self.load_uniform(1, candidate)
        self.load_uniform(1, b)
        before = self.call("engine.transport")
        name = "Trial & 回憶 <entry> " + self.token
        point = self.edit_cues(b, point={"start_seconds": 4, "end_seconds": 8,
                                       "name": "Draft " + self.token, "role": "intro"})
        cue_id = next(p["id"] for p in point["cues"]["points"] if p["name"] == "Draft " + self.token)
        self.edit_cues(b, point={"id": cue_id, "name": name, "start_seconds": 4.5})
        for kind in ["hot", "memory", "main"]:
            bind = {"kind": kind, "id": cue_id}
            if kind != "main":
                bind["index"] = 0
            self.edit_cues(b, bind=bind)
        persisted = self.cues(b)["cues"]
        self.require(persisted["bindings"]["hot"][0] == cue_id
                     and persisted["bindings"]["memory"][0] == cue_id
                     and persisted["bindings"]["main"] == cue_id, "Bindings do not share canonical ID")
        throwaway = self.edit_cues(b, point={"name": "Delete " + self.token, "start_seconds": 12})
        deleted = next(p["id"] for p in throwaway["cues"]["points"] if p["name"] == "Delete " + self.token)
        self.edit_cues(b, bind={"kind": "hot", "index": 1, "id": deleted})
        self.edit_cues(b, delete_id=deleted)
        after = self.cues(b)
        self.require(not any(p["id"] == deleted for p in after["cues"]["points"])
                     and after["cues"]["bindings"]["hot"][1] is None, "Deletion left a point/binding")
        self.call("cues.set", {"source": b["source"], "expected_revision": after["revision"] - 1,
                               "point": {"id": cue_id, "name": "Stale edit"}}, reject="revision_conflict")
        after_transport = self.call("engine.transport")
        for i in range(2):
            for field in ["position_seconds", "position_revision", "playing", "grid_revision"]:
                self.require(before["decks"][i][field] == after_transport["decks"][i][field],
                             "Cue edit changed transport/grid: " + field)
        xml = self.profile / ("scheduled-cues-" + self.token + ".xml")
        self.run("library", "export-rekordbox", "--path", xml)
        document = ET.parse(xml)
        marks = [mark for track in document.findall("./COLLECTION/TRACK")
                 for mark in track.findall("POSITION_MARK") if mark.get("Name") == name]
        self.require(len(marks) >= 2 and all(abs(float(m.get("Start")) - 4.5) < .001 for m in marks),
                     "Named hot/memory cues did not export escaped names/times")
        self.run("library", "import-rekordbox", "--path", xml)
        imported = self.cues(b)["cues"]
        for kind in ["hot", "memory"]:
            identity = imported["bindings"][kind][0]
            cue = next(p for p in imported["points"] if p["id"] == identity)
            self.require(cue["name"] == name and abs(cue["start_seconds"] - 4.5) < .001,
                         "XML did not roundtrip canonical named cue")
        self.passed("canonical cue create/edit/shared bindings/delete, stale revision and named XML roundtrip")

        playlist = self.call("playlist.create", {"name": "Scheduled trial " + self.token})
        self.call("playlist.edit", {"playlist": playlist["id"], "action": "replace",
                                    "tracks": [a["id"], candidate["id"]]})
        cache_before = self.safety()
        for alias, direction in [("keep", "keep"), ("up", "raise"), ("down", "lower")]:
            ranked = self.run("library", "suggest", "--deck", "A", "--energy", alias,
                              "--provider", "local", "--playlist", f"local:{playlist['id']}",
                              "--sources", json.dumps([candidate["path"]]),
                              "--max-stretch-percent", 3, "--allow-half-double=false", "--limit", 5)
            self.require(ranked["cached_only"] and ranked["scope_complete"] and ranked["direction"] == direction,
                         "Suggestion aliases/cache/scope contract failed")
            self.require(bool(ranked["suggestions"]), "Cached compatible scoped candidate was not suggested")
            for suggestion in ranked["suggestions"]:
                self.require(suggestion["source"] == candidate["path"] and not suggestion["spotify"]
                             and abs(suggestion["tempo"]["percent"]) <= 3.0001
                             and suggestion["tempo"]["multiple"] == 1, "Suggestion escaped scope/stretch")
        cache_after = self.safety()
        self.require(cache_before["source_jobs"] == cache_after["source_jobs"]
                     and cache_before["analysis"] == cache_after["analysis"],
                     "Cached suggestion reads changed source/analysis job state")
        self.passed("nonempty cached local playlist + source scope and keep/up/down energy aliases")

    def prepare_handover(self):
        self.safety()
        for i in range(2):
            for words in [("pause",), ("sync", "off"), ("tempo", "set", "--percent", 0),
                          ("bpm", "set", 119), ("bpm", "set", 120), ("grid", "set", "--offset", "0.25s"),
                          ("loop", "off"), ("seek", "--at", "0.25s"),
                          ("eq", "set", "--low", 1, "--mid", 1, "--high", 1),
                          ("fader", "set", 1 if i == 0 else 0)]:
                self.deck(i, *words)
        self.deck(1, "eq", "set", "--low", 0)
        self.run("mixer", "crossfader", "set", 0)
        self.deck(0, "play")

    def plan(self, label):
        view = self.call("track.inspect", {"deck": "A", "start_seconds": 0,
                                           "end_seconds": 20, "points": 128})
        self.require(view["beat_events"]["uniform"] is True and not view["beat_events"]["truncated"],
                     "Generated trial requires inspected uniform events")
        snap = self.safety()["engine"]  # All epoch/revision guards from one fresh snapshot.
        events = view["beat_events"]["events"]
        launch = next(e for e in events if e["seconds"] >= snap["decks"][0]["position_seconds"] + 3)
        coordinate = {e["beat_index"]: e["seconds"] for e in events}
        launch_index, entry = launch["beat_index"], snap["decks"][1]["position_seconds"]
        opening, completion = launch_index + 4, launch_index + 10
        revisions = snap["load_revisions"]
        recovery = [{"type": "ramp", "target": {"type": "channel_fader", "deck": i},
                     "value": 1 if i else 0, "seconds": .2, "clock": None,
                     "beats": None, "quantize": None} for i in range(2)]
        recovery.append({"type": "set_eq", "deck": 1, "low": 1, "mid": 1, "high": 1})
        schedule = {"operation_id": f"trial-{self.token}-{label}", "clock": 0,
                    "expected_load_revisions": revisions,
                    "expected_position_revisions": [d["position_revision"] for d in snap["decks"]],
                    "expected_grid_revisions": [d["grid_revision"] for d in snap["decks"]],
                    "expected_engine_epoch": snap["engine_epoch"],
                    "expected_control_revision": snap["control_revision"],
                    "arm_before_event": launch_index - 2, "completion_event": completion,
                    "pause_outgoing": True,
                    "intent": {"fixture_only": True, "neural_claim": False,
                               "entry_seconds": entry, "prepared_follower": self.manifest["tracks"][1]["source"],
                               "performance_baseline": {k: snap[k] for k in
                                                        ["engine_epoch", "rendered_frames", "sample_rate"]}},
                    "recovery": {"carrier": 1, "seconds": .2, "controls": recovery}}
        steps = [{"command": "engine.command", "expected_load_revision": revisions[1],
                  "at_event": launch_index, "require": {"deck": 1, "playing": False, "ready": True},
                  "args": {"type": "play_quantized", "deck": 1, "clock": 0,
                           "clock_revision": revisions[0], "at_seconds": None, "quantize": 1}}]
        prerequisite = {"deck": 1, "playing": True, "ready": True, "advanced_from_seconds": entry,
                        "min_advanced_beats": 3, "phase_tolerance_beats": .01}
        for deck, target, value in [(0, "channel_fader", 0), (1, "channel_fader", 1), (1, "eq_low", 1)]:
            steps.append({"command": "automation.ramp", "expected_load_revision": revisions[deck],
                          "at_event": opening, "require": prerequisite.copy(),
                          "args": {"target": {"type": target, "deck": deck},
                                   "value": value, "clock": 0, "beats": 4}})
        plan = {"schedule": schedule, "steps": steps}
        path = self.out / f"scheduled-{self.token}-{label}.json"
        path.write_text(json.dumps(plan, indent=2) + "\n")
        self.plans.append({"path": str(path), "plan": plan, "observed_engine": snap,
                           "effective_events": events})
        remaining = (coordinate[completion] - snap["decks"][0]["position_seconds"]) / snap["decks"][0]["playback_rate"]
        launch_remaining = (launch["seconds"] - snap["decks"][0]["position_seconds"]) / snap["decks"][0]["playback_rate"]
        return path, plan, remaining, launch_remaining

    def endpoints(self, engine, outgoing_paused=False):
        a, b = engine["decks"]
        self.require(abs(a["channel_fader"]) < .002 and abs(b["channel_fader"] - 1) < .002,
                     "Full carrier/isolation endpoints not reached")
        self.require(b["playing"] and b["position_seconds"] > .25 and not b["preparation_pending"],
                     "Carrier did not advance ready")
        self.require(all(abs(b[field] - 1) < .002 for field in ["eq_low", "eq_mid", "eq_high"])
                     and not any(engine["active_ramps"]) and not any(d["launch_pending"] for d in [a, b]),
                     "EQ/ramp/launch endpoints not settled")
        if outgoing_paused:
            self.require(not a["playing"], "Clock was not paused after verified handover")

    def scheduling_trial(self):
        self.prepare_handover()
        path, plan, remaining, _ = self.plan("complete")
        invalid = copy.deepcopy(plan)
        invalid["schedule"]["operation_id"] += "-stale-source"
        invalid["schedule"]["expected_load_revisions"][1] += 1
        for step in invalid["steps"]:
            target_deck = step["args"].get("deck", step["args"].get("target", {}).get("deck"))
            if target_deck == 1:
                step["expected_load_revision"] += 1
        self.call("batch.validate", invalid, reject="source_changed")
        self.call("batch.run", invalid, reject="source_changed")
        rejected = self.safety()["engine"]
        self.require(rejected["scheduled_batch"]["state"] not in ["armed", "running", "recovering"]
                     and not rejected["decks"][1]["playing"]
                     and not rejected["decks"][1]["launch_pending"], "Rejected plan partially armed/launched")
        self.run("batch", "validate", "--file", path)
        accepted = self.run("batch", "run", "--file", path, "--no-wait")
        job = accepted["job_id"]
        self.jobs.append(job)
        duplicate = self.run("batch", "run", "--file", path, "--no-wait")
        self.require(duplicate["job_id"] == job and duplicate["duplicate"] and duplicate["replayed"] is False,
                     "Identical operation retry replayed or changed identity")
        changed = copy.deepcopy(plan)
        changed["schedule"]["intent"]["different"] = True
        self.call("batch.run", changed, reject="idempotency_conflict")
        # Deliberate model/tool absence: absolutely no IPC or per-phase polls.
        quiet_start = time.monotonic()
        time.sleep(remaining + .8)
        self.trace.append({"no_command_interval_seconds": time.monotonic() - quiet_start,
                           "purpose": "Native queue must launch, advance, ramp and complete without another agent turn"})
        state = self.run("jobs", "get", job)
        self.require(state["state"] == "completed" and state["completed"] == len(plan["steps"]), str(state))
        receipts = state["native"]["events"]
        self.require(len(receipts) == len(plan["steps"])
                     and {r["step"] for r in receipts} == set(range(len(plan["steps"])))
                     and all(r["state"] == 2 for r in receipts), "Missing/duplicate/unverified native receipts")
        resume = self.run("status", "--view", "resume")
        self.require(resume["journal"] == {"durable": True, "replay": False}
                     and resume["jobs"][0]["id"] == job
                     and resume["jobs"][0]["status"]["schedule"]["intent"] == plan["schedule"]["intent"]
                     and resume["jobs"][0]["status"]["sources"] == state["sources"]
                     and resume["jobs"][0]["status"]["native"]["events"] == receipts,
                     "Resume did not retain intent/identity/native receipts without replay")
        self.require(state["sources"]["load_revisions"] == plan["schedule"]["expected_load_revisions"]
                     and [t["path"] for t in state["sources"]["tracks"]]
                     == [t["path"] for t in self.manifest["tracks"][:2]], "Receipt source identities differ")
        self.endpoints(resume["engine"], outgoing_paused=True)
        replay = self.run("batch", "run", "--file", path, "--no-wait")
        self.require(replay["job_id"] == job and replay["replayed"] is False, "Completed retry replayed")
        self.call("events.get")
        self.passed("invalid source plan rejects; one guarded native handover completes across no-command interval; retry/resume retain receipts")

        self.prepare_handover()
        path, plan, _, launch_remaining = self.plan("stabilize")
        accepted = self.run("batch", "run", "--file", path, "--no-wait")
        self.jobs.append(accepted["job_id"])
        time.sleep(launch_remaining + .3)  # No intervention until carrier's isolated launch.
        self.run("jobs", "cancel", accepted["job_id"], "--mode", "stabilize")
        time.sleep(.6)
        state = self.run("jobs", "get", accepted["job_id"])
        self.require(state["state"] == "recovered" and state["reason"] == "cancel_requested", str(state))
        self.endpoints(self.safety()["engine"])
        self.passed("explicit stabilize cancellation uses predeclared carrier recovery with full endpoints")

        self.prepare_handover()
        path, _, _, launch_remaining = self.plan("manual-takeover")
        accepted = self.run("batch", "run", "--file", path, "--no-wait")
        self.jobs.append(accepted["job_id"])
        self.deck(1, "fader", "set", .37)
        time.sleep(launch_remaining + .2)
        state = self.run("jobs", "get", accepted["job_id"])
        engine = self.safety()["engine"]
        self.require(state["state"] == "yielded" and state["reason"] == "manual_takeover", str(state))
        self.require(abs(engine["decks"][1]["channel_fader"] - .37) < .002
                     and not engine["decks"][1]["playing"] and not engine["decks"][1]["launch_pending"]
                     and not any(engine["active_ramps"]), "Yield restored knobs or fired a cancelled launch")
        self.passed("manual takeover yields without restoration or later pending launch")

    def execute(self):
        initial = self.safety()
        self.require(not any(d["playing"] for d in initial["engine"]["decks"])
                     and not any(j["state"] in ["armed", "running", "recovering"] for j in initial["jobs"]),
                     "Use a fresh idle disposable profile")
        self.run("commands", "list", "--prefix", "batch.", "--names-only")
        for name in ["batch.run", "cues.set", "library.suggest_next", "track.inspect", "jobs.cancel"]:
            self.run("commands", "describe", name)
        self.passed("current shared schema discovery and PID/socket/isolated/muted guards")
        self.metadata_trial()
        self.scheduling_trial()
        final = self.safety()
        for path, digest in self.hashes.items():
            self.require(hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest, "Generated source file changed")
        # Leave the disposable renderer idle and muted, never stop another app.
        for i in range(2):
            self.deck(i, "pause")
        final = self.safety()
        self.passed("original generated WAVs unchanged; all playback remains isolated and master/cue muted")
        return {"passed": len(self.checks), "checks": self.checks, "plans": self.plans,
                "jobs": self.jobs, "trace": self.trace, "final_observed_status": final,
                "elapsed_seconds": time.monotonic() - self.started,
                "audible_verification": False, "compaction_test": "No IPC interval + persisted resume; no actual model compaction/restart",
                "timing_basis": "Explicit manual 120 BPM grid with 0.25 second origin on generated fixtures; not neural phrase evidence"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    opts = parser.parse_args()
    trial = Trial(opts.manifest)
    try:
        report = trial.execute()
        path = trial.out / "scheduled-workflow.json"
        path.write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({"passed": report["passed"], "report": str(path)}))
    except BaseException as error:
        # Preserve evidence without automatic corrective controls/replay.
        path = trial.out / "scheduled-workflow-failed.json"
        path.write_text(json.dumps({"error": str(error), "checks": trial.checks,
                                    "jobs": trial.jobs, "plans": trial.plans, "trace": trial.trace}, indent=2) + "\n")
        raise


if __name__ == "__main__":
    main()
