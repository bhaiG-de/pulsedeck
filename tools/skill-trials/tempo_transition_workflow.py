#!/usr/bin/env python3
"""Tempo and transition regression on a muted, generated-audio profile only."""
import argparse
import json
from pathlib import Path
import time

from scheduled_workflow import Trial
from persistent_session_workflow import Session


class TempoTrial(Trial):
    def transport(self):
        return self.call("engine.transport")

    def workflow(self):
        self.safety()
        for index in range(2):
            self.load_uniform(index, self.manifest["tracks"][index])
        before = self.transport()["decks"][1]
        self.deck(1, "tempo", "set", "--bpm", 126)
        time.sleep(.05)
        after = self.transport()["decks"][1]
        self.require(abs(after["playback_bpm"]-126) < .001, str(after))
        self.require(abs(after["target_playback_bpm"]-126) < .001, "Requested BPM target was not retained")
        self.require(abs(after["tempo_percent"]-5) < .001, "Absolute BPM did not resolve source-relative percent")
        for field in ["bpm", "grid_revision", "beatgrid_offset_seconds", "beatgrid_uniform"]:
            self.require(before[field] == after[field], "Playback BPM changed source grid: "+field)
        self.passed("Absolute playback BPM resolves precisely and preserves the source/grid")

        self.deck(0, "master")
        self.deck(0, "tempo", "set", "--bpm", 124)
        self.deck(0, "play")
        self.deck(1, "sync", "on")
        self.deck(1, "play")
        deadline = time.monotonic()+2
        while not self.transport()["decks"][1]["playing"]:
            self.require(time.monotonic()<deadline, "Quantized follower did not start")
            time.sleep(.05)
        samples = []
        for _ in range(4):
            time.sleep(.1)
            samples.append(self.transport()["decks"][1]["playback_bpm"])
        self.require(all(abs(bpm-124)<.001 for bpm in samples), str(samples))
        self.deck(0, "pause")
        frozen = self.transport()["decks"][1]["actual_playback_rate"]
        self.deck(0, "load", "--track", self.manifest["tracks"][2]["source"])
        time.sleep(.1)
        follower = self.transport()["decks"][1]
        self.require(follower["playing"] and follower["is_master"]
                     and abs(follower["actual_playback_rate"]-frozen)<.0001,
                     "Reloading a paused master retuned the live follower")
        self.call("engine.command", {"type":"set_sync","deck":"B","enabled":True}, reject="any")
        self.passed("Sync tempo stays fixed; master reload promotes the live deck without retuning; self-master Sync rejects")
        self.deck(1, "pause")

        session = Session(self)
        self.session = session
        try:
            # Resolve BPM only after the new source is loaded, through the persistent queue.
            session.send(
                {"id":"tempo-load", "command":"deck.load", "args":{"deck":"B","track":self.manifest["tracks"][1]["source"]}},
                {"id":"absolute-tempo", "command":"engine.command", "args":{"type":"set_playback_bpm","deck":"B","bpm":122},"when":{"after":"tempo-load"}},
                {"id":"start", "command":"engine.command", "args":{"type":"play","deck":"B"},"when":{"after":"absolute-tempo"}},
            )
            for identity in ["tempo-load", "absolute-tempo", "start"]:
                receipt = session.executed(identity)["data"]
                self.require(receipt["state"] == "executed", str(receipt))
            self.require(abs(self.transport()["decks"][1]["playback_bpm"]-122)<.001,
                         "Queued BPM did not resolve against loaded source")
            # The predecessor has already completed before this valid zero-offset append.
            time.sleep(.08)
            session.send({"id":"zero-delay-blend", "commands":[
                {"command":"automation.ramp", "args":{"target":{"type":"channel_fader","deck":"B"},"value":.7,"seconds":.2}},
                {"command":"automation.ramp", "args":{"target":{"type":"eq_high","deck":"B"},"value":.8,"seconds":.1}},
            ], "when":{"after":"start","type":"after_beats","clock":"B","beats":0}})
            result = session.executed("zero-delay-blend")["data"]
            self.require(result["state"] == "executed", str(result))
            self.passed("Persistent load→absolute BPM→Play works; delayed zero-beat group executes instead of expiring")
        finally:
            session.close()

        self.load_uniform(0, self.manifest["tracks"][0])
        self.deck(0, "seek", "--at", "95.5s")
        self.deck(0, "play")
        self.call("automation.ramp", {"target":{"type":"channel_fader","deck":"B"},
                                     "value":1,"beats":2,"clock":"A"})
        time.sleep(1.3)
        state = self.transport()
        self.require(not state["decks"][0]["playing"] and state["decks"][1]["channel_fader"]>.999,
                     "A ramp stopped partially applied when its beat clock reached natural EOF")
        self.passed("A started beat-duration ramp finishes at its endpoint through natural clock EOF")
        self.safety()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    trial = TempoTrial(parser.parse_args().manifest)
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
        (trial.out/"tempo-transition.json").write_text(json.dumps({
            "checks":trial.checks,"error":error,"trace":trial.trace,
            "session_messages":getattr(getattr(trial,"session",None),"messages",[]),
            "elapsed_seconds":time.monotonic()-trial.started}, indent=2)+"\n")


if __name__ == "__main__":
    main()
