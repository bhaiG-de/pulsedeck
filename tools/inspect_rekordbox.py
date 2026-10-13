#!/usr/bin/env python3
"""Collect static REA evidence for one explicitly selected rekordbox app."""

import argparse
import hashlib
import json
import os
import plistlib
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


REA_VERSION = "6.4.0"


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def select_target(path):
    app = path.expanduser().resolve(strict=True)
    if not app.is_dir() or app.suffix.lower() != ".app":
        raise ValueError("Select the actual rekordbox.app bundle, not its parent folder.")
    info_path = app / "Contents" / "Info.plist"
    with info_path.open("rb") as source:
        info = plistlib.load(source)
    identity = " ".join(str(info.get(key, "")) for key in (
        "CFBundleName", "CFBundleDisplayName", "CFBundleIdentifier"))
    if "rekordbox" not in identity.lower():
        raise ValueError("The selected bundle does not identify itself as rekordbox.")
    executable_name = info.get("CFBundleExecutable")
    if not isinstance(executable_name, str) or Path(executable_name).name != executable_name:
        raise ValueError("Invalid CFBundleExecutable in the selected app.")
    executable = (app / "Contents" / "MacOS" / executable_name).resolve(strict=True)
    if not executable.is_relative_to(app) or not executable.is_file():
        raise ValueError("Main executable is missing or resolves outside the app bundle.")
    return app, info_path, executable, info


def runtime(node_arg):
    env = os.environ.copy()
    if node_arg:
        node = node_arg.expanduser().resolve(strict=True)
        if not node.is_file():
            raise ValueError("--node must point to an existing Node.js executable.")
        env["PATH"] = str(node.parent) + os.pathsep + env.get("PATH", "")
    else:
        node_path = shutil.which("node", path=env.get("PATH"))
        if not node_path:
            raise ValueError("Node.js is missing. Supply an existing compatible runtime with --node.")
        node = Path(node_path)
    version = subprocess.check_output([str(node), "--version"], env=env, text=True).strip()
    numbers = tuple(int(part) for part in version.lstrip("v").split("."))
    compatible = (numbers[0] == 22 and numbers >= (22, 19, 0)) or (
        numbers[0] == 24 and numbers >= (24, 11, 0)) or numbers[0] >= 26
    if not compatible:
        raise ValueError(f"Node {version} is incompatible; use an existing Node 22.19+, 24.11+, or 26+ runtime.")
    npx = shutil.which("npx", path=env.get("PATH"))
    if not npx:
        raise ValueError("npx is missing from the selected runtime PATH.")
    return env, [npx, "-y", f"rea-agents@{REA_VERSION}"], version


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path, help="Exact local path to rekordbox.app")
    parser.add_argument("--node", type=Path, help="Existing compatible Node executable")
    parser.add_argument("--output", type=Path, help="New evidence directory; existing directories are refused")
    parser.add_argument("--plan", action="store_true", help="Validate target and print commands without running REA or writing files")
    parser.add_argument("--timeout", type=int, default=600, help="Seconds allowed per REA operation")
    args = parser.parse_args()
    if args.timeout < 1:
        parser.error("--timeout must be positive")
    try:
        app, info_path, executable, info = select_target(args.app)
        env, launcher, node_version = runtime(args.node)
    except (ValueError, OSError, plistlib.InvalidFileException, subprocess.SubprocessError) as error:
        parser.error(str(error))

    operations = [
        ("artifact", ["inspect-artifact", str(app)]),
        ("plist", ["inspect-plist", str(app)]),
        ("main-macho", ["inspect-macho", str(executable)]),
        ("signature", ["inspect-signature", str(app)]),
        ("dylib-resolution", ["trace-dylib-resolution", str(app)]),
    ]
    commands = [(name, launcher + op + ["--format", "json", "--full-output"]) for name, op in operations]
    if args.plan:
        print(json.dumps({"app": str(app), "node": node_version,
                          "rea_version": REA_VERSION, "commands": dict(commands)}, indent=2))
        return 0

    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    output = (args.output or Path(__file__).resolve().parents[1] / "research" / "evidence" / f"rekordbox-{stamp}").expanduser().resolve()
    if output.is_relative_to(app):
        parser.error("Save evidence outside the inspected application bundle.")
    output.mkdir(parents=True, exist_ok=False)
    tracked = {"Info.plist": info_path, "main_executable": executable}
    manifest = {
        "captured_at_utc": datetime.now(timezone.utc).isoformat(),
        "app": str(app), "rea_version": REA_VERSION, "node_version": node_version,
        "bundle_identifier": info.get("CFBundleIdentifier"),
        "bundle_version": info.get("CFBundleVersion"),
        "short_version": info.get("CFBundleShortVersionString"),
        "initial_sha256": {name: digest(path) for name, path in tracked.items()},
        "operations": [],
        "limitations": ["Static inspection only; no application launch or device traffic.",
                        "No function decompilation, DSP behavior recovery, or latency measurement.",
                        "Main executable and plist are checked for change; this is not an atomic full-bundle snapshot."],
    }
    manifest_path = output / "collection-manifest.json"

    def save():
        manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")

    save()
    failed = False
    for name, command in commands:
        print(f"Collecting {name} evidence...", flush=True)
        stdout_path, stderr_path = output / f"{name}.json", output / f"{name}.stderr.log"
        record = {"name": name, "command": command, "stdout": stdout_path.name,
                  "stderr": stderr_path.name}
        try:
            with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
                result = subprocess.run(command, env=env, stdout=stdout, stderr=stderr,
                                        timeout=args.timeout, check=False)
            record["exit_code"] = result.returncode
            record["stdout_sha256"] = digest(stdout_path)
            try:
                with stdout_path.open() as source:
                    envelope = json.load(source)
                record["valid_json"] = True
                record["rea_ok"] = isinstance(envelope, dict) and envelope.get("ok") is True
                if record["rea_ok"] and isinstance(envelope.get("data"), dict):
                    record["evidence_id"] = envelope["data"].get("evidence_id")
            except (ValueError, UnicodeDecodeError):
                record["valid_json"] = False
            # Preserve a valid REA error envelope; an exit status alone does not prove success.
            record["requires_result_review"] = True
            failed = failed or result.returncode != 0 or not record["valid_json"] or not record.get("rea_ok", False)
        except (subprocess.TimeoutExpired, OSError) as error:
            record["error"] = str(error)
            failed = True
        manifest["operations"].append(record)
        save()
    manifest["final_sha256"] = {name: digest(path) for name, path in tracked.items()}
    manifest["tracked_files_unchanged"] = manifest["initial_sha256"] == manifest["final_sha256"]
    failed = failed or not manifest["tracked_files_unchanged"]
    save()
    print(f"Evidence saved to {output}. Review each REA result and its limitations.")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
