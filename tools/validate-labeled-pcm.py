#!/usr/bin/env python3
"""Run original labeled local WAV clips through full PCM analysis, without audio export.
Requires ffmpeg and built release benchmark_rhythm/labeled_worker examples.
Annotations are scored separately with validate-labeled-rhythm.py.
"""
import json,subprocess,concurrent.futures
from pathlib import Path
import argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--manifest',type=Path,required=True)
parser.add_argument('--audio-root',type=Path,required=True)
parser.add_argument('--cache',type=Path,required=True)
parser.add_argument('--workers',type=int,choices=range(1,5),default=2)
args=parser.parse_args()
manifest={r['id']:r for r in json.loads(args.manifest.read_text())['tracks']}
base=args.cache;base.mkdir(exist_ok=True)
def run(audio):
 tid='gtzan_'+audio.stem.replace('.','_');logits=base/(tid+'.logits')
 p=subprocess.run(['target/release/examples/benchmark_rhythm',str(audio),'--activations',str(logits)],capture_output=True,text=True,timeout=120)
 assert not p.returncode,p.stderr
 neural=json.loads(p.stdout);raw=json.loads(logits.read_text())
 request={'id':tid,'logits_path':str(logits),'meters':[3,4]}
 w=subprocess.run(['target/release/examples/labeled_worker'],input=json.dumps(request)+'\n',capture_output=True,text=True,timeout=30);assert not w.returncode,w.stderr
 baseline=json.loads(w.stdout);baseline.update(spectrogram_npy_sha256=None,spectrogram_shape=[len(raw['beat_logits']),128],split=manifest[tid]['split'],duration_seconds=neural['audio']['duration_seconds'],audio=neural['audio'],production_pcm_result=neural['neural'])
 (base/(tid+'.json')).write_text(json.dumps(baseline));logits.unlink();print(tid,flush=True)
with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:list(pool.map(run, sorted(args.audio_root.glob('*/*.wav'))))
print('Full PCM pipelines complete.',flush=True)
