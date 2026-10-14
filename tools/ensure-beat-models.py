#!/usr/bin/env python3
"""Install pinned, verified Beat This! models. Python is only an install tool.

Inference uses pure Rust and needs neither Python nor a network connection.
Downloads are verified before atomic replacement; existing valid files are kept.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def verify(path, asset):
    if not path.is_file() or path.stat().st_size != asset["bytes"]:
        return False
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest() == asset["sha256"]


def main():
    root = Path(__file__).resolve().parent.parent
    default = root / "src-tauri/resources/beat-models"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path,
                        default=Path(os.environ.get("PULSEDECK_BEAT_MODEL_DIR", default)))
    parser.add_argument("--check", action="store_true", help="verify only; do not download")
    args = parser.parse_args()
    manifest = json.loads((default / "manifest.json").read_text())
    args.directory.mkdir(parents=True, exist_ok=True)
    for asset in manifest["assets"]:
        destination = args.directory / asset["filename"]
        if verify(destination, asset):
            print(f"Verified {destination.name}: {asset['sha256']}")
            continue
        if args.check:
            raise SystemExit(f"Missing or invalid model: {destination}")
        # curl uses system CA configuration, supports redirects and retries, and
        # avoids depending on a Python distribution's extra certificate setup.
        fd, temporary = tempfile.mkstemp(prefix=".model-", dir=args.directory)
        os.close(fd)
        temporary = Path(temporary)
        try:
            print(f"Downloading {asset['filename']} ({asset['bytes']} bytes)...", flush=True)
            subprocess.run(["curl", "--fail", "--location", "--retry", "3",
                            "--silent", "--show-error", "--connect-timeout", "30",
                            "--max-time", "600",
                            "--proto", "=https", "--proto-redir", "=https",
                            "--output", str(temporary), asset["url"]], check=True)
            if not verify(temporary, asset):
                raise SystemExit(f"Size/SHA-256 verification failed: {asset['filename']}")
            temporary.replace(destination)
            print(f"Installed and verified {destination.name}")
        finally:
            temporary.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
