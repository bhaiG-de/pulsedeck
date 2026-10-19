#!/usr/bin/env python3
"""Actual desktop, audio-disabled restart/receipt checks using an explicit real-song move capture."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / 'target/debug/pulsedeckctl'
APP = ROOT / 'target/debug/bundle/macos/PulseDeck.app/Contents/MacOS/pulsedeck'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    move = json.loads(args.manifest.read_text())['option']
    # Simulate a durable acceptance immediately before the app exits during preparation.
    operation = 'move-restart-proof'
    request_id = 'execute-restart-proof'
    action = {'option_id': move['id'], 'review_confirmed': True}
    fingerprint = json.dumps(['mix.execute', action, None, False], separators=(',', ':'), sort_keys=True)
    receipt = {'protocol_version': 1, 'request_id': request_id, 'ok': True, 'stage': 'accepted',
               'result': {'operation_id': operation, 'state': 'preparing', 'event_sequence': 80}}
    record = {'operation_id': operation, 'prepared_id': operation, 'option': move,
              'state': 'preparing', 'job_id': None, 'error': None, 'automatic': True}
    with tempfile.TemporaryDirectory(prefix='pd-move-restart-') as temporary:
        profile = Path(temporary)
        data = profile / 'data'
        data.mkdir()
        socket = profile / 'control.sock'
        (data / 'control-journal.json').write_text(json.dumps({
            'version': 1, 'jobs': [], 'moves': [record], 'receipts': [[request_id, fingerprint, receipt]],
            'events': [], 'event_sequence': 8,
        }))
        env = dict(os.environ, PULSEDECK_ISOLATED='1', PULSEDECK_DATA_DIR=str(data), PULSEDECK_SOCKET=str(socket))
        def call(method, payload=None, timeout=10):
            completed = subprocess.run([str(CLI), 'call', method, '--args', json.dumps(payload or {}),
                                        '--socket', str(socket), '--json'], text=True, capture_output=True, timeout=timeout)
            return json.loads(completed.stdout)
        with (profile / 'app.log').open('w') as log:
            app = subprocess.Popen([str(APP)], env=env, stdout=log, stderr=log, start_new_session=True)
        try:
            deadline = time.monotonic() + 20
            while True:
                try:
                    status = call('status')
                    if status['ok']:
                        break
                except (ValueError, OSError):
                    pass
                if app.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError((profile / 'app.log').read_text())
                time.sleep(0.1)
            assert status['result']['isolated'] and status['result']['instance'] == app.pid
            assert not status['result']['engine']['running']
            restored = call('mix.status', {'operation_id': operation})
            assert restored['ok'] and restored['result']['state'] == 'interrupted', restored
            events = call('events.get', {'operation_id': operation, 'since': 8, 'wait_ms': 1000})
            assert events['ok']
            rewind = call('events.get', {'operation_id': operation, 'since': 80})
            assert rewind['result']['resync_required'], rewind
            assert any(e['data']['state'] == 'interrupted' for e in events['result']['events']), events
            # These used to take the submission mutex twice and hang before returning not_found.
            started = time.monotonic()
            cancel = call('mix.cancel', {'operation_id': 'missing'})
            preview = call('mix.preview', {'prepared_id': 'missing', 'enabled': True})
            assert cancel['error']['code'] == preview['error']['code'] == 'not_found'
            assert time.monotonic() - started < 3
            retry = subprocess.run([str(CLI), 'mix', 'execute', '--option-id', move['id'],
                                    '--review-confirmed', '--request-id', request_id, '--follow',
                                    '--socket', str(socket)], text=True, capture_output=True, timeout=10)
            lines = [json.loads(line) for line in retry.stdout.splitlines()]
            assert lines[0]['ok'] and lines[0]['stage'] == 'accepted', lines
            assert any(line.get('data', {}).get('state') == 'interrupted' for line in lines), lines
            assert retry.returncode != 0
            after = call('status')['result']
            assert not after['engine']['running']
            assert not any(d['playing'] for d in after['engine']['decks'])
            assert not after['jobs']
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps({'passed': True, 'restored': restored['result'],
                'events': events['result'], 'retry_output': lines, 'retry_exit': retry.returncode,
                'cancel_and_preview_return_without_deadlock': True,
                'hardware_output': False, 'playback_replayed': False}, indent=2) + '\n')
            print(json.dumps({'passed': True, 'unfinished_move_interrupted': True,
                'idempotent_follow_received_terminal_event': True, 'playback_replayed': False,
                'cancel_preview_deadlock_regression': True, 'rewound_event_cursor_resyncs': True}))
        finally:
            if app.poll() is None:
                try:
                    call('app.stop')
                    app.wait(timeout=5)
                except (OSError, ValueError, subprocess.TimeoutExpired):
                    app.terminate()
                    app.wait(timeout=5)


if __name__ == '__main__':
    main()
