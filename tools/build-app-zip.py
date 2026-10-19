#!/usr/bin/env python3
"""Archive a verified, ready-to-run macOS app and check its extracted copy."""
import argparse
import hashlib
import json
import plistlib
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def run(*args):
    subprocess.run([str(arg) for arg in args], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    app = args.app.resolve(strict=True)
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    run('python3', ROOT / 'tools/verify-release.py', app)
    run('codesign', '--verify', '--deep', '--strict', app)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    # ditto preserves macOS bundle metadata, executable permissions and symlinks.
    with tempfile.TemporaryDirectory(prefix='pulsedeck-zip-') as temporary:
        temporary = Path(temporary)
        archive = temporary / 'PulseDeck.zip'
        run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', app, archive)
        extracted = temporary / 'extracted'
        run('ditto', '-x', '-k', archive, extracted)
        assert {path.name for path in extracted.iterdir()} == {'PulseDeck.app'}, 'ZIP must open to one ready-to-run app'
        restored = extracted / 'PulseDeck.app'
        run('codesign', '--verify', '--deep', '--strict', restored)
        run('python3', ROOT / 'tools/verify-release.py', restored)
        assert (restored / 'Contents/MacOS/pulsedeck').read_bytes() == (app / 'Contents/MacOS/pulsedeck').read_bytes()
        args.output.write_bytes(archive.read_bytes())
    print(json.dumps({'zip': str(args.output), 'version': info['CFBundleShortVersionString'], 'sha256': hashlib.sha256(args.output.read_bytes()).hexdigest()}, indent=2))


if __name__ == '__main__':
    main()
