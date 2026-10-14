"""Store an exported model's weights as float16 while keeping float32 compute.

Each float32 initializer with at least 256 values becomes a float16
initializer followed by a Cast back to float32; runners fold the casts at load
time. This shrinks the grouped model from about 12 MB to about 8 MB.

Usage: weights_f16.py [source.onnx] [destination.onnx]   (names inside models/)"""

import os
import sys
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
from onnx import TensorProto, helper, numpy_helper

ROOT = Path(os.environ.get("SPIKE_ROOT", Path(__file__).parent))


def convert(src, dst):
    model = onnx.load(src)
    graph = model.graph
    casts, initializers = [], []
    for init in graph.initializer:
        values = numpy_helper.to_array(init)
        if values.dtype == np.float32 and values.size >= 256:
            half = numpy_helper.from_array(values.astype(np.float16), init.name + "__f16")
            initializers.append(half)
            casts.append(helper.make_node("Cast", [half.name], [init.name], to=TensorProto.FLOAT))
        else:
            initializers.append(init)
    del graph.initializer[:]
    graph.initializer.extend(initializers)
    nodes = list(graph.node)
    del graph.node[:]
    graph.node.extend(casts + nodes)
    onnx.save(model, dst)


def main():
    src = ROOT / "models" / (sys.argv[1] if len(sys.argv) > 1 else "beat_this_small0_g1.onnx")
    dst = ROOT / "models" / (sys.argv[2] if len(sys.argv) > 2 else "beat_this_small0_g1_w16.onnx")
    convert(src, dst)
    x = np.random.default_rng(0).standard_normal((1, 1500, 128)).astype(np.float32)
    a = ort.InferenceSession(str(src)).run(None, {"spect": x})
    b = ort.InferenceSession(str(dst)).run(None, {"spect": x})
    diff = float(max(np.abs(p - q).max() for p, q in zip(a, b)))
    mb = lambda p: round(p.stat().st_size / 2**20, 2)  # noqa: E731
    print(f"{src.name} {mb(src)} MB -> {dst.name} {mb(dst)} MB, max logit diff {diff:.4f}")


if __name__ == "__main__":
    main()
