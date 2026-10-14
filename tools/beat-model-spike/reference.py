"""Beat This! spike reference: export the small model to ONNX and write the
intermediate data (22.05 kHz mono signal, log-mel spectrogram, frame logits,
final beats) that the Rust implementation must reproduce."""

import json
import os
import sys
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort
import soxr
import torch
import torchaudio

from beat_this.inference import load_model, split_predict_aggregate
from beat_this.model.postprocessor import Postprocessor
from beat_this.preprocessing import LogMelSpect, load_audio

ROOT = Path(os.environ.get("SPIKE_ROOT", Path(__file__).parent))
CKPT = ROOT / "models" / "small0.ckpt"
ONNX = ROOT / "models" / "beat_this_small0.onnx"
OUT = ROOT / "ref"
CHUNK = 1500


class Wrapper(torch.nn.Module):
    def __init__(self, model):
        super().__init__()
        self.model = model

    def forward(self, spect):
        out = self.model(spect)
        return out["beat"], out["downbeat"]


def export(model):
    dummy = torch.randn(1, CHUNK, 128)
    wrapper = Wrapper(model).eval()
    with torch.inference_mode():
        expected = wrapper(dummy)
    try:
        torch.onnx.export(
            wrapper, (dummy,), str(ONNX), input_names=["spect"],
            output_names=["beat", "downbeat"], opset_version=17, dynamo=False,
        )
        exporter = "torchscript"
    except Exception as e:  # legacy exporter unavailable
        print("legacy export failed:", e)
        torch.onnx.export(
            wrapper, (dummy,), str(ONNX), input_names=["spect"],
            output_names=["beat", "downbeat"], opset_version=18, dynamo=True,
            external_data=False,
        )
        exporter = "dynamo"
    sess = ort.InferenceSession(str(ONNX), providers=["CPUExecutionProvider"])
    got = sess.run(None, {"spect": dummy.numpy()})
    diff = max(float(np.abs(g - e.numpy()).max()) for g, e in zip(got, expected))
    print(f"exported with {exporter}; onnxruntime vs torch max |diff| = {diff:.2e}")
    return sess


def main(paths):
    OUT.mkdir(exist_ok=True)
    model = load_model(str(CKPT))
    print("params:", sum(p.numel() for p in model.parameters()))
    sess = export(model)
    fb = torchaudio.functional.melscale_fbanks(513, 30, 11000, 128, 22050, None, "slaney")
    fb.numpy().astype("<f4").tofile(OUT / "melfb.f32")
    spect_fn = LogMelSpect()
    post = Postprocessor("minimal")
    summary = []
    for i, path in enumerate(paths):
        name = f"t{i}"
        t0 = time.perf_counter()
        signal, sr = load_audio(path)
        if signal.ndim == 2:
            signal = signal.mean(1)
        sig = soxr.resample(signal, in_rate=sr, out_rate=22050).astype(np.float32)
        t1 = time.perf_counter()
        with torch.inference_mode():
            spect = spect_fn(torch.tensor(sig))
            t2 = time.perf_counter()
            pred = split_predict_aggregate(spect, CHUNK, 6, "keep_first", model)
        t3 = time.perf_counter()
        beats, downbeats = post(pred["beat"], pred["downbeat"])

        def onnx_model(chunk):
            n = chunk.shape[1]
            x = torch.nn.functional.pad(chunk, (0, 0, 0, CHUNK - n)).numpy()
            b, d = sess.run(None, {"spect": x})
            return {"beat": torch.from_numpy(b[:, :n]), "downbeat": torch.from_numpy(d[:, :n])}

        opred = split_predict_aggregate(spect, CHUNK, 6, "keep_first", onnx_model)
        onnx_diff = float((opred["beat"] - pred["beat"]).abs().max())
        sig.astype("<f4").tofile(OUT / f"{name}.sig.f32")
        spect.numpy().astype("<f4").tofile(OUT / f"{name}.spect.f32")
        pred["beat"].numpy().astype("<f4").tofile(OUT / f"{name}.beat.f32")
        pred["downbeat"].numpy().astype("<f4").tofile(OUT / f"{name}.downbeat.f32")
        rec = {
            "name": name, "path": path, "source_rate": int(sr),
            "frames": int(spect.shape[0]), "beats": beats.tolist(),
            "downbeats": downbeats.tolist(), "onnx_vs_torch_max_logit_diff": onnx_diff,
            "python_seconds": {"decode_resample": t1 - t0, "spect": t2 - t1, "model": t3 - t2},
        }
        (OUT / f"{name}.json").write_text(json.dumps(rec))
        summary.append(rec)
        print(f"{name}: {Path(path).name[:50]} frames={rec['frames']} beats={len(beats)} "
              f"downbeats={len(downbeats)} onnx_diff={onnx_diff:.2e} "
              f"model={t3 - t2:.2f}s")
    (OUT / "index.json").write_text(json.dumps([{"name": r["name"], "path": r["path"]} for r in summary]))


if __name__ == "__main__":
    main(sys.argv[1:])
