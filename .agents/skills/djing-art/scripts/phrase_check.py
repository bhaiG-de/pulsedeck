#!/usr/bin/env python3
"""Read-only validation of a selected two-deck musical timeline. Never controls playback."""
import argparse
import json
import math
import subprocess
import sys


def payload(value):
    if 'ok' in value and not value['ok']:
        raise ValueError(str(value.get('error', value)))
    return value.get('result', value)


def anchor(packet, seconds):
    events = packet.get('beat_events', {})
    if events.get('truncated'):
        raise ValueError('Event interval is truncated; narrow the proposed overlap.')
    values = events.get('events', [])
    if not values:
        raise ValueError('No effective beat events in selected interval.')
    event = min(values, key=lambda e: abs(e['seconds'] - seconds))
    if abs(event['seconds'] - seconds) > .001:
        raise ValueError(f'{seconds} is not an inspected beat; nearest is {event["seconds"]}.')
    if event.get('beat_position') is None:
        raise ValueError('Missing effective beat_position.')
    return {k: event[k] for k in ('seconds', 'beat_position', 'downbeat')}


def meter(packet):
    value = (packet.get('metadata', {}).get('neural_rhythm') or {}).get('beats_per_bar')
    if value not in (3, 4, 5, 7):
        raise ValueError('Meter/origin needs evidence; cannot assume four.')
    return value


def evidence(packet, seconds):
    points = packet.get('metadata', {}).get('cue_points', [])
    found = []
    for p in points:
        if abs(p.get('start_seconds', -1e9) - seconds) <= .001:
            name = p.get('name') or p.get('label') or p.get('role')
            if name:
                item = {k: p.get(k) for k in ('name', 'role', 'origin', 'confirmed')}
                if item not in found:
                    found.append(item)
    return found


def check(a, b, state, out, inc, launch, handover, entry, arrival, technique='blend', preflight=False):
    """Validate arithmetic/transport evidence, not whether the selected locations are musical phrase boundaries."""
    errors = []
    a0, a1, b0, b1 = anchor(a, launch), anchor(a, handover), anchor(b, entry), anchor(b, arrival)
    ma, mb = meter(a), meter(b)
    la = a1['beat_position'] - a0['beat_position']
    lb = b1['beat_position'] - b0['beat_position']
    if min(la, lb) < 0:
        errors.append('Arrival precedes entry.')
    if ma != mb:
        errors.append('Different meters: this same-meter linear beatmix check does not apply; choose compatible passages or review the metrical relationship separately.')
    if la != lb:
        errors.append(f'Unequal leads: outgoing {la} beats, incoming {lb} beats.')
    if technique == 'blend' and (la <= 0 or lb <= 0):
        errors.append('A lead-in blend needs positive approach intervals before the joint foreground arrival. Choose a complete earlier incoming entry and corresponding outgoing launch.')
    if ma == mb and (a0['beat_position'] - b0['beat_position']) % ma != 0:
        errors.append('Launch and entry are in different parts of the bar.')
    if not a1['downbeat'] or not b1['downbeat']:
        errors.append('Selected foreground arrivals are not both published downbeats.')
    if technique == 'blend' and any(
        (p.get('beat_events', {}).get('local_tempo') or {}).get('pulse_review_required')
        or p.get('metadata', {}).get('rhythm_error')
        or p.get('metadata', {}).get('bpm_ambiguous') for p in (a, b)
    ):
        errors.append('This overlap contains pulse/grid uncertainty; inspect another passage before a sustained beatmix.')
    ev = {'outgoing_launch': evidence(a, a0['seconds']),
          'outgoing_handover': evidence(a, a1['seconds']),
          'incoming_entry': evidence(b, b0['seconds']),
          'incoming_arrival': evidence(b, b1['seconds'])}
    engine = state['engine']
    da, db = engine['decks'][out], engine['decks'][inc]
    if preflight and da['playing']:
        errors.append('Preflight requires a paused outgoing source; use the live check while playing.')
    if not preflight and not da['playing']:
        errors.append('Outgoing clock is not playing.')
    if db['playing']:
        errors.append('Incoming source already advances; reconcile its actual timeline before a new planned launch.')
    if launch <= da['position_seconds']:
        errors.append('Launch has passed; select a future musical opportunity.')
    for d in (da, db):
        if not d.get('beatgrid_valid') or d.get('preparation_pending'):
            errors.append('A source lacks a ready effective grid.')
        if d.get('loop_start_seconds') is not None:
            errors.append('This linear plan does not account for the active loop.')
    synced = da.get('is_master') and db.get('sync_enabled') and not db.get('is_master')
    static_match = (da.get('beatgrid_uniform') and db.get('beatgrid_uniform')
                    and abs(da.get('actual_playback_bpm', 0) - db.get('actual_playback_bpm', 0)) < .001)
    if technique == 'blend' and not (synced or static_match):
        errors.append('Sustained rhythmic overlap needs the outgoing master/incoming Sync follower (or matched uniform clocks). Prepare that strategy and recheck.')
    if technique == 'cut' and (la or lb):
        errors.append('A phrase cut starts directly at the joint arrival: both leads must be zero.')
    return {'timing_pass': not errors, 'errors': errors, 'technique': technique,
            'preflight': preflight, 'execution_ready': not preflight and not errors,
            'scope': 'clock_coordinates_only', 'audible_overlap': 'not_evaluated',
            'outgoing': {'launch': a0, 'handover': a1, 'lead_beats': la, 'meter': ma},
            'incoming': {'entry': b0, 'arrival': b1, 'lead_beats': lb, 'meter': mb},
            'anchor_evidence': ev,
            'source_guards': {k: engine.get(k) for k in ('load_revisions', 'engine_epoch')},
            'remaining_to_launch_source_seconds': launch - da['position_seconds'],
            'sync_strategy_ready': bool(synced),
            'musical_review_required': 'This checks clock coordinates only. Manual/imported cues are optional evidence, not proof of a phrase. Inspect surrounding passages; audible overlap and musical quality are not evaluated.'}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cli', default='pulsedeckctl')
    p.add_argument('--outgoing', choices=['A', 'B'], required=True)
    p.add_argument('--incoming', choices=['A', 'B'], required=True)
    for name in ('launch', 'handover', 'entry', 'arrival'):
        p.add_argument('--' + name, type=float, required=True)
    p.add_argument('--technique', choices=['blend', 'cut'], default='blend')
    p.add_argument('--preflight', action='store_true', help='Check prepared paused sources before the opener starts; does not certify live execution.')
    p.add_argument('--save', help='Optional JSON evidence output path')
    args = p.parse_args()
    def call(command, data):
        argv = ([args.cli, 'status', '--view', 'compact', '--json'] if command == 'state.get'
                else [args.cli, 'call', command, '--args', json.dumps(data), '--json'])
        result = subprocess.run(argv,
                                capture_output=True, text=True, check=True, timeout=20)
        return payload(json.loads(result.stdout))
    try:
        times = [args.launch, args.handover, args.entry, args.arrival]
        if not all(math.isfinite(t) and t >= 0 for t in times):
            raise ValueError('Source times must be finite and nonnegative.')
        if args.outgoing == args.incoming:
            raise ValueError('Choose different outgoing/incoming decks.')
        before = call('state.get', {'view': 'compact'})
        def inspect(deck, lo, hi):
            # Duration comes from the same bound loaded source, not an invented end.
            i = 0 if deck == 'A' else 1
            duration = before['metadata']['tracks'][i]['duration_seconds']
            if max(lo, hi) >= duration:
                raise ValueError('Selected anchor is outside the loaded source.')
            return call('track.inspect', {'deck': deck, 'start_seconds': max(0, min(lo, hi)-.01),
                        'end_seconds': min(duration, max(lo, hi)+.01), 'points': 512})
        a = inspect(args.outgoing, args.launch, args.handover)
        b = inspect(args.incoming, args.entry, args.arrival)
        after = call('state.get', {'view': 'compact'})
        for field in ('load_revisions',):
            if before['engine'][field] != after['engine'][field]:
                raise ValueError('Sources changed during inspection; inspect the new sources.')
        for i in (0, 1):
            for field in ('grid_revision', 'position_revision'):
                if before['engine']['decks'][i][field] != after['engine']['decks'][i][field]:
                    raise ValueError('Grid/seek changed during inspection; recheck.')
        out, inc = ('AB'.index(args.outgoing), 'AB'.index(args.incoming))
        result = check(a, b, after, out, inc, *times, args.technique, args.preflight)
        if args.save:
            with open(args.save, 'w') as f:
                json.dump({'check': result, 'outgoing_packet': a, 'incoming_packet': b, 'state': after}, f, indent=2)
        print(json.dumps(result, indent=2))
        return 0 if result['timing_pass'] else 1
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as e:
        print(json.dumps({'timing_pass': False, 'errors': [str(e)]}, indent=2))
        return 2


if __name__ == '__main__':
    sys.exit(main())
