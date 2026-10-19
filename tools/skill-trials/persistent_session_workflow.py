#!/usr/bin/env python3
"""Real duplex CLI regression, restricted to muted generated-audio profiles."""
import argparse
import json
from pathlib import Path
import queue
import subprocess
import threading
import time

from scheduled_workflow import Trial


class Session:
    def __init__(self, trial):
        self.trial = trial
        self.messages = []
        self.backlog = []
        self.pending = queue.Queue()
        self.process = subprocess.Popen(
            [str(trial.cli), "session", "start", "--stdio", "--socket", str(trial.socket)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, bufsize=1)
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()
        self.ready = self.wait(lambda m: m.get("type") == "session_ready")
        self.job = self.ready["job_id"]
        trial.jobs.append(self.job)

    def read(self):
        for line in self.process.stdout:
            try:
                message = json.loads(line)
            except ValueError:
                message = {"invalid_stdout": line}
            self.messages.append(message)
            self.pending.put(message)

    def send(self, *messages):
        for message in messages:
            self.process.stdin.write((message if isinstance(message, str) else json.dumps(message)) + "\n")
        self.process.stdin.flush()

    def wait(self, predicate, timeout=25):
        for index, message in enumerate(self.backlog):
            if predicate(message):
                return self.backlog.pop(index)
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                message = self.pending.get(timeout=max(.001, deadline-time.monotonic()))
            except queue.Empty:
                break
            self.trial.require("invalid_stdout" not in message, str(message))
            if predicate(message):
                return message
            self.backlog.append(message)
        raise AssertionError("No expected session response; tail=" + json.dumps(self.messages[-8:]))

    def ack(self, item, kind="registered", ok=True):
        result = self.wait(lambda m: m.get("type") == kind and
                           (m.get("item_id") == item or m.get("action_id") == item))
        self.trial.require(result.get("ok") is ok, str(result))
        return result

    def executed(self, item):
        return self.wait(lambda m: m.get("type") == "job.action" and
                         m.get("data", {}).get("item_id", m.get("data", {}).get("action_id")) == item and
                         m["data"].get("state") in ("executed", "failed", "cancelled"))

    def close(self):
        if self.process.stdin and not self.process.stdin.closed:
            self.process.stdin.close()
        self.process.wait(timeout=8)
        self.reader.join(timeout=2)
        self.trial.require(self.process.returncode == 0, self.process.stderr.read())


class PersistentTrial(Trial):
    def workflow(self):
        self.safety()
        for index in range(2):
            self.load_uniform(index, self.manifest["tracks"][index])
        session = Session(self)
        self.session = session
        try:
            # Every line is submitted immediately: no model-side timing/polling.
            commands = [
                {"id":"load-b", "command":"deck.load", "args":{"deck":"B", "track":self.manifest["tracks"][2]["source"]}},
                {"id":"tempo-b", "command":"engine.command", "args":{"type":"set_tempo", "deck":"B", "percent":-1}, "when":{"after":"load-b"}},
                {"id":"seek-b", "command":"engine.command", "args":{"type":"seek", "deck":"B", "seconds":.25}, "when":{"after":"tempo-b"}},
                {"id":"play-b", "command":"engine.command", "args":{"type":"play", "deck":"B"}, "when":{"after":"seek-b"}},
                {"id":"fade-b", "command":"automation.ramp", "args":{"target":{"type":"channel_fader","deck":"B"},"value":.3,"seconds":.25}, "when":{"after":"play-b","type":"after_seconds","seconds":.3}},
                {"id":"pause-b", "command":"engine.command", "args":{"type":"pause","deck":"B"},"when":{"after":"fade-b"}},
            ]
            session.send(*commands)
            deadline = time.monotonic() + 35
            registered, executed = set(), set()
            while len(registered) < len(commands) or len(executed) < len(commands):
                message = session.wait(lambda m: True, max(.1, deadline-time.monotonic()))
                if message.get("type") == "registered":
                    self.require(message["ok"], str(message))
                    registered.add(message["item_id"])
                if message.get("type") == "job.action":
                    data = message["data"]
                    self.require(data["state"] not in ("failed", "cancelled"), str(message))
                    if data["state"] == "executed":
                        executed.add(data.get("item_id", data.get("action_id")))
            transport = self.call("engine.transport")
            self.require(not transport["decks"][1]["playing"] and abs(transport["decks"][1]["channel_fader"]-.3)<.002,
                         "Executed pause/ramp receipts disagree with real deck")
            self.passed("Back-to-back load, tempo, seek, launch, timed ramp and pause execute with spontaneous receipts")

            future = {"id":"future", "command":"engine.command", "args":{"type":"set_filter","deck":"B","amount":.7},
                      "when":{"type":"after_seconds","seconds":20}}
            session.send(future)
            session.ack("future")
            update = {"op":"update", "id":"future", "request_id":"edit-"+self.token,
                      "command":"engine.command", "args":{"type":"set_filter","deck":"B","amount":.1},
                      "when":{"type":"after_seconds","seconds":20}}
            session.send(update)
            session.ack("future", "updated")
            session.send(update)
            duplicate = session.ack("future", "updated")
            self.require(duplicate.get("duplicate") and duplicate.get("replayed") is False,
                         "Edit retry replayed a mutation")
            session.send({"op":"remove", "id":"future"})
            session.ack("future", "removed")
            session.send("bad-json")
            self.require(session.wait(lambda m: m.get("ok") is False)["error"]["code"] == "invalid_request",
                         "Malformed JSON not rejected")
            session.send({"id":"recover", "command":"engine.command", "args":{"type":"set_filter","deck":"B","amount":0}})
            session.ack("recover")
            self.require(session.executed("recover")["data"]["state"] == "executed", "Valid input failed after invalid input")
            self.passed("Pending update/remove, stable edit retry and malformed-input recovery")

            session.send(
                {"id":"play-clock","command":"engine.command","args":{"type":"play","deck":"A"}},
                {"id":"beat-marker","command":"engine.command","args":{"type":"set_filter","deck":"B","amount":.02},
                 "when":{"after":"play-clock","type":"after_beats","clock":"A","beats":2}},
                {"id":"stop-clock","command":"engine.command","args":{"type":"pause","deck":"A"},"when":{"after":"beat-marker"}})
            start = session.executed("play-clock")
            marker = session.executed("beat-marker")
            self.require(session.executed("stop-clock")["data"]["state"] == "executed", "Clock stop failed")
            frames = marker["data"]["receipt"]["completed_frame"] - start["data"]["receipt"]["completed_frame"]
            sample_rate = self.call("engine.transport")["sample_rate"]
            self.require(abs(frames/sample_rate-1)<.04, "Two beats did not use the 120 BPM renderer clock")
            self.passed("Relative beat timing resolves from actual dependency completion on the renderer")

            # More than one native segment's lifetime slots, without ending session.
            for index in range(72):
                identity = "rotate-" + str(index)
                session.send({"id":identity,"command":"engine.command", "args":{"type":"set_filter","deck":"B","amount":(index%2)*.01}})
                session.ack(identity)
                self.require(session.executed(identity)["data"]["state"] == "executed", "Segment rotation failed")
            self.passed("One persistent session survives more than 64 native actions without capacity exhaustion")
            session.send({"id":"eof-pending", "command":"engine.command", "args":{"type":"play","deck":"A"},
                          "when":{"type":"after_seconds","seconds":30}})
            session.ack("eof-pending")
        finally:
            session.close()
        state = self.run("jobs", "get", session.job)
        actions = {a["action_id"]:a for a in state["actions"]}
        self.require(actions["eof-pending"]["state"] == "cancelled", "EOF left pending work alive")
        self.require(not self.call("engine.transport")["decks"][0]["playing"], "EOF pending launch fired")
        self.passed("EOF cancels owned pending work and exits without executing the future launch")
        self.safety()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    trial = PersistentTrial(parser.parse_args().manifest)
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
        (trial.out / "persistent-session.json").write_text(json.dumps({
            "checks":trial.checks,"error":error,"trace":trial.trace,
            "session_messages":getattr(getattr(trial,"session",None),"messages",[]),
            "elapsed_seconds":time.monotonic()-trial.started}, indent=2)+"\n")


if __name__ == "__main__":
    main()
