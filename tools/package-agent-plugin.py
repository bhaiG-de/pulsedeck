#!/usr/bin/env python3
"""Build the Codex/Claude PulseDeck plugin from the canonical DJ skills."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MARKETPLACE = "pulsedeck-local"
SERVER = "/Applications/PulseDeck.app/Contents/MacOS/pulsedeckctl"


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "tools" / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def build(destination):
    destination = Path(destination).resolve()
    if destination.exists():
        raise ValueError(f"Output already exists: {destination}; choose a fresh output directory")
    version = json.loads((ROOT / "package.json").read_text())["version"]
    identity = {"name": "pulsedeck", "version": version,
                "author": {"name": "PulseDeck"},
                "description": "DJ musical judgment, Spotify library evidence and full native deck performance through MCP."}
    server = {"type": "stdio", "command": "./scripts/pulsedeck-mcp", "args": []}
    claude_server = {**server, "command": "${CLAUDE_PLUGIN_ROOT}/scripts/pulsedeck-mcp"}
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="pd-plugin-", dir=destination.parent) as temporary:
        stage = Path(temporary) / "marketplace"
        plugin = stage / "plugins/pulsedeck"
        load("pd_skill_installer", "install-dj-skills.py").install(plugin / "skills")
        for cache in plugin.rglob("__pycache__"):
            shutil.rmtree(cache)
        load("pd_skill_validator", "validate-dj-skills.py").validate(plugin / "skills", self_contained=True)
        write_json(plugin / "plugin.json", {
            "$schema": "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
            **identity, "extensions": {"com.openai": {
                "requires_local_executor": True,
                "interface": {"displayName": "PulseDeck", "shortDescription": "DJ with musical judgment and full native deck control"}}}})
        write_json(plugin / "mcp.json", {
            "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
            "mcpServers": {"pulsedeck": server}})
        # Both hosts start this contained launcher; the app remains the engine owner.
        launcher = plugin / "scripts/pulsedeck-mcp"
        launcher.parent.mkdir()
        launcher.write_text(
            '#!/bin/sh\nset -eu\n'
            'plugin_dir=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)\n'
            'export PULSEDECK_AGENT_SKILLS_DIR="$plugin_dir/skills"\n'
            f'client="{SERVER}"\n'
            'if [ ! -x "$client" ]; then\n'
            '  echo "Install the current PulseDeck app in /Applications/PulseDeck.app, then reconnect the plugin." >&2\n'
            '  exit 1\nfi\nexec "$client" mcp --stdio\n')
        launcher.chmod(0o755)
        write_json(plugin / ".claude-plugin/plugin.json", identity)
        write_json(plugin / ".mcp.json", {"mcpServers": {"pulsedeck": claude_server}})
        write_json(stage / ".agents/plugins/marketplace.json", {
            "name": MARKETPLACE, "interface": {"displayName": "PulseDeck local development"},
            "plugins": [{"name": "pulsedeck", "source": {"source": "local", "path": "./plugins/pulsedeck"},
                         "policy": {"installation": "AVAILABLE", "authentication": "ON_USE"},
                         "category": "Productivity"}]})
        write_json(stage / ".claude-plugin/marketplace.json", {
            "name": MARKETPLACE, "owner": {"name": "PulseDeck"},
            "description": "Local development and installation of the PulseDeck DJ plugin.",
            "plugins": [{**identity, "source": "./plugins/pulsedeck"}]})
        (plugin / "README.md").write_text(
            "# PulseDeck agent plugin\n\n"
            "Requires macOS and the current PulseDeck app in /Applications/PulseDeck.app. "
            "Open PulseDeck and sign in to Spotify inside the app before DJing. "
            "The plugin launches the app's bundled MCP server; it contains no account data or audio.\n\n"
            "Connection instructions report the resolved installed art, waveform-reasoning, library and control file paths. "
            "Use those paths and relative references from the linking file rather than an old personal skill directory. "
            "Before committing a score, compare performance_validate's advisory transition forecast with the intended musical course.\n\n"
            "For a local developer installation, add this marketplace folder in Codex or Claude Code, "
            "then install pulsedeck@pulsedeck-local. Restart the agent session after installation. "
            "Use only one PulseDeck integration: remove a manually configured duplicate MCP server "
            "and duplicate personal DJ skills after verifying the plugin works.\n\n"
            "From the generated/extracted marketplace directory:\n\n"
            "```sh\ncodex plugin marketplace add .\ncodex plugin add pulsedeck@pulsedeck-local\n"
            "claude plugin marketplace add .\nclaude plugin install pulsedeck@pulsedeck-local\n```\n\n"
            "Try: Play a 10-minute playful café mix from my Spotify Liked Songs.\n\n"
            "Before performance, verify MCP performance_observe and deck_control are available. "
            "A missing tool means the connection/app needs updating; do not substitute shell deck commands. "
            "The art skill can also reason from supplied waveform evidence without playback.\n")
        hashes = {str(p.relative_to(plugin)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in sorted(plugin.rglob("*")) if p.is_file()}
        write_json(plugin / "package-evidence.json", {
            "version": version, "source": ".agents/skills", "server": server, "installed_app_client": SERVER,
            "protocol": "2025-11-25", "files": hashes})
        archive = stage / f"pulsedeck-{version}.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
            for path in sorted(plugin.rglob("*")):
                if path.is_file():
                    bundle.write(path, path.relative_to(plugin))
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        marketplace_archive = stage / f"pulsedeck-marketplace-{version}.zip"
        with zipfile.ZipFile(marketplace_archive, "w", zipfile.ZIP_DEFLATED) as bundle:
            for path in sorted(stage.rglob("*")):
                if path.is_file() and path.suffix != ".zip":
                    bundle.write(path, path.relative_to(stage))
        marketplace_digest = hashlib.sha256(marketplace_archive.read_bytes()).hexdigest()
        (stage / "SHA256SUMS.txt").write_text(
            f"{digest}  {archive.name}\n{marketplace_digest}  {marketplace_archive.name}\n")
        shutil.move(stage, destination)
    return {"marketplace": str(destination), "plugin": str(destination / "plugins/pulsedeck"),
            "zip": str(destination / archive.name),
            "marketplace_zip": str(destination / marketplace_archive.name),
            "version": version, "sha256": digest}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/agent-plugin-marketplace")
    print(json.dumps(build(parser.parse_args().output), indent=2))
