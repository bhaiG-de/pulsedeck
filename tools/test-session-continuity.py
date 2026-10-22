#!/usr/bin/env python3
"""Exercise timed manual performance in an isolated null renderer, without hardware audio."""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', type=Path, default=Path('target/debug/pulsedeck'))
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    report = {'passed': False, 'hardware_output': False, 'cases': {}}
    with tempfile.TemporaryDirectory(prefix='pd-session-continuity-') as temporary:
        root = Path(temporary)
        address = root / 'control.sock'
        env = dict(os.environ, PULSEDECK_ISOLATED='1', PULSEDECK_DATA_DIR=str(root / 'data'),
                   PULSEDECK_SOCKET=str(address), PULSEDECK_TEST_NULL_AUDIO='1')
        log = (root / 'app.log').open('w')
        app = subprocess.Popen([str(args.app.resolve())], env=env, stdout=log, stderr=log)

        def call(command, payload=None):
            request = {'protocol_version': 1, 'request_id': str(uuid.uuid4()),
                       'command': command, 'args': payload or {}, 'timeout_ms': 5000}
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
                client.settimeout(7)
                client.connect(str(address))
                client.sendall(json.dumps(request).encode() + b'\n')
                response = json.loads(client.makefile('rb').readline())
            if response.get('error', {}).get('code') == 'busy':
                raise BlockingIOError(response['error']['message'])
            assert response['ok'], response
            return response['result']

        def wait_for(predicate, seconds=10):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                try:
                    value = predicate()
                    if value:
                        return value
                except (OSError, ValueError):
                    pass
                assert app.poll() is None, (root / 'app.log').read_text()[-3000:]
                time.sleep(.02)
            raise TimeoutError('Timed out waiting for session condition')

        def command(kind, **kwargs):
            return call('engine.command', {'type': kind, **kwargs})

        def transport():
            return call('engine.transport')

        def prepare():
            for deck in [0, 1]:
                command('pause', deck=deck)
                call('deck.load', {'deck': deck, 'demo': True})
                command('set_channel_fader', deck=deck, value=1 if deck == 0 else 0)
            command('play', deck=0)
            return transport()

        try:
            wait_for(lambda: call('status'))
            snapshot = prepare()
            finish = int(time.time() * 1000) + 8000
            session = call('mix.session.start', {'finish_unix_ms': finish})
            assert session['finish_unix_ms'] == finish, session
            for kind, payload in [('set_master_gain', {'gain': 1.5}),
                                  ('set_bpm', {'deck': 0, 'bpm': 122.9}),
                                  ('cancel_ramps', {}),
                                  ('ramp', {'target': {'type': 'tempo_percent', 'deck': 0},
                                            'value': 0, 'seconds': .1, 'clock': None,
                                            'beats': None, 'quantize': None})]:
                command(kind, **payload)
                current = call('mix.session.status')
                assert current['state'] == 'running' and current['finish_unix_ms'] == finish, current
            duration = snapshot['decks'][0]['track']['duration_seconds']
            command('seek', deck=0, seconds=duration)
            wait_for(lambda: call('mix.session.status')['continuity_notice'] == 'no_audible_carrier_route')
            assert call('mix.session.status')['state'] == 'running'
            command('toggle_play', deck=1)
            command('toggle_play', deck=1)
            assert call('mix.session.status')['state'] == 'running'
            report['cases']['controls_and_gap_preserve_absolute_endpoint'] = True

            command('seek', deck=0, seconds=0)
            command('play', deck=0)
            command('set_eq', deck=1, low=0, mid=.7, high=.5)
            command('set_filter', deck=1, amount=.65)
            command('set_fx', deck=1, echo=.4, reverb=.2)
            armed = call('mix.session.successor', {'deck': 'B'})
            assert armed['successor']['deck'] == 1
            command('seek', deck=0, seconds=duration - .8)
            resumed = wait_for(lambda: (s if s['decks'][1]['playing'] and
                                        s['decks'][1]['channel_fader'] == 1 and
                                        s['decks'][1]['position_seconds'] > 0 and
                                        s['decks'][0]['channel_fader'] == 0 and
                                        not s['decks'][0]['playing'] else None)
                               if (s := transport()) else None)
            assert resumed['decks'][1]['position_seconds'] > 0
            assert resumed['decks'][0]['channel_fader'] == 0
            assert resumed['decks'][1]['is_master']
            assert all(resumed['decks'][1][name] == value for name, value in
                       [('eq_low', 1), ('eq_mid', 1), ('eq_high', 1), ('filter', 0),
                        ('echo', 0), ('reverb', 0)])
            report['cases']['armed_successor_starts_before_carrier_eof'] = resumed['decks'][0]['position_seconds'] < duration
            completed = wait_for(lambda: (s if s['state'] == 'completed' else None)
                                 if (s := call('mix.session.status')) else None)
            assert completed['reason'] == 'duration_reached' and completed['finish_unix_ms'] == finish
            assert all(not d['playing'] for d in transport()['decks'])
            report['cases']['original_endpoint_pauses_both_decks'] = True

            snapshot = prepare()
            call('mix.session.start', {'duration_seconds': 3})
            call('mix.session.successor', {'deck': 'B'})
            call('deck.load', {'deck': 1, 'demo': True})
            command('seek', deck=0, seconds=duration)
            time.sleep(.2)
            assert not transport()['decks'][1]['playing']
            assert call('mix.session.status')['successor'] is None
            assert call('mix.session.status')['state'] == 'running'
            options = call('mix.options', {'deck': 0})
            assert not options['options'], options
            assert options['considered_tracks'] > 0, options
            assert options['rejected_window_checks'], options
            assert all('unanalysed' not in reason for reason in options['reasons']), options
            report['cases']['stale_successor_not_replayed_and_empty_windows_explained'] = options['reasons']
            wait_for(lambda: call('mix.session.status')['state'] == 'completed')

            prepare()
            call('mix.session.start', {'duration_seconds': 8})
            call('mix.session.successor', {'deck': 'B'})
            command('pause', deck=0)
            stopped = call('mix.session.status')
            assert stopped['state'] == 'cancelled' and stopped['successor'] is None, stopped
            time.sleep(.2)
            assert all(not d['playing'] for d in transport()['decks'])
            report['cases']['explicit_stop_revokes_timer_and_successor'] = True
            assert all(value is not False for value in report['cases'].values()), report
            report['passed'] = True
        finally:
            if app.poll() is None:
                try:
                    call('app.stop')
                    app.wait(timeout=5)
                except (OSError, AssertionError, subprocess.TimeoutExpired):
                    app.terminate()
                    app.wait(timeout=5)
            log.close()
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + '\n')
            args.output.with_suffix('.app.log').write_text((root / 'app.log').read_text())
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
