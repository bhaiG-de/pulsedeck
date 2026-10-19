#!/usr/bin/env python3
"""Package PulseDeck plus its animated native installer, without Finder automation."""
import argparse
import hashlib
import json
import os
import platform
import plistlib
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(*args, **kwargs):
    subprocess.run([str(arg) for arg in args], check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', type=Path, default=ROOT / 'target/release/bundle/macos/PulseDeck.app')
    parser.add_argument('--cli', type=Path, default=ROOT / 'target/release/pulsedeckctl')
    parser.add_argument('--output', type=Path, default=ROOT / 'target/release/bundle/dmg')
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
    installer = app.parent.parent / 'installer/Install PulseDeck.app'
    if installer.exists():
        shutil.rmtree(installer)
    executable = installer / 'Contents/MacOS/PulseDeckInstaller'
    resources = installer / 'Contents/Resources'
    executable.parent.mkdir(parents=True)
    resources.mkdir(parents=True)
    shutil.copy2(ROOT / 'src-tauri/icons/icon.icns', resources / 'icon.icns')
    shutil.copy2(ROOT / 'src-tauri/icons/icon.png', resources / 'AppIcon.png')
    for resource in (ROOT / 'installer/resources').iterdir():
        shutil.copy2(resource, resources / resource.name)
    # Keep the payload inside the installer: Gatekeeper may translocate this bundle.
    shutil.copytree(app, resources / app.name, symlinks=True)
    (installer / 'Contents/Info.plist').write_bytes(plistlib.dumps({
        'CFBundleName': 'Install PulseDeck', 'CFBundleDisplayName': 'Install PulseDeck',
        'CFBundleIdentifier': 'app.pulsedeck.installer', 'CFBundleExecutable': 'PulseDeckInstaller',
        'CFBundleIconFile': 'icon.icns', 'CFBundlePackageType': 'APPL',
        'CFBundleShortVersionString': version, 'CFBundleVersion': version,
        'LSMinimumSystemVersion': '12.0', 'NSHighResolutionCapable': True,
        'NSPrincipalClass': 'NSApplication', 'LSApplicationCategoryType': 'public.app-category.music'
    }))
    run('xcrun', 'swiftc', ROOT / 'installer/main.swift', '-O', '-target', platform.machine() + '-apple-macosx12.0', '-o', executable)
    run(executable, '--self-test')
    run('codesign', '--force', '--sign', '-', '--timestamp=none', installer)
    run('codesign', '--verify', '--deep', '--strict', installer)
    with tempfile.TemporaryDirectory(prefix='pulsedeck-dmg-') as temp:
        temp = Path(temp)
        stage = temp / 'contents'
        stage.mkdir()
        shutil.copytree(installer, stage / installer.name, symlinks=True)
        # Help must be readable even when Gatekeeper blocks the installer itself.
        shutil.copy2(resources / 'First Launch.html', stage / 'START HERE.html')
        settings_url = 'x-apple.systempreferences:com.apple.preference.security'
        (stage / 'Open Privacy & Security.inetloc').write_bytes(plistlib.dumps({'URL': settings_url}))
        shutil.copy2(ROOT / 'src-tauri/icons/icon.icns', stage / '.VolumeIcon.icns')
        image = temp / 'PulseDeck.dmg'
        # A single create avoids create-dmg's mount/resize/convert race on recent macOS.
        for attempt in range(3):
            result = subprocess.run(['hdiutil', 'create', '-volname', 'PulseDeck', '-srcfolder', str(stage), '-format', 'UDZO', '-imagekey', 'zlib-level=6', '-fs', 'HFS+', '-ov', '-o', str(image)])
            if result.returncode == 0:
                break
            if attempt == 2:
                raise SystemExit(result.returncode)
            time.sleep(2)
        run('hdiutil', 'verify', image)
        mount = temp / 'mounted'
        run('hdiutil', 'attach', '-readonly', '-nobrowse', '-mountpoint', mount, image)
        try:
            visible = {path.name for path in mount.iterdir() if not path.name.startswith('.')}
            assert visible == {installer.name, 'START HERE.html', 'Open Privacy & Security.inetloc'}, f'Unexpected DMG entry points: {visible}'
            mounted_installer = mount / installer.name
            assert (mount / 'START HERE.html').read_bytes() == (mounted_installer / 'Contents/Resources/First Launch.html').read_bytes()
            assert plistlib.loads((mount / 'Open Privacy & Security.inetloc').read_bytes())['URL'] == settings_url
            run(mounted_installer / 'Contents/MacOS/PulseDeckInstaller', '--self-test')
            run('codesign', '--verify', '--deep', '--strict', mounted_installer)
            run('python3', ROOT / 'tools/verify-release.py', mounted_installer / 'Contents/Resources/PulseDeck.app')
        finally:
            run('hdiutil', 'detach', mount)
        destination = args.output / f'PulseDeck_{version}_{arch}.dmg'
        shutil.copy2(image, destination)
    app_zip = args.output / f'PulseDeck_{version}_{arch}.zip'
    run('python3', ROOT / 'tools/build-app-zip.py', '--app', app, '--output', app_zip)
    checksums = {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in (destination, app_zip)}
    (args.output / 'SHA256SUMS.txt').write_text(''.join(f'{digest}  {name}\n' for name, digest in checksums.items()))
    print(json.dumps({'dmg': str(destination), 'zip': str(app_zip), 'checksums': checksums, 'installer': str(installer), 'version': version}, indent=2))


if __name__ == '__main__':
    main()
