#!/usr/bin/env python3
"""Luna recovery/feedback regressions on a muted generated-audio profile only."""
import argparse
import json
from pathlib import Path
import time

from scheduled_workflow import Trial
from persistent_session_workflow import Session


def command(identity, kind, **args):
    return {"id": identity, "command": "engine.command", "args": {"type": kind, **args}}


class LunaGapsTrial(Trial):
    def transport(self):
        return self.call("engine.transport")

    def atomic(self, session, name, items, accepted=True):
        name = name + "-" + self.token
        session.send({"op": "add", "request_id": name, "items": items})
        ack = session.wait(lambda m: m.get("request_id") == name and
                           m.get("type") in ("registered", "rejected"))
        self.require(ack.get("ok") is accepted and ack.get("atomic") is True, str(ack))
        self.require(ack["registered_count"] == (len(items) if accepted else 0), str(ack))
        return ack

    def workflow(self):
        self.safety()
        for index in range(2):
            self.load_uniform(index, self.manifest["tracks"][index])
        session = Session(self)
        self.sessions = [session]
        try:
            before = self.transport()
            bad_uri = "spotify:track:5u4hhtZ7f4rWkMZEZcTKr"
            invalid_uri = self.atomic(session, "bad-uri", [
                command("would-isolate", "set_channel_fader", deck="B", value=.17),
                {"id": "bad-load", "command": "deck.load", "args": {"deck": "B", "uri": bad_uri},
                 "when": {"after": "would-isolate"}},
            ], accepted=False)
            self.require(invalid_uri["error"]["code"] == "invalid_track_uri" and
                         "22" in invalid_uri["error"]["message"], str(invalid_uri))
            after = self.transport()
            self.require(before["load_revisions"] == after["load_revisions"] and
                         before["decks"][1]["channel_fader"] == after["decks"][1]["channel_fader"],
                         "Rejected URI plan changed source or fader")
            invalid = command("beyond-range", "set_filter", deck="B", amount=.8)
            invalid["when"] = {"type": "after_beats", "clock": "A", "beats": 259}
            self.atomic(session, "bad-range", [command("prefix", "set_filter", deck="B", amount=.7), invalid], False)
            session.send({"op": "status"})
            status = session.wait(lambda m: m.get("type") == "status")
            self.require(not status["result"]["actions"], str(status))
            self.passed("Atomic URI/range rejection registers no prefix and preserves deck state")

            pause = command("pause-b", "pause", deck="B")
            session.send(pause)
            session.ack("pause-b")
            self.require(session.executed("pause-b")["data"]["state"] == "executed", "Pause failed")
            session.send(command("pause-b", "play", deck="B"))
            session.ack("pause-b", "rejected", False)
            follow = command("unsafe-follow", "set_filter", deck="B", amount=.1)
            follow["when"] = {"after": "pause-b"}
            session.send(follow)
            self.require(session.ack("unsafe-follow", "rejected", False)["error"]["code"] ==
                         "dependency_ambiguous", "Rejected reuse silently bound old completed action")
            follow["id"] = "explicit-follow"
            follow["when"]["after_version"] = 1
            session.send(follow)
            session.ack("explicit-follow")
            self.require(session.executed("explicit-follow")["data"]["state"] == "executed", "Explicit accepted version failed")
            session.send({"op": "status"})
            status = session.wait(lambda m: m.get("type") == "status")
            self.require(status["dependency_ambiguities"] == [{"item_id": "pause-b", "accepted_versions": [1]}], str(status))
            self.passed("Rejected ID reuse blocks ambiguous dependencies; explicit acknowledged version works")
        finally:
            session.close()

        # Submit the whole musical plan before starting the clock. No agent-side firing loop.
        self.deck(0, "seek", "--at", "0.25s")
        self.deck(0, "master")
        session = Session(self)
        self.sessions.append(session)
        try:
            isolate = command("isolate-b", "set_channel_fader", deck="B", value=0)
            start = command("start-clock", "play", deck="A")
            start["when"] = {"after": "isolate-b"}
            launch = command("launch-b", "play", deck="B")
            launch["when"] = {"after": "start-clock", "type": "at_seconds", "clock": "A", "seconds": 2.25}
            blend = {"id": "blend", "commands": [
                {"command": "automation.ramp", "args": {"target": {"type": "channel_fader", "deck": "A"}, "value": 0, "seconds": .3}},
                {"command": "automation.ramp", "args": {"target": {"type": "channel_fader", "deck": "B"}, "value": 1, "seconds": .3}},
                {"command": "automation.ramp", "args": {"target": {"type": "eq_low", "deck": "B"}, "value": 1, "seconds": .2}},
            ], "when": {"after": "launch-b", "type": "at_seconds", "clock": "A", "seconds": 3.25}}
            self.atomic(session, "musical-plan", [isolate, start, launch, blend])
            self.require(session.executed("isolate-b")["data"]["state"] == "executed", "Isolation failed")
            resolved = session.wait(lambda m: m.get("type") == "job.action" and
                                    m.get("data", {}).get("item_id") == "launch-b" and
                                    (m["data"].get("receipt") or {}).get("resolved_clock_seconds") == 2.25)
            self.require(abs(resolved["data"]["receipt"]["clock_source_headroom_seconds"]-93.75)<.01, str(resolved))
            launched = resolved["data"] if resolved["data"]["state"] == "executed" else session.executed("launch-b")["data"]
            mixed = session.executed("blend")["data"]
            self.require(launched["state"] == mixed["state"] == "executed", str([launched, mixed]))
            self.require(launched["receipt"]["resolved_clock_seconds"] == 2.25, str(launched))
            self.require(all(m["receipt"]["ramp_end_frame"] is not None for m in mixed["members"]), str(mixed))
            time.sleep(.03)
            state = self.transport()
            started = session.executed("start-clock")["data"]["receipt"]["frame"]
            rate = state["sample_rate"]
            self.require(abs(launched["receipt"]["frame"]-started-2*rate) <= 1,
                         "Absolute launch missed the selected source boundary")
            self.require(all(abs(m["receipt"]["frame"]-started-3*rate) <= 1 for m in mixed["members"]),
                         "Parallel blend did not start at the selected source boundary")
            self.require(state["decks"][1]["playing"] and state["decks"][1]["channel_fader"]>.999 and
                         state["decks"][0]["channel_fader"]<.001, str(state))
            self.passed("Absolute launch/parallel blend stream resolved timing, headroom and ramp endpoints, then execute")
        finally:
            session.close()
        for index in range(2):
            self.deck(index, "pause")
        self.deck(0, "master")
        session = Session(self)
        self.sessions.append(session)
        try:
            session.send(command("self-master-sync", "set_sync", deck="A", enabled=True))
            session.ack("self-master-sync")
            failed = session.executed("self-master-sync")["data"]
            self.require(failed["state"] == "failed" and
                         failed["result"]["error"]["failure"]["cause"] == "sync_master_self", str(failed))
            self.require(failed["result"]["error"]["failure"]["deck"] == 0, str(failed))
            self.passed("Native prerequisite failure names its cause and deck in spontaneous completion feedback")
        finally:
            session.close()
        self.safety()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    trial = LunaGapsTrial(parser.parse_args().manifest)
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
        (trial.out/"luna-gaps.json").write_text(json.dumps({
            "checks": trial.checks, "error": error, "trace": trial.trace,
            "session_messages": [s.messages for s in getattr(trial, "sessions", [])],
            "elapsed_seconds": time.monotonic()-trial.started}, indent=2)+"\n")


if __name__ == "__main__":
    main()
