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
    parser.add_argument('--output', type=Path, default=ROOT / 'target/release/bundle/dmg')
    args = parser.parse_args()
    app = args.app.resolve()
    # Embed the client and self-contained skills before the final app signature.
    cli = app / 'Contents/MacOS/pulsedeckctl'
    shutil.copy2(ROOT / 'target/release/pulsedeckctl', cli)
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
    for font in (ROOT / 'installer/resources').iterdir():
        shutil.copy2(font, resources / font.name)
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
        (stage / 'Read Me.txt').write_text(
            'Welcome to PulseDeck.\n\nDouble-click Install PulseDeck to open the custom installer.\n'
            'Use its Install button or drag its PulseDeck icon to the Applications target.\n'
            'Opening the disk image mounts it in Finder; it does not automatically run the installer.\n\n'
            'This build is ad-hoc signed, not Apple-notarized. macOS may require first-launch approval in Privacy & Security.\n'
            'Your music, library and accounts stay in their own app-private folders.\n')
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
            assert visible == {installer.name, 'Read Me.txt'}, f'Unexpected DMG entry points: {visible}'
            mounted_installer = mount / installer.name
            run(mounted_installer / 'Contents/MacOS/PulseDeckInstaller', '--self-test')
            run('codesign', '--verify', '--deep', '--strict', mounted_installer)
            run('python3', ROOT / 'tools/verify-release.py', mounted_installer / 'Contents/Resources/PulseDeck.app')
        finally:
            run('hdiutil', 'detach', mount)
        destination = args.output / f'PulseDeck_{version}_{arch}.dmg'
        shutil.copy2(image, destination)
    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    (args.output / 'SHA256SUMS.txt').write_text(f'{digest}  {destination.name}\n')
    print(json.dumps({'dmg': str(destination), 'sha256': digest, 'installer': str(installer), 'version': version}, indent=2))


if __name__ == '__main__':
    main()
