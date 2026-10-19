#!/usr/bin/env python3
"""Actual local PCM through the debug isolated null renderer; never opens an audio device."""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import sqlite3
import subprocess
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', type=Path, default=ROOT / 'target/debug/bundle/macos/PulseDeck.app/Contents/MacOS/pulsedeck')
    parser.add_argument('--library', type=Path, default=ROOT / 'artifacts/query-stress-2026-10-18/normal-before.sqlite3')
    parser.add_argument('--manifest', type=Path, default=ROOT / 'artifacts/single-call-moves-2026-10-19/eq_blend.json')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    capture = json.loads(args.manifest.read_text())
    opener = Path(capture['snapshot']['decks'][capture['option']['reference_deck']]['track']['path'])
    second = Path(capture['option']['source'])
    source_db = sqlite3.connect(f'file:{args.library.resolve()}?mode=ro', uri=True)
    row = source_db.execute("SELECT path FROM tracks WHERE title LIKE 'Explore Your Future%' LIMIT 1").fetchone()
    assert row, 'Actual third source is required in the explicit library snapshot'
    third = Path(row[0])
    assert all(p.is_file() for p in (opener, second, third))
    report = {'passed': False, 'hardware_output': False, 'clock': 'accelerated only during native batches; not wall-clock/performance evidence',
              'sources': [str(p) for p in (opener, second, third)], 'events': []}
    with tempfile.TemporaryDirectory(prefix='pd-successor-runtime-') as temporary:
        profile = Path(temporary)
        data = profile / 'data'
        data.mkdir()
        output_db = sqlite3.connect(data / 'library.sqlite3')
        source_db.backup(output_db)
        output_db.close()
        source_db.close()
        address = profile / 'control.sock'
        env = dict(os.environ, PULSEDECK_ISOLATED='1', PULSEDECK_DATA_DIR=str(data),
                   PULSEDECK_SOCKET=str(address), PULSEDECK_TEST_NULL_AUDIO='1')
        log = (profile / 'app.log').open('w')
        app = None

        def call(method, payload=None, request_id=None, timeout=90):
            request = {'protocol_version': 1, 'request_id': request_id or str(uuid.uuid4()),
                       'command': method, 'args': payload or {}, 'timeout_ms': int(timeout * 1000)}
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
                connection.settimeout(timeout + 2)
                connection.connect(str(address))
                connection.sendall(json.dumps(request).encode() + b'\n')
                result = json.loads(connection.makefile('rb').readline())
            if not result.get('ok'):
                raise RuntimeError(json.dumps({'request': request, 'response': result}))
            return result.get('result')

        def launch():
            process = subprocess.Popen([str(args.app)], env=env, stdout=log, stderr=log, start_new_session=True)
            deadline = time.monotonic() + 25
            while time.monotonic() < deadline:
                try:
                    status = call('status', timeout=2)
                    if status.get('instance') == process.pid and status.get('isolated') and status['engine']['running']:
                        return process
                except (OSError, ValueError, RuntimeError):
                    pass
                assert process.poll() is None, (profile / 'app.log').read_text()[-8000:]
                time.sleep(.1)
            raise RuntimeError('Isolated null renderer did not start: ' + (profile / 'app.log').read_text()[-8000:])

        def wait_for(predicate, timeout=180):
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                value = predicate()
                if value:
                    return value
                assert app.poll() is None, (profile / 'app.log').read_text()[-8000:]
                time.sleep(.05)
            raise TimeoutError('Queue condition timed out: ' + json.dumps(call('mix.queue.status')))

        try:
            app = launch()
            call('deck.load', {'deck': 0, 'path': str(opener)}, timeout=180)
            for command in [
                {'type': 'set_channel_fader', 'deck': 0, 'value': 1},
                {'type': 'set_channel_fader', 'deck': 1, 'value': 0},
                {'type': 'seek', 'deck': 0, 'seconds': max(0, capture['option']['launch_seconds'] - 100)},
                {'type': 'play', 'deck': 0},
            ]:
                call('engine.command', command)
            started = call('mix.queue.start', {'sources': [str(second), str(third)], 'provider': 'local',
                'energy_direction': 'keep', 'max_stretch_percent': 2, 'review_confirmed': True,
                'replace_prepared': True, 'max_skips': 3}, request_id='actual-two-successors')
            queue_id = started['queue_id']
            cursor = started.get('event_sequence', 0)
            deadline = time.monotonic() + 240
            while time.monotonic() < deadline:
                events = call('events.get', {'queue_id': queue_id, 'since': cursor, 'wait_ms': 1000}, timeout=2)
                cursor = events['sequence']
                report['events'].extend(events['events'])
                queue = call('mix.queue.status', {'queue_id': queue_id})
                if queue['terminal']:
                    break
            else:
                raise TimeoutError('Actual queue did not finish two native moves')
            assert queue['state'] == 'completed' and queue['completed_moves'] >= 2, queue
            assert [item['source'] for item in queue['items'] if item['state'] == 'completed'] == [str(second), str(third)], queue
            report['two_native_completions'] = queue

            cold = profile / 'unanalysed-owned-source.mp3'
            shutil.copyfile(opener, cold)
            stopped_queue = call('mix.queue.start', {'sources': [str(cold)], 'review_confirmed': True,
                'replace_prepared': True, 'max_skips': 2})
            def source_is_preparing():
                status = call('status')
                return status if any(j.get('preparing') and j.get('uri') == str(cold) for j in status['source_jobs']) else None
            preparing = wait_for(source_is_preparing, timeout=30)
            stopped = call('mix.queue.stop', {'queue_id': stopped_queue['queue_id']})
            assert stopped['state'] == 'stopped', stopped
            time.sleep(1)
            after = call('status')
            assert not any(j.get('preparing') and j.get('uri') == str(cold) for j in after['source_jobs']), after
            assert not any(d.get('playing') and d.get('track', {}).get('path') == str(cold) for d in after['engine']['decks']), after
            assert any(d['playing'] for d in after['engine']['decks']), after
            assert call('mix.queue.status')['state'] == 'stopped'
            report['stop_during_unknown_source_preparation'] = {'preparation_observed': True, 'queue': stopped,
                'healthy_carrier_preserved': any(d['playing'] for d in after['engine']['decks']),
                'late_source_not_playing': True, 'preparation_before_stop': preparing['source_jobs']}

            restart_action = {'provider': 'local', 'energy_direction': 'keep', 'review_confirmed': True,
                              'replace_prepared': True, 'duration_seconds': 300}
            receipt = call('mix.queue.start', restart_action, request_id='restart-queue-once')
            os.kill(app.pid, signal.SIGKILL)
            app.wait(timeout=5)
            app = launch()
            restored = call('mix.queue.status', {'queue_id': receipt['queue_id']})
            assert restored['state'] == 'interrupted' and restored['restored'] and restored['terminal'], restored
            assert restored['reason'] == 'app_restarted_no_replay', restored
            status = call('status')
            assert not any(d['playing'] for d in status['engine']['decks']) and not any(j['preparing'] for j in status['source_jobs']), status
            retry = call('mix.queue.start', restart_action, request_id='restart-queue-once')
            assert retry['queue_id'] == receipt['queue_id']
            assert call('mix.queue.status')['state'] == 'interrupted'
            report['restart_no_replay'] = {'restored': restored, 'same_id_receipt': True, 'empty_stopped_decks': True}
            report['passed'] = True
        finally:
            if app and app.poll() is None:
                try:
                    call('app.stop', timeout=5)
                    app.wait(timeout=5)
                except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired):
                    app.terminate()
                    app.wait(timeout=5)
            log.close()
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + '\n')
            args.output.with_suffix('.app.log').write_text((profile / 'app.log').read_text())
    print(json.dumps({'passed': report['passed'], 'two_native_completions': report.get('two_native_completions', {}).get('completed_moves'),
                      'stop_during_unknown_preparation': bool(report.get('stop_during_unknown_source_preparation')),
                      'restart_without_replay': bool(report.get('restart_no_replay')), 'hardware_output': False}))


if __name__ == '__main__':
    main()
