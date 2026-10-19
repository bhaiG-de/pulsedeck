#!/usr/bin/env python3
"""Exercise append/invalidate/seal and completion wakeups in a muted trial.

Run only against a profile.py --audio manifest. Sources are its generated WAVs;
this is a queue integration fixture, not a DJ policy or listening test.
"""
import argparse
import copy
import json
from pathlib import Path
import time

from scheduled_workflow import Trial


class QueueTrial(Trial):
    def state(self):
        return self.run("jobs", "get", self.job)

    def events(self, cursor, wait=0):
        return self.call("events.get", {"since": cursor, "job_id": self.job, "wait_ms": wait})

    def wait_for(self, cursor, predicate, timeout=25):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            remaining = max(1, int((deadline - time.monotonic()) * 1000))
            event = self.events(cursor, min(remaining, 12000))
            self.require(not event["resync_required"], "Unexpected event stream gap")
            cursor = event["sequence"]
            if predicate(event["events"]):
                return cursor, event["events"]
        raise AssertionError("No completion notification before fixture deadline")

    def workflow(self):
        self.safety()
        for index in range(2):
            self.load_uniform(index, self.manifest["tracks"][index])
            self.call("engine.command", {"type": "set_eq", "deck": index,
                                         "low": 1, "mid": 1, "high": 1})
            self.call("engine.command", {"type": "set_filter", "deck": index, "amount": 0})
            self.call("engine.command", {"type": "set_channel_fader", "deck": index,
                                         "value": 1 - index})
            self.deck(index, "seek", "--at", "0.25s")
        self.call("engine.command", {"type": "set_master", "deck": 0})
        self.deck(0, "play")
        recovery = [
            {"type": "set_channel_fader", "deck": 0, "value": 1},
            {"type": "set_channel_fader", "deck": 1, "value": 0},
            {"type": "set_eq", "deck": 0, "low": 1, "mid": 1, "high": 1},
            {"type": "set_eq", "deck": 1, "low": 1, "mid": 1, "high": 1},
            {"type": "set_filter", "deck": 1, "amount": 0},
        ]
        # No caller-copied engine/source/grid/position/control revisions.
        start = {"schedule": {"operation_id": "incremental-" + self.token,
                              "clock": "A", "open": True, "expiry_seconds": 35,
                              "completion_event": 64, "completion_carrier": "B",
                              "pause_outgoing": True,
                              "recovery": {"carrier": "A", "seconds": .2, "controls": recovery}},
                 "steps": [{"action_id": "launch-b", "command": "engine.command",
                            "args": {"type": "play", "deck": "B"}, "at_event": 12}]}
        accepted = self.call("batch.run", start)
        self.job = accepted["job_id"]
        self.jobs.append(self.job)
        before = self.state()
        self.require(before["revision"] == 1 and before["open"], "Queue did not open at revision 1")
        self.require("expected_load_revisions" not in before.get("schedule", {}) and "steps" not in before,
                     "Default queue status returned a full plan")
        cursor = self.call("events.get", {"since": 0})["sequence"]
        waiting = time.monotonic()
        cursor, events = self.wait_for(cursor, lambda es: any(
            e["type"] == "job.action" and e["data"].get("action_id") == "launch-b"
            and e["data"].get("state") == "verified" for e in es))
        self.require(time.monotonic() - waiting >= 1,
                     "Fixture did not actually wait for future execution")
        self.require(self.call("engine.transport")["decks"][1]["playing"],
                     "Launch receipt arrived without actual playback")
        self.passed("open queue launches through a blocking completion wait with no transport polling")

        fade = {"id": self.job, "patch_id": "faders-" + self.token, "expected_revision": 1,
                "steps": [
                    {"action_id": "fade-a", "at_event": 28, "command": "automation.ramp",
                     "args": {"target": {"type": "channel_fader", "deck": "A"},
                              "value": 0, "seconds": 1}},
                    {"action_id": "fade-b", "at_event": 28, "command": "automation.ramp",
                     "args": {"target": {"type": "channel_fader", "deck": "B"},
                              "value": 1, "seconds": 1}},
                    {"action_id": "old-filter", "at_event": 32, "command": "engine.command",
                     "args": {"type": "set_filter", "deck": "B", "amount": .2}},
                ]}
        patched = self.call("batch.append", fade)
        self.require(patched["revision"] == 2, "Append did not advance queue revision")
        duplicate = self.call("batch.append", fade)
        self.require(duplicate.get("replayed") is False, "Retry replayed native work")
        conflict = copy.deepcopy(fade)
        conflict["steps"][0]["args"]["value"] = .1
        self.call("batch.append", conflict, reject="idempotency_conflict")
        stale = copy.deepcopy(fade)
        stale["patch_id"] = "stale-" + self.token
        stale["steps"] = [{"action_id": "stale-filter", "at_event": 34,
                           "command": "engine.command",
                           "args": {"type": "set_filter", "deck": "B", "amount": 0}}]
        self.call("batch.append", stale, reject="any")
        self.require(self.state()["revision"] == 2, "Rejected patch changed revision")
        replacement = {"id": self.job, "patch_id": "replace-" + self.token,
                       "expected_revision": 2, "invalidate_action_ids": ["old-filter"],
                       "steps": [{"action_id": "new-filter", "at_event": 34,
                                  "command": "engine.command",
                                  "args": {"type": "set_filter", "deck": "B", "amount": 0}}]}
        self.require(self.call("batch.append", replacement)["revision"] == 3,
                     "Atomic replacement was not accepted")
        bad_cancel = {"id": self.job, "patch_id": "cancel-applied-" + self.token,
                      "expected_revision": 3, "action_ids": ["launch-b"]}
        self.call("batch.invalidate", bad_cancel, reject="any")
        self.require(self.state()["revision"] == 3, "Applied-action invalidation changed queue")
        sealed = self.call("batch.seal", {"id": self.job, "patch_id": "seal-" + self.token,
                                           "expected_revision": 3, "completion_event": 40})
        self.require(sealed["revision"] == 4, "Seal revision missing")
        self.passed("append after launch, idempotent retry, stale rejection and atomic pending-action replacement")

        cursor = self.call("events.get", {"since": 0})["sequence"]
        cursor, _ = self.wait_for(cursor, lambda es: any(
            e["type"] == "job.updated" and e["data"].get("state") == "completed" for e in es))
        final = self.state()
        actions = {a["action_id"]: a for a in final["actions"]}
        self.require(final["state"] == "completed" and final["revision"] == 4,
                     "Queue did not finish its sealed endpoint")
        self.require(actions["launch-b"]["state"] == "verified"
                     and actions["old-filter"]["state"] == "cancelled"
                     and actions["new-filter"]["state"] == "verified", "Action receipts are inconsistent")
        transport = self.call("engine.transport")
        self.require(not transport["decks"][0]["playing"] and transport["decks"][1]["playing"]
                     and transport["decks"][0]["channel_fader"] < .002
                     and transport["decks"][1]["channel_fader"] > .998,
                     "Handover final transport/endpoints are wrong")
        self.passed("native faders and sealed completion notify without model-timed commands; applied history preserved")
        self.safety()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    trial = QueueTrial(parser.parse_args().manifest)
    error = None
    try:
        trial.workflow()
    except BaseException as exc:
        error = repr(exc)
        raise
    finally:
        if trial.authorized:
            trial.run("automation", "stop")
            for index in range(2):
                trial.deck(index, "pause")
        evidence = {"passed": len(trial.checks), "checks": trial.checks, "error": error,
                    "isolated_pid": trial.manifest["pid"], "trace": trial.trace,
                    "elapsed_seconds": time.monotonic() - trial.started}
        (trial.out / "incremental-queue.json").write_text(json.dumps(evidence, indent=2) + "\n")
        print(json.dumps({"passed": len(trial.checks), "error": error}))


if __name__ == "__main__":
    main()
