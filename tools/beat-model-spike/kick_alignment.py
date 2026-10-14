"""Objective phase check: where do kick-drum onsets fall relative to a grid?
For each grid, average a low-band (~34-155 Hz) onset envelope at grid times
shifted by -250..+250 ms; an aligned grid peaks near 0 ms. The envelope's
58 ms window biases every peak by roughly -25 ms, so compare grids with each
other rather than reading absolute values.

Usage: kick_alignment.py results.json   (the Rust spike's output)"""

import json
import os
import sys
from pathlib import Path

import numpy as np
import soxr

from beat_this.preprocessing import load_audio

SR = 2205
FRAME, HOP = 128, 11  # 58 ms window, 5 ms hop


def onset_envelope(path):
    signal, sr = load_audio(path)
    if signal.ndim == 2:
        signal = signal.mean(1)
    x = soxr.resample(signal, sr, SR).astype(np.float32)
    n = 1 + (len(x) - FRAME) // HOP
    frames = np.lib.stride_tricks.as_strided(x, (n, FRAME), (x.strides[0] * HOP, x.strides[0]))
    spec = np.abs(np.fft.rfft(frames * np.hanning(FRAME), axis=1))
    band = spec[:, 2:9].sum(1)  # ~34-155 Hz
    env = np.maximum(np.diff(np.log1p(100 * band), prepend=0), 0)
    times = (np.arange(n) * HOP + FRAME / 2) / SR
    return times, env


def profile(times, env, bpm, offset, duration):
    period = 60 / bpm
    grid = offset + period * np.arange(-1, int(duration / period) + 1)
    grid = grid[(grid > 1) & (grid < duration - 1)]
    shifts = np.arange(-250, 251, 5) / 1000
    vals = [np.interp(grid + s, times, env).mean() for s in shifts]
    best = shifts[int(np.argmax(vals))]
    return round(best * 1000), round(max(vals) / np.mean(vals), 2)


if __name__ == "__main__":
    root = Path(os.environ.get("SPIKE_ROOT", Path(__file__).parent))
    paths = {i["name"]: i["path"] for i in json.loads((root / "ref" / "index.json").read_text())}
    res = json.load(open(sys.argv[1]))
    for t in res["tracks"]:
        times, env = onset_envelope(paths[t["name"]])
        g, d = t["model_grid"], t["existing_dsp"]
        out = {"file": t["file"][:30], "model_peak_shift_ms": None, "dsp_peak_shift_ms": None}
        if g:
            out["model_peak_shift_ms"], out["model_peak_ratio"] = profile(times, env, g["bpm"], g["beat1_offset_s"], t["duration_s"])
        if d["bpm"] and d["phase_offset_s"] is not None:
            out["dsp_peak_shift_ms"], out["dsp_peak_ratio"] = profile(times, env, d["bpm"], d["phase_offset_s"], t["duration_s"])
        print(json.dumps(out))
