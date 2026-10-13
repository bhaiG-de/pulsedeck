#!/usr/bin/env python3
"""Disposable real desktop profiles for shell-driven DJ skill trials."""
import argparse
import json
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import time
import wave

ROOT = Path(__file__).resolve().parents[2]
CLI = ROOT / "target/debug/pulsedeckctl"
APP = ROOT / "target/debug/bundle/macos/PulseDeck.app"

def request(manifest, *words):
    proc = subprocess.run([str(CLI), *map(str, words), "--socket", manifest["socket"], "--json"],
                          capture_output=True, text=True, timeout=185)
    response = json.loads(proc.stdout)
    if not response.get("ok") or proc.returncode:
        raise RuntimeError(response)
    return response.get("result")

def generate(path, bpm, duration, frequency):
    rate = 24000
    beat = 60 / bpm
    chunk = bytearray()
    with wave.open(str(path), "wb") as output:
        output.setparams((2, 2, rate, 0, "NONE", "not compressed"))
        for frame in range(duration * rate):
            t = frame / rate
            phase = t % beat
            bass_on = 16 <= t < duration - 16 and not 48 <= t < 52
            kick = 0.26 * math.exp(-phase * 38) * math.sin(2 * math.pi * 60 * t) if bass_on else 0
            mid = 0.06 * math.sin(2 * math.pi * frequency * t) * (0.6 + 0.4 * math.exp(-phase * 6))
            high = 0.07 * math.exp(-phase * 95) * math.sin(2 * math.pi * 4000 * t)
            value = int(max(-1, min(1, kick + mid + high)) * 32767)
            chunk.extend(struct.pack("<hh", value, value))
            if len(chunk) >= 65536:
                output.writeframesraw(chunk)
                chunk.clear()
        output.writeframes(chunk)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["start", "stop", "show"])
    parser.add_argument("name")
    parser.add_argument("--audio", action="store_true", help="Start real audio with master and phones muted")
    opts = parser.parse_args()
    if not opts.name or any(c not in "abcdefghijklmnopqrstuvwxyz0123456789-_" for c in opts.name):
        parser.error("name must be lowercase letters, digits, hyphens or underscores")
    dest = ROOT / "artifacts/skill-trials" / opts.name
    dest.mkdir(parents=True, exist_ok=True)
    manifest_path = dest / "profile.json"
    if opts.action in ["stop", "show"]:
        manifest = json.loads(manifest_path.read_text())
        if opts.action == "stop":
            status = request(manifest, "status")
            if not status["isolated"] or status["instance"] != manifest["pid"]:
                raise RuntimeError("Refusing to stop an instance other than this isolated trial")
            request(manifest, "app", "stop")
        print(json.dumps(manifest))
        return
    if manifest_path.exists():
        previous = json.loads(manifest_path.read_text())
        try:
            if request(previous, "status")["isolated"]:
                raise SystemExit("Trial already running; stop it or choose another name")
        except RuntimeError:
            pass
    profile = Path(tempfile.mkdtemp(prefix="pd-skill-", dir="/tmp")).resolve()
    bundle = profile / "PulseDeck.app"
    shutil.copytree(APP, bundle)
    manifest = {"name": opts.name, "profile": str(profile), "socket": str(profile / "control.sock"),
                "cli": str(CLI), "audio": opts.audio, "tracks": []}
    env = dict(os.environ, PULSEDECK_ISOLATED="1", PULSEDECK_DATA_DIR=str(profile / "data"),
               PULSEDECK_SOCKET=manifest["socket"])
    with (dest / "app.log").open("w") as log:
        proc = subprocess.Popen([str(bundle / "Contents/MacOS/pulsedeck")], env=env,
                                stdin=subprocess.DEVNULL, stdout=log, stderr=log, start_new_session=True)
    manifest["pid"] = proc.pid
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    try:
        deadline = time.monotonic() + 20
        while True:
            try:
                status = request(manifest, "status")
                assert status["isolated"] and status["instance"] == proc.pid
                break
            except (RuntimeError, json.JSONDecodeError):
                if time.monotonic() > deadline or proc.poll() is not None:
                    raise
                time.sleep(0.1)
        discovery = request(manifest, "commands", "list")
        (dest / "commands.json").write_text(json.dumps(discovery, indent=2) + "\n")
        for title, bpm, duration, freq in [("Pulse Intro", 120, 96, 261.63), ("Rising Pulse", 124, 112, 329.63), ("Final Pulse", 128, 80, 392)]:
            path = profile / (title + ".wav")
            generate(path, bpm, duration, freq)
            imported = request(manifest, "library", "import", "--path", path)
            track = imported[0]
            manifest["tracks"].append({"source": f"local:{track['id']}", "id": track["id"], "path": str(path),
                                       "duration_seconds": duration, "generated_bpm": bpm, "title": title})
        request(manifest, "mixer", "master-gain", "set", "0")
        request(manifest, "mixer", "cue-gain", "set", "0")
        if opts.audio:
            request(manifest, "audio", "start", "--buffer", "256")
            time.sleep(0.25)
        manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
        print(json.dumps(manifest))
    except BaseException:
        try:
            request(manifest, "app", "stop")
        except Exception:
            proc.terminate()
        raise

if __name__ == "__main__":
    main()
