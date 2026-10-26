#!/usr/bin/env python3
"""Launch a copied release bundle outside the checkout and exercise its bundled CLI."""
import argparse
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', type=Path)
    parser.add_argument('--audio', action='store_true', help='Exercise muted real audio output')
    opts = parser.parse_args()
    source = opts.app.resolve(strict=True)
    # Tauri rejects executable paths containing symlinks on macOS (including /tmp).
    work = Path(tempfile.mkdtemp(prefix='pulsedeck-release-')).resolve()
    print(f'Release smoke artifacts: {work}', flush=True)
    installed = work / 'Applications/PulseDeck.app'
    shutil.copytree(source, installed, symlinks=True)
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(installed)], check=True)
    subprocess.run(['python3', str(ROOT / 'tools/verify-release.py'), str(installed)], check=True)
    binary = installed / 'Contents/MacOS/pulsedeck'
    cli = installed / 'Contents/MacOS/pulsedeckctl'
    socket = work / 'control.sock'
    profile = work / 'data'
    env = {key: value for key, value in os.environ.items() if not key.startswith('PULSEDECK_')}
    env.update(PULSEDECK_ISOLATED='1', PULSEDECK_DATA_DIR=str(profile), PULSEDECK_SOCKET=str(socket),
               CODEX_HOME=str(work / 'codex'), CLAUDE_CONFIG_DIR=str(work / 'claude'))
    process = None
    log = None

    def call(command, args=None):
        result = subprocess.run(
            [str(cli), 'call', command, '--args', json.dumps(args or {}), '--socket', str(socket), '--json'],
            cwd='/', capture_output=True, text=True, timeout=15,
        )
        response = json.loads(result.stdout)
        assert result.returncode == 0 and response['ok'], response
        return response['result']

    def launch(name):
        nonlocal process, log
        log = (work / f'{name}.log').open('w')
        process = subprocess.Popen(
            [str(binary), '-ApplePersistenceIgnoreState', 'YES'], cwd='/', env=env, stdout=log, stderr=log,
        )
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            assert process.poll() is None, f'App exited with {process.returncode}'
            try:
                if 'app' in call('ui.state'):
                    return
            except (AssertionError, ValueError, subprocess.TimeoutExpired):
                pass
            time.sleep(0.1)
        raise AssertionError('Installed app did not expose a working UI and control server')

    def stop():
        nonlocal process, log
        if process is not None:
            if process.poll() is None:
                try:
                    call('app.stop')
                    process.wait(timeout=10)
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait()
            assert process.returncode == 0, f'App exited with {process.returncode}'
            process = None
        if log is not None:
            log.close()
            log = None

    try:
        launch('clean')
        assert call('skills.welcome')['show']
        reports = call('skills.install', {'agents': ['codex', 'claude']})
        assert len(reports) == 2 and all(r['installed'] for r in reports), reports
        for agent in ('codex', 'claude'):
            for skill in ('pulsedeck-control', 'pulsedeck-library', 'djing-art'):
                assert (work / agent / 'skills' / skill / 'SKILL.md').is_file()
        customized = work / 'codex/skills/pulsedeck-control/SKILL.md'
        customized.write_text('customized skill')
        updated = call('skills.install', {'agents': ['codex']})[0]
        assert updated['installed'] and updated['backup'], updated
        assert (Path(updated['backup']) / 'pulsedeck-control/SKILL.md').read_text() == 'customized skill'
        assert customized.read_text() != 'customized skill'
        # Per-agent failure must preserve a successful install for the other agent.
        shutil.rmtree(work / 'claude/skills')
        (work / 'claude/skills').write_text('blocked destination')
        partial = call('skills.install', {'agents': ['codex', 'claude']})
        assert partial[0]['installed'] and not partial[1]['installed'] and partial[1]['error'], partial
        call('ui.call', {'target': 'app', 'action': 'agent-skills', 'value': False})
        deadline = time.monotonic() + 5
        while call('skills.welcome')['show'] and time.monotonic() < deadline:
            time.sleep(0.05)
        assert not call('skills.welcome')['show']
        call('ui.call', {'target': 'app', 'action': 'settings', 'value': True})
        call('ui.call', {'target': 'app', 'action': 'settings-tab', 'value': 'skills'})
        call('ui.call', {'target': 'app', 'action': 'settings', 'value': False})
        print('PASS first-launch setup, both agents, backups, partial failure and Settings tab', flush=True)
        youtube = call('ytmusic.status')
        assert not youtube['configured'] and not youtube['connected'], youtube
        call('ui.call', {'target': 'app', 'action': 'settings', 'value': True})
        call('ui.call', {'target': 'app', 'action': 'settings-tab', 'value': 'ytmusic'})
        call('ui.call', {'target': 'app', 'action': 'settings', 'value': False})
        call('ui.call', {'target': 'app', 'action': 'source', 'value': 'ytmusic'})
        call('ui.call', {'target': 'app', 'action': 'source', 'value': 'local'})
        print('PASS YouTube Music status, settings tab and library source controls', flush=True)
        first_instance = call('status')['instance']
        # Another binary copy must exit before it creates a profile or restores a journal.
        duplicate_profile = work / 'must-not-create'
        duplicate_env = {**env, 'PULSEDECK_DATA_DIR': str(duplicate_profile)}
        duplicate = subprocess.run(
            [str(source / 'Contents/MacOS/pulsedeck'), '-ApplePersistenceIgnoreState', 'YES'],
            cwd='/', env=duplicate_env, capture_output=True, text=True, timeout=15,
        )
        assert duplicate.returncode == 0, duplicate.stderr
        assert not duplicate_profile.exists(), 'Duplicate instance touched the app profile'
        assert call('status')['instance'] == first_instance
        print('PASS second app copy focuses the first instance without creating a profile', flush=True)
        smoke = ['python3', str(ROOT / 'tools/cli_smoke.py'), '--cli', str(cli), '--socket', str(socket), '--work-dir', str(work)]
        subprocess.run(smoke + (['--audio'] if opts.audio else []), cwd=ROOT, check=True, timeout=240)
        if opts.audio:
            call('audio.start', {'buffer_frames': 256})
            call('engine.command', {'type': 'play', 'deck': 'A'})
            recording = work / 'quit-recording.wav'
            call('recording.start', {'path': str(recording)})
            time.sleep(0.25)
            # Send macOS Quit to this exact PID, exercising the native exit event.
            subprocess.run(['osascript', '-l', 'JavaScript', '-e',
                f'ObjC.import("AppKit"); $.NSRunningApplication.runningApplicationWithProcessIdentifier({process.pid}).terminate'],
                check=True, capture_output=True, timeout=10)
            process.wait(timeout=15)
            raw = recording.read_bytes()
            frames = struct.unpack_from('<I', raw, 40)[0]
            assert frames > 0 and len(raw) == 44 + frames, 'Native Quit did not finalize the WAV header'
            print('PASS native macOS Quit finalizes an active recording', flush=True)
        stop()
        launch('restart')
        assert not call('skills.welcome')['show'], 'Welcome repeated after dismissal'
        subprocess.run(smoke + ['--restart-check'], cwd=ROOT, check=True, timeout=90)
        stop()
        damaged = '{damaged Spotify settings'
        (profile / 'spotify.json').write_text(damaged)
        (profile / 'youtube-music.json').write_text(damaged)
        launch('damaged-settings')
        spotify = call('spotify.status')
        assert not spotify['configured'] and spotify['last_error'], spotify
        assert (profile / 'spotify.json').read_text() == damaged
        youtube = call('ytmusic.status')
        assert not youtube['configured'] and youtube['last_error'], youtube
        assert (profile / 'youtube-music.json').read_text() == damaged
        print('PASS damaged optional Spotify and YouTube settings preserve the files and leave local controls available', flush=True)
        stop()
    finally:
        try:
            stop()
        finally:
            for path in work.glob('*.log'):
                print(f'{path.name}:\n{path.read_text()[-6000:]}', flush=True)
    print('PASS copied release launch, single instance, library/UI/controls, restart and settings recovery', flush=True)


if __name__ == '__main__':
    main()
