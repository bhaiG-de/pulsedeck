#!/usr/bin/env python3
"""Check portable packaging, host adapters, relocation and overwrite protection."""
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("pd_plugin", ROOT / "tools/package-agent-plugin.py")
plugin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plugin)


class PluginTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.output = Path(self.temporary.name) / "distribution"
        self.result = plugin.build(self.output)
        self.package = Path(self.result["plugin"])

    def test_host_identity_and_server_match(self):
        manifests = [json.loads((self.package / p).read_text()) for p in (
            "plugin.json", ".claude-plugin/plugin.json")]
        self.assertEqual({m["name"] for m in manifests}, {"pulsedeck"})
        self.assertEqual({m["version"] for m in manifests}, {self.result["version"]})
        portable = json.loads((self.package / "mcp.json").read_text())
        compatibility = json.loads((self.package / ".mcp.json").read_text())
        codex_server = portable["mcpServers"]["pulsedeck"]
        claude_server = compatibility["mcpServers"]["pulsedeck"]
        self.assertEqual(codex_server["command"], "./scripts/pulsedeck-mcp")
        self.assertEqual(claude_server["command"], "${CLAUDE_PLUGIN_ROOT}/scripts/pulsedeck-mcp")
        self.assertEqual(codex_server["args"], claude_server["args"])
        self.assertIn(plugin.SERVER, (self.package / "scripts/pulsedeck-mcp").read_text())
        self.assertTrue((self.package / "scripts/pulsedeck-mcp").stat().st_mode & 0o111)
        readme = (self.package / "README.md").read_text()
        self.assertIn("claude plugin marketplace add .\nclaude plugin install pulsedeck@pulsedeck-local", readme)

    def test_relocated_skills_are_self_contained(self):
        relocated = Path(self.temporary.name) / "elsewhere"
        shutil.move(self.package, relocated)
        validator = plugin.load("pd_validator_test", "validate-dj-skills.py")
        self.assertGreater(validator.validate(relocated / "skills", self_contained=True), 0)
        self.assertEqual(len(list((relocated / "skills").glob("*/SKILL.md"))), 3)

    def test_launcher_resolves_skill_root_after_relocation(self):
        relocated = Path(self.temporary.name) / "relocated plugin"
        shutil.move(self.package, relocated)
        fake_client = Path(self.temporary.name) / "fixture client"
        fake_client.write_text('#!/bin/sh\nprintf \'%s\\n\' "$PULSEDECK_AGENT_SKILLS_DIR" "$@"\n')
        fake_client.chmod(0o755)
        launcher = relocated / "scripts/pulsedeck-mcp"
        launcher.write_text(launcher.read_text().replace(plugin.SERVER, str(fake_client)))
        output = subprocess.check_output([str(launcher)], cwd=self.temporary.name, text=True).splitlines()
        self.assertEqual(output, [str((relocated / "skills").resolve()), "mcp", "--stdio"])

    def test_marketplaces_resolve_same_package(self):
        codex = json.loads((self.output / ".agents/plugins/marketplace.json").read_text())
        claude = json.loads((self.output / ".claude-plugin/marketplace.json").read_text())
        self.assertEqual(codex["name"], claude["name"])
        self.assertEqual((self.output / codex["plugins"][0]["source"]["path"]).resolve(), self.package)
        self.assertEqual((self.output / claude["plugins"][0]["source"]).resolve(), self.package)

    def test_archive_contains_adapters_and_verified_contents(self):
        archive = Path(self.result["zip"])
        self.assertEqual(hashlib.sha256(archive.read_bytes()).hexdigest(), self.result["sha256"])
        with zipfile.ZipFile(archive) as bundle:
            names = bundle.namelist()
            self.assertIn(".claude-plugin/plugin.json", names)
            self.assertIn("mcp.json", names)
            self.assertTrue(bundle.getinfo("scripts/pulsedeck-mcp").external_attr >> 16 & 0o111)
            self.assertFalse(any(n.startswith("/") or ".." in Path(n).parts for n in names))
            self.assertFalse(any(Path(n).suffix in (".db", ".sqlite", ".sock", ".pem", ".key") for n in names))
            self.assertFalse(any("__pycache__" in Path(n).parts for n in names))
        with zipfile.ZipFile(self.result["marketplace_zip"]) as bundle:
            self.assertIn(".agents/plugins/marketplace.json", bundle.namelist())
            self.assertIn(".claude-plugin/marketplace.json", bundle.namelist())
            self.assertIn("plugins/pulsedeck/.mcp.json", bundle.namelist())
        evidence = json.loads((self.package / "package-evidence.json").read_text())
        for name, digest in evidence["files"].items():
            self.assertEqual(hashlib.sha256((self.package / name).read_bytes()).hexdigest(), digest)

    def test_existing_output_is_never_overwritten(self):
        sentinel = self.output / "user-file"
        sentinel.write_text("keep")
        with self.assertRaisesRegex(ValueError, "Output already exists"):
            plugin.build(self.output)
        self.assertEqual(sentinel.read_text(), "keep")


if __name__ == "__main__":
    unittest.main()
