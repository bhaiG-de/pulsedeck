#!/usr/bin/env python3
"""Score native predictions against the authors' GTZAN v1.1 beat annotations.
Requires numpy and mir_eval==0.8.2. No audio or model activations are published.
Use the fixed split manifest; development/evaluation are clip partitions and
GTZAN duplicates mean they are not proven recording-independent partitions.
"""
import argparse,json,statistics,hashlib,subprocess,zipfile,io
from pathlib import Path
import numpy as np,mir_eval
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--cache',type=Path,required=True)
parser.add_argument('--annotations',type=Path,required=True)
parser.add_argument('--worker',type=Path,required=True)
parser.add_argument('--split',choices=['development','evaluation'],required=True)
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--spectrograms',type=Path)
parser.add_argument('--expected-analysis-version',type=int,default=11)
args=parser.parse_args()
BASE=args.cache;ANN=args.annotations
def bpm(beats):
 span=next((x for x in [32,16,8,4,1] if len(beats)>x),1)
 return 60/statistics.median((beats[i+span]-beats[i])/span for i in range(len(beats)-span)) if len(beats)>1 else None
def metrics(ref,pred):
 r=np.array(ref);p=np.array(pred);out={}
 for trim in [0,5]:
  rr=r[r>=trim];pp=p[p>=trim]
  scores={'f1_'+str(ms):mir_eval.beat.f_measure(rr,pp,f_measure_threshold=ms/1000) for ms in [70,20,10]}
  if len(rr)>1 and len(pp)>1:
   c=mir_eval.beat.continuity(rr,pp);scores.update(dict(zip(['cmlc','cmlt','amlc','amlt'],map(float,c))))
  out[str(trim)]=scores
 return out
def score(row,pred):
 a=np.loadtxt(ANN/(row['id']+'.beats'),ndmin=2);beats=a[:,0];down=a[a[:,1]==1,0] if a.shape[1]>1 else np.array([])
 predbeats=np.array(pred['beats']);preddown=np.array(pred['downbeats']);duration=row.get('duration_seconds',(row['spectrogram_shape'][0]-1)/50)
 predbeats=predbeats[predbeats<duration];preddown=preddown[preddown<duration]
 gaps=np.diff(np.searchsorted(beats,down));pgaps=np.diff(np.searchsorted(predbeats,preddown))
 m=lambda gaps:int(max(set(gaps),key=list(gaps).count)) if len(gaps) else None
 rb=bpm(beats);pb=bpm(predbeats)
 return {'id':row['id'],'genre':row['id'].split('_')[1],'beats':metrics(beats,predbeats),'downbeats':metrics(down,preddown) if len(down)>1 else None,'reference_bpm':rb,'predicted_bpm':pb,'tempo_acc1':bool(pb and abs(pb/rb-1)<=.04),'tempo_acc2':bool(pb and any(abs(pb*f/rb-1)<=.04 for f in [.5,1,2])),'reference_meter_mode':m(gaps),'predicted_meter_mode':m(pgaps),'reference_meter_changes':len(set(gaps))>1}

rows=[]
spectra=zipfile.ZipFile(args.spectrograms) if args.spectrograms else None
worker=subprocess.Popen([str(args.worker.resolve())],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
try:
 for file in sorted(BASE.glob('gtzan*.json')):
  row=json.loads(file.read_text())
  if row['split']!=args.split:continue
  if row.get('production_pcm_result'):
   # Score the actual full PCM result, including its acoustic evidence; saved
   # logits alone cannot reproduce the mel-based production policy.
   prediction={'beats':row['production_pcm_result']['beats_seconds'],'downbeats':row['production_pcm_result']['downbeats_seconds'],'decoder_analysis_version':row['decoder_analysis_version']}
  else:
   request={'id':row['id'],'logits_path':str(file),'policy':'production'}
   assert spectra is not None, 'production policy requires acoustic mel evidence'
   mel=np.load(io.BytesIO(spectra.read(row['id']+'/track.npy')))
   mel_path=args.output.with_suffix('.mel.f32');mel.astype('<f4').tofile(mel_path)
   request['evidence_mel_path']=str(mel_path)
   worker.stdin.write(json.dumps(request)+'\n');worker.stdin.flush()
   prediction=json.loads(worker.stdout.readline())
  assert prediction['decoder_analysis_version']==args.expected_analysis_version, 'stale benchmark worker/PCM cache'
  rows.append({'baseline':score(row,row),'current':score(row,prediction),'annotation_sha256':hashlib.sha256((ANN/(row['id']+'.beats')).read_bytes()).hexdigest(),'spectrogram_npy_sha256':row['spectrogram_npy_sha256'],'audio':row.get('audio')})
finally:
 worker.stdin.close();assert worker.wait()==0
 if spectra:spectra.close();args.output.with_suffix('.mel.f32').unlink(missing_ok=True)
summary={}
for policy in ['baseline','current']:
 result={}
 for kind in ['beats','downbeats']:
  result[kind]={}
  for trim in ['0','5']:
   subset=[r[policy][kind][trim] for r in rows if r[policy][kind]]
   result[kind][trim]={'track_count':len(subset),**{k:statistics.mean(r[k] for r in subset if k in r) for k in ['f1_70','f1_20','f1_10','cmlt','amlt']}}
 result['tempo_acc1']=statistics.mean(r[policy]['tempo_acc1'] for r in rows)
 result['tempo_acc2']=statistics.mean(r[policy]['tempo_acc2'] for r in rows)
 summary[policy]=result
output={'decoder_analysis_version':args.expected_analysis_version,'mir_eval_version':mir_eval.__version__,'split':args.split,'tracks':len(rows),'macro_mean_per_track':summary,'per_track':rows,'method':'Standard mir_eval F1 at 70/20/10 ms; CMLt/AMLt; separate 0 s/5 s trimming. Tempo Acc1 within 4%; Acc2 allows 0.5x/1x/2x. Input provenance is recorded separately; this is not a rekordbox comparison.'}
args.output.write_text(json.dumps(output,indent=2)+'\n')
print(json.dumps({k:v for k,v in output.items() if k!='per_track'},indent=2))
