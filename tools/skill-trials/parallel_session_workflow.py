#!/usr/bin/env python3
"""Exercise real grouped session commands in a muted generated-audio profile."""
import argparse
import json
from pathlib import Path
import time

from persistent_session_workflow import Session
from scheduled_workflow import Trial


def ramp(target, deck, value, seconds):
    return {"command": "automation.ramp", "args": {
        "target": {"type": target, "deck": deck}, "value": value, "seconds": seconds}}


class ParallelTrial(Trial):
    def workflow(self):
        self.safety()
        for index in range(2):
            self.load_uniform(index, self.manifest["tracks"][index])
            self.call("engine.command", {"type":"set_eq", "deck":"AB"[index],
                                        "low":1,"mid":1,"high":1})
        self.call("engine.command", {"type":"set_channel_fader", "deck":"B", "value":0})
        self.call("engine.command", {"type":"set_filter", "deck":"B", "amount":0})
        time.sleep(.2)
        self.deck(0, "play")
        session = self.session = Session(self)
        try:
            group = {"id": "parallel", "commands": [
                ramp("eq_low", "A", .25, .5),
                ramp("tempo_percent", "A", 2, 1),
                ramp("channel_fader", "B", .75, 1.5),
                ramp("eq_high", "B", 1.4, .75)],
                "when": {"type": "after_seconds", "seconds": 1}}
            session.send(group, {"id": "after-parallel", "command": "engine.command",
                "args": {"type": "set_filter", "deck": "B", "amount": .1},
                "when": {"after": "parallel", "type": "after_seconds", "seconds": .25}})
            session.ack("parallel")
            done = session.executed("parallel")["data"]
            self.require(done["state"] == "executed", str(done))
            receipts = [member["receipt"] for member in done["members"]]
            starts = {receipt["frame"] for receipt in receipts}
            self.require(len(starts) == 1, "Group members did not start on exactly the same frame")
            state = self.call("engine.transport")
            rate = state["sample_rate"]
            for receipt, duration in zip(receipts, [.5, 1, 1.5, .75]):
                # Existing de-zipper smoothing can settle just after the ramp's
                # nominal duration. Completion proves the actual audible endpoint.
                elapsed = (receipt["completed_frame"]-receipt["frame"])/rate
                self.require(duration-.002 <= elapsed < duration+.025,
                             "Member duration changed to group duration: " + str(receipt))
            follower = session.executed("after-parallel")["data"]
            self.require(follower["state"] == "executed", str(follower))
            offset = (follower["receipt"]["frame"]-max(r["completed_frame"] for r in receipts))/rate
            self.require(abs(offset-.25)<.002, "Follower did not wait for the longest member")
            state = self.call("engine.transport")
            for deck, field, value in [(0,"eq_low",.25),(0,"tempo_percent",2),
                                       (1,"channel_fader",.75),(1,"eq_high",1.4)]:
                self.require(abs(state["decks"][deck][field]-value)<.005, "Wrong endpoint " + field)
            self.require(state["decks"][0]["bpm"] == 120, "Tempo ramp changed source BPM")
            self.passed("Four controls start on one exact renderer frame, finish independently, and gate the follower on the last completion")

            future = {"id":"future", "commands":[ramp("eq_low","A",.6,.2),
                        ramp("tempo_percent","A",1,.3)],
                      "when":{"type":"after_seconds","seconds":10}}
            session.send(future, {"id":"future-dependent","command":"engine.command",
                "args":{"type":"set_filter","deck":"B","amount":.2},
                "when":{"after":"future","type":"after_seconds","seconds":.1}})
            session.ack("future")
            session.ack("future-dependent")
            session.send({**future, "op":"update", "request_id":"cyclic-edit-"+self.token,
                          "when":{"after":"future-dependent"}})
            session.wait(lambda m: m.get("ok") is False)
            update = {**future, "op":"update", "request_id":"group-edit-"+self.token,
                      "commands":[ramp("eq_low","A",.7,.2),ramp("tempo_percent","A",0,.4)],
                      "when":{"type":"after_seconds","seconds":.6}}
            session.send(update)
            revised_ack = session.ack("future","updated")
            session.send(update)
            duplicate = session.ack("future","updated")
            self.require(duplicate.get("duplicate") and duplicate.get("replayed") is False,
                         "Group update retry replayed mutation")
            revised = session.wait(lambda m: m.get("type")=="job.action" and
                m.get("data",{}).get("item_id")=="future" and
                m["data"].get("version")==revised_ack["version"] and
                m["data"].get("state") in ("executed","failed","cancelled"))["data"]
            dependent = session.executed("future-dependent")["data"]
            self.require(revised["state"] == dependent["state"] == "executed",
                         "A queued dependent did not follow the replacement group")
            end = max(m["receipt"]["completed_frame"] for m in revised["members"])
            self.require(abs((dependent["receipt"]["frame"]-end)/rate-.1)<.002,
                         "A queued dependent retained the old group's completion anchor")
            session.send({**future,"id":"remove-future"})
            session.ack("remove-future")
            session.send({"op":"remove","id":"remove-future"})
            session.ack("remove-future","removed")
            session.send({"id":"conflict","commands":[ramp("eq_low","A",.8,.2),ramp("eq_low","A",.9,.2)]})
            failure = session.wait(lambda m: m.get("ok") is False)
            self.require("error" in failure, "Conflicting targets accepted")
            session.send({"id":"unsupported","commands":[ramp("eq_low","A",.8,.2),
                {"command":"deck.load","args":{"deck":"B","track":self.manifest["tracks"][2]["source"]}}]})
            session.wait(lambda m: m.get("ok") is False)
            session.send({"id":"recover","command":"engine.command", "args":{"type":"set_filter","deck":"B","amount":0}})
            self.require(session.executed("recover")["data"]["state"] == "executed", "Stream unusable after rejected group")
            state = self.call("engine.transport")
            self.require(abs(state["decks"][0]["eq_low"]-.7)<.005, "Rejected/removed group applied a member")
            self.passed("Whole-group update rebinds queued dependents, retry/removal work, and invalid groups reject atomically")

            # Four-member groups force rotation past the native 64-member capacity.
            for index in range(18):
                identity = "rotate-"+str(index)
                variation = index % 2
                session.send({"id":identity, "commands":[ramp("eq_low","A",.4+.2*variation,.01),
                    ramp("tempo_percent","A",.5*variation,.02),
                    ramp("channel_fader","B",.3+.1*variation,.03),
                    ramp("eq_high","B",1-.2*variation,.01)]})
                executed = session.executed(identity)["data"]
                self.require(executed["state"] == "executed", str(executed))
                self.require(len({m["receipt"]["frame"] for m in executed["members"]}) == 1,
                             "Capacity rotation split a group")
            self.passed("A persistent session rotates beyond 64 native members without splitting groups")

            # An unrelated muted phones ramp must survive cancellation of this session.
            self.call("automation.ramp", {"target":{"type":"cue_gain"},"value":0,"seconds":10})
            session.send({"id":"cancel-running","commands":[ramp("eq_low","A",1,5),
                ramp("tempo_percent","A",3,5),ramp("channel_fader","B",.8,5),ramp("eq_high","B",1.5,5)]})
            session.wait(lambda m: m.get("type")=="job.action" and
                m.get("data",{}).get("action_id")=="cancel-running" and
                m["data"].get("state")=="running")
        finally:
            session.close()
        time.sleep(.2)
        first = self.call("engine.transport")
        time.sleep(.3)
        second = self.call("engine.transport")
        for deck, field in [(0,"eq_low"),(0,"tempo_percent"),(1,"channel_fader"),(1,"eq_high")]:
            self.require(abs(first["decks"][deck][field]-second["decks"][deck][field])<.005,
                         "Owned group ramp survived session EOF: "+field)
        self.require(sum(second["active_ramps"]) == 1, "Cancellation stopped an unrelated ramp or left owned ramps")
        self.passed("Session EOF freezes every owned group ramp while preserving an unrelated muted ramp")
        self.safety()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    trial = ParallelTrial(parser.parse_args().manifest)
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
                trial.deck(index,"pause")
        (trial.out/"parallel-session.json").write_text(json.dumps({
            "checks":trial.checks,"error":error,"trace":trial.trace,
            "session_messages":getattr(getattr(trial,"session",None),"messages",[]),
            "elapsed_seconds":time.monotonic()-trial.started},indent=2)+"\n")


if __name__ == "__main__":
    main()
