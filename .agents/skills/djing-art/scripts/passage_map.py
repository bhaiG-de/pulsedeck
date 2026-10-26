#!/usr/bin/env python3
"""Condense captured track_inspect MCP JSON; never connect to or control the app."""
import argparse
import importlib.util
import json
from pathlib import Path
import sys


def load_mapper():
    path=Path(__file__).with_name('structure_map.py')
    spec=importlib.util.spec_from_file_location('dj_structure_map',path)
    module=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build(overview, extras, save_dir, bars=4):
    mapper=load_mapper()
    files=[Path(overview),*[Path(p) for p in extras]]
    documents=[mapper.read_inspection(path) for path in files]
    source=documents[0]['source']
    if any(d['source']!=source for d in documents):
        raise ValueError('captured packets belong to different sources')
    events=[event for d in documents for event in (d.get('beat_events') or {}).get('events',[])]
    if not events:
        raise ValueError('no effective events; retain the waveform trajectory without invented bar counts')
    for document in documents:
        evidence=document.get('beat_events') or {}
        values=evidence.get('events') or []
        if evidence.get('truncated') or evidence.get('total_in_range',len(values))>len(values):
            bounds=document.get('requested_range') or {}
            left,right=bounds.get('start_seconds'),bounds.get('end_seconds')
            if left is None or right is None or evidence.get('total_in_range') is None:
                raise ValueError('truncated capture lacks coverage bounds; obtain complete bounded track_inspect packets')
            merged={event['beat_position'] for event in events if left-1e-9<=event['seconds']<=right+1e-9}
            if len(merged)<evidence['total_in_range']:
                raise ValueError('event range remains incomplete; capture additional bounded track_inspect JSON')
    metadata=documents[0].get('metadata') or {}
    keys=('bpm','bpm_source','beatgrid_offset_seconds','beatgrid_source','beatgrid_uniform','manual_grid_anchor_index')
    grid={key:metadata.get(key) for key in keys}
    if any({key:(d.get('metadata') or {}).get(key) for key in keys}!=grid for d in documents):
        raise ValueError('source grid/tempo context changed between captures')
    rhythm=metadata.get('neural_rhythm') or {}
    meter=rhythm.get('beats_per_bar')
    meter=meter if isinstance(meter,int) and not isinstance(meter,bool) and meter>0 else None
    downbeats=[event for event in events if event.get('downbeat')]
    origin=None if downbeats else min(events,key=lambda e:e['seconds'])['beat_position']
    result=mapper.summarize(files[0],files[1:],bars,meter or 1,origin)
    if result['coverage']['gaps'] or result['coverage']['omitted_blocks']:
        raise ValueError('captured effective-event coverage has gaps; obtain missing bounded MCP inspections')
    indexed={event['beat_position']:event for event in events}
    for block in result['blocks']:
        block['start_event']=indexed[block['start_beat']]
        block['end_event']=indexed[block['end_beat']]
    result['counting'].update({'observed_meter':meter,'bar_identity':'effective_downbeat_observed' if downbeats and meter else 'unknown','phrase_origins_verified':False,'beat_intervals_per_block':bars*(meter or 1)})
    if not downbeats or meter is None:
        result['counting']['bars_per_block']=None
        result['counting']['origin_basis']='counted intervals only; not a verified bar or phrase origin'
    if meter is None:result['counting']['beats_per_bar']=None
    beat=documents[0].get('beat_events') or {}
    result['clock_context']={'effective_uniform':beat.get('uniform'),'effective_manual_origin':beat.get('manual'),'effective_event_source':beat.get('source'),'source_bpm':metadata.get('bpm'),'bpm_source':metadata.get('bpm_source'),'manual_bpm_override':metadata.get('bpm_source')=='manual','stored_neural_bpm':rhythm.get('bpm'),'observed_downbeat_events':bool(downbeats),'grid_metadata':grid,'interpretation':'Effective clock provenance; acoustic pulse, bar correctness and phrases are not certified.'}
    result['requested_source']=source
    result['requested_passage']=documents[0].get('requested_range')
    directory=Path(save_dir);directory.mkdir(parents=True,exist_ok=True)
    with (directory/'structure-map.json').open('x') as handle:json.dump(result,handle,indent=2,allow_nan=False)
    return result


def compact(result, save_dir):
    rounded = lambda values: [round(values[b], 5) if values[b] is not None else None for b in ('low', 'mid', 'high')]
    blocks = [{'start_event': b['start_event'], 'end_event': b['end_event'],
        'rms_raw': rounded(b['rms_raw']), 'rms_normalized': rounded(b['rms_normalized']),
        'delta_db': rounded(b['delta_db_from_previous']) if b['delta_db_from_previous'] else None,
        'waveform_coverage': round(b['waveform_coverage_fraction'], 6),
        'peak_raw': round(b['peak_raw_max'], 5) if b['peak_raw_max'] is not None else None} for b in result['blocks']]
    return {'schema': 'passage-map-compact-v1', 'track': result['track'], 'identity': result['identity'],
        'requested_passage': result['requested_passage'], 'clock_context': result['clock_context'],
        'counting': result['counting'], 'band_order': ['low', 'mid', 'high'],
        'gain_context': {k: result['measurements'][k] for k in ('normalization_gain_db', 'integrated_lufs', 'scope')},
        'coverage': {k: result['coverage'][k] for k in ('unique_events', 'segments', 'gaps', 'omitted_blocks', 'events_truncated_in_any_input')},
        'blocks': blocks, 'full_map_file': str(Path(save_dir) / 'structure-map.json')}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('overview',type=Path)
    parser.add_argument('--events',type=Path,action='append',default=[])
    parser.add_argument('--save-dir',type=Path,required=True)
    parser.add_argument('--bars',type=int,choices=(4,8),default=4)
    parser.add_argument('--full',action='store_true')
    args=parser.parse_args()
    try:result=build(args.overview,args.events,args.save_dir,args.bars)
    except (ValueError,OSError,KeyError,TypeError) as error:parser.exit(2,f'passage_map: {error}\n')
    json.dump(result if args.full else compact(result,args.save_dir),sys.stdout,allow_nan=False,separators=(',',':'));print()


if __name__=='__main__':main()
