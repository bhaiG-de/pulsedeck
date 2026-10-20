#!/usr/bin/env python3
"""Package the self-contained macOS app as a ready-to-run ZIP."""
import argparse
import hashlib
import json
import platform
import plistlib
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(*args, **kwargs):
    subprocess.run([str(arg) for arg in args], check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', type=Path, default=ROOT / 'target/release/bundle/macos/PulseDeck.app')
    parser.add_argument('--cli', type=Path, default=ROOT / 'target/release/pulsedeckctl')
    parser.add_argument('--output', type=Path, default=ROOT / 'target/release/bundle/zip')
    args = parser.parse_args()
    app = args.app.resolve()
    # Embed the client and self-contained skills before the final app signature.
    cli = app / 'Contents/MacOS/pulsedeckctl'
    shutil.copy2(args.cli, cli)
    skills = app / 'Contents/Resources/dj-skills'
    if skills.exists():
        shutil.rmtree(skills)
    run('python3', ROOT / 'tools/install-dj-skills.py', '--destination', skills)
    run('codesign', '--force', '--sign', '-', '--timestamp=none', cli)
    run('codesign', '--force', '--sign', '-', '--timestamp=none', app)
    run('python3', ROOT / 'tools/verify-release.py', app)
    run('codesign', '--verify', '--deep', '--strict', app)
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    version = info['CFBundleShortVersionString']
    arch = {'arm64': 'aarch64', 'x86_64': 'x64'}[platform.machine()]
    args.output.mkdir(parents=True, exist_ok=True)
    app_zip = args.output / f'PulseDeck_{version}_{arch}.zip'
    run('python3', ROOT / 'tools/build-app-zip.py', '--app', app, '--output', app_zip)
    digest = hashlib.sha256(app_zip.read_bytes()).hexdigest()
    (args.output / 'SHA256SUMS.txt').write_text(f'{digest}  {app_zip.name}\n')
    print(json.dumps({'zip': str(app_zip), 'sha256': digest, 'version': version}, indent=2))


if __name__ == '__main__':
    main()
