"""Export a memory-bounded variant of the model and time ONNX Runtime backends.

Variant: the frontend's time-direction attention runs over 32 frequency rows
at once (32 x 1500 x 1500 scores). Splitting that batch into groups gives the
same math with a fraction of the peak memory."""

import json
import os
import resource
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort
import torch
import torch.nn.functional as F

from beat_this.inference import load_model
from beat_this.model import roformer

ROOT = Path(os.environ.get("SPIKE_ROOT", Path(__file__).parent))
MODELS = ROOT / "models"
CHUNK = 1500


class Wrapper(torch.nn.Module):
    def __init__(self, model):
        super().__init__()
        self.model = model

    def forward(self, spect):
        out = self.model(spect)
        return out["beat"], out["downbeat"]


def export_grouped(group):
    original = roformer.Attend.forward

    def grouped(self, q, k, v):
        if q.shape[2] < 1000 or q.shape[0] <= group:
            return original(self, q, k, v)
        parts = [
            F.scaled_dot_product_attention(a, b, c)
            for a, b, c in zip(q.split(group), k.split(group), v.split(group))
        ]
        return torch.cat(parts, 0)

    model = Wrapper(load_model(str(MODELS / "small0.ckpt"))).eval()
    x = torch.randn(1, CHUNK, 128)
    with torch.inference_mode():
        expected = model(x)
    roformer.Attend.forward = grouped
    path = MODELS / f"beat_this_small0_g{group}.onnx"
    torch.onnx.export(model, (x,), str(path), input_names=["spect"],
                      output_names=["beat", "downbeat"], opset_version=17, dynamo=False)
    roformer.Attend.forward = original
    sess = ort.InferenceSession(str(path), providers=["CPUExecutionProvider"])
    got = sess.run(None, {"spect": x.numpy()})
    err = max(float(np.abs(g - e.numpy()).max()) for g, e in zip(got, expected))
    print(json.dumps({"variant": path.name, "max_diff_vs_torch": err,
                      "size_mb": round(path.stat().st_size / 2**20, 2)}))


def time_ort(model, provider, threads):
    """Runs in a fresh process so peak RSS belongs to this configuration."""
    opts = ort.SessionOptions()
    opts.intra_op_num_threads = threads
    providers = [provider] if provider == "CPUExecutionProvider" else [provider, "CPUExecutionProvider"]
    t = time.perf_counter()
    sess = ort.InferenceSession(str(MODELS / model), opts, providers=providers)
    load = time.perf_counter() - t
    x = np.random.randn(1, CHUNK, 128).astype(np.float32)
    sess.run(None, {"spect": x})  # warm-up / CoreML compile
    t = time.perf_counter()
    for _ in range(3):
        sess.run(None, {"spect": x})
    per = (time.perf_counter() - t) / 3
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 2**20
    print(json.dumps({"runner": f"onnxruntime {provider}", "model": model, "threads": threads,
                      "load_s": round(load, 2), "ms_per_chunk": round(per * 1e3, 1),
                      "peak_rss_mb": round(rss)}))


if __name__ == "__main__":
    if sys.argv[1] == "export":
        for g in map(int, sys.argv[2:]):
            export_grouped(g)
    elif sys.argv[1] == "ort":
        time_ort(sys.argv[2], sys.argv[3], int(sys.argv[4]))
    elif sys.argv[1] == "ort-all":
        for model in sys.argv[2:]:
            for provider, threads in [("CPUExecutionProvider", 1), ("CPUExecutionProvider", 4),
                                      ("CoreMLExecutionProvider", 4)]:
                subprocess.run([sys.executable, __file__, "ort", model, provider, str(threads)])
