#!/usr/bin/env python3
"""Cache native model activations and baseline events on official GTZAN spectra.
Requires numpy. Download the authors' gtzan.zip separately; original source audio
is neither requested nor exported. Four workers each use the app's bounded two
thread model pool. Annotation IDs/split manifest are fixed before scoring.
"""
import argparse,concurrent.futures,hashlib,io,json,subprocess,zipfile
from pathlib import Path
import numpy as np
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--archive',type=Path,required=True)
parser.add_argument('--manifest',type=Path,required=True)
parser.add_argument('--worker',type=Path,required=True)
parser.add_argument('--cache',type=Path,required=True)
parser.add_argument('--workers',type=int,default=2,choices=range(1,5))
args=parser.parse_args()
assert hashlib.md5(args.archive.read_bytes()).hexdigest()=='39a7dfe6a6b0a5279a94d770506db879','official archive checksum mismatch'
args.cache.mkdir(parents=True,exist_ok=True)
npz_path=args.cache/'gtzan.npz'
if not npz_path.exists():
 with zipfile.ZipFile(args.archive) as z:z.extract('gtzan.npz',args.cache)
rows=sorted(json.loads(args.manifest.read_text())['tracks'],key=lambda r:(r['split']!='development',r['id']))
def batch(index):
 worker=subprocess.Popen([str(args.worker.resolve())],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
 try:
  with zipfile.ZipFile(npz_path) as npz:
   names=set(npz.namelist())
   for row in rows[index::args.workers]:
    out=args.cache/(row['id']+'.json')
    if out.exists():continue
    name=row['id']+'/track.npy';assert name in names,row
    raw=npz.read(name);spect=np.load(io.BytesIO(raw));assert spect.ndim==2 and spect.shape[1]==128
    mel=args.cache/f'current-{index}.f32';spect.astype('<f4').tofile(mel)
    worker.stdin.write(json.dumps({'id':row['id'],'mel_path':str(mel),'meters':[3,4]})+'\n');worker.stdin.flush()
    response=json.loads(worker.stdout.readline())
    response.update(spectrogram_npy_sha256=hashlib.sha256(raw).hexdigest(),spectrogram_shape=list(spect.shape),spectrogram_dtype=str(spect.dtype),split=row['split'])
    out.write_text(json.dumps(response)+'\n')
    print(row['id'],flush=True)
 finally:
  worker.stdin.close();assert worker.wait()==0
with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:list(pool.map(batch,range(args.workers)))
