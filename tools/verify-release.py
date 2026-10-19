#!/usr/bin/env python3
"""Verify a distributable app's identity/assets and absence of runtime profiles."""
import argparse
import hashlib
import json
import plistlib
from pathlib import Path


def verify(app):
    app = Path(app)
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    assert info['CFBundleIdentifier'] == 'app.pulsedeck.desktop'
    assert info['CFBundleName'] == 'PulseDeck'
    icon = app / 'Contents/Resources' / info['CFBundleIconFile']
    assert icon.is_file() and icon.stat().st_size > 1000, 'Missing bundled icon'
    resources = app / 'Contents/Resources'
    cli = app / 'Contents/MacOS/pulsedeckctl'
    assert cli.is_file() and cli.stat().st_mode & 0o111, 'Missing executable control client'
    skills = ('pulsedeck-control', 'pulsedeck-library', 'djing-art')
    for name in skills:
        assert (resources / 'dj-skills' / name / 'SKILL.md').is_file(), f'Missing skill: {name}'
    manifests = list(resources.rglob('manifest.json'))
    manifest = next(p for p in manifests if p.parent.name == 'beat-models')
    data = json.loads(manifest.read_text())
    assets = data.get('assets', data.get('models', []))
    if isinstance(assets, dict):
        assets = list(assets.values())
    for item in assets:
        filename = item.get('filename', item.get('name', item.get('file')))
        asset = manifest.parent / filename
        assert asset.stat().st_size == item['bytes'], f'Asset size: {filename}'
        assert hashlib.sha256(asset.read_bytes()).hexdigest() == item['sha256'], f'Asset hash: {filename}'
    banned = {'spotify.json', 'spotify-native.json', 'library.sqlite', 'control-journal.json', 'audio.json', '.env'}
    assert not any(p.name in banned or p.suffix in {'.db', '.sqlite', '.sock', '.pem', '.key'} for p in app.rglob('*')), 'Runtime profile or credentials in app'
    binary = app / 'Contents/MacOS' / info['CFBundleExecutable']
    assert binary.is_file()
    return {'version': info['CFBundleShortVersionString'], 'icon': icon.name, 'models': len(assets), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'runtime_profiles': False, 'skills': list(skills), 'cli': cli.name}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', type=Path)
    print(json.dumps(verify(parser.parse_args().app), indent=2))
