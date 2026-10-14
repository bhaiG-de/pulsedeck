#!/usr/bin/env python3
"""Opt-in Python oracle on saved Beat This! logits; not an app dependency.

Use an isolated Python 3.11 environment with madmom==0.16.1, numpy==1.26.4,
scipy==1.15.3 and Cython==0.29.37. No madmom model inference/weights are used.
See research/beat-detector-validation.md for reproducible installation steps.
"""
import argparse
import collections
import collections.abc
import inspect
import json
import textwrap
import time
import numpy as np

# Compatibility shims only in this isolated legacy reference process.
collections.MutableSequence = collections.abc.MutableSequence
np.float = float
np.int = int
np.complex = complex
from madmom.features.downbeats import DBNDownBeatTrackingProcessor

# NumPy >=1.24 rejects the reference's ragged (path, logp) result aggregation.
# Recompile only that method with explicit object dtype. State space, transition
# probabilities, Viterbi implementation and event correction are unmodified.
original = DBNDownBeatTrackingProcessor.process
source = textwrap.dedent(inspect.getsource(original))
needle = "np.asarray(results)[:, 1]"
if needle in source:
    namespace = {}
    exec(compile(source.replace(needle, "np.asarray(results, dtype=object)[:, 1]"),
                 "<madmom_numpy_compat>", "exec"), original.__globals__, namespace)
    DBNDownBeatTrackingProcessor.process = namespace["process"]

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("activations")
parser.add_argument("output")
parser.add_argument("--max-bpm", type=float, default=300.0)
args = parser.parse_args()
with open(args.activations) as source:
    data = json.load(source)
beat = np.asarray(data["beat_logits"], dtype=np.float64)
downbeat = np.asarray(data["downbeat_logits"], dtype=np.float64)
if beat.size == 0 or beat.shape != downbeat.shape or not np.isfinite(beat).all() or not np.isfinite(downbeat).all():
    raise ValueError("Invalid activation vectors")
epsilon = 1e-5
beat = (1 / (1 + np.exp(-beat))) * (1 - epsilon) + epsilon / 2
downbeat = (1 / (1 + np.exp(-downbeat))) * (1 - epsilon) + epsilon / 2
activations = np.vstack((np.maximum(beat - downbeat, epsilon / 2), downbeat)).T
processor = DBNDownBeatTrackingProcessor(
    beats_per_bar=[3, 4], min_bpm=55, max_bpm=args.max_bpm, fps=50,
    transition_lambda=100, num_tempi=60, observation_lambda=16,
    threshold=.05, correct=True, num_threads=1,
)
started = time.perf_counter()
result = processor(activations)
output = {
    "beats": result[:, 0].tolist(),
    "downbeats": result[result[:, 1] == 1, 0].tolist(),
    "provenance": {
        "postprocessing": "Python madmom DBN on exact PulseDeck Beat This! activations",
        "madmom_version": "0.16.1", "fps": 50, "beats_per_bar": [3, 4],
        "min_bpm": 55, "max_bpm": args.max_bpm, "transition_lambda": 100,
        "num_tempi": 60, "observation_lambda": 16, "threshold": .05,
        "correct": True, "extra_peak_refinement": False,
        "compatibility": "legacy collections/NumPy aliases and object-dtype result aggregation only",
        "activation_file": args.activations, "runtime_seconds": time.perf_counter() - started,
    },
}
with open(args.output, "w") as destination:
    json.dump(output, destination, indent=2)
print(f"Python reference: {len(output['beats'])} beats, {len(output['downbeats'])} downbeats")
