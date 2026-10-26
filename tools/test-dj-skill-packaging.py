#!/usr/bin/env python3
"""Exercise skill package relocation, collision protection and failure cleanup."""
import importlib.util
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load_module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "tools" / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


installer = load_module("installer", "install-dj-skills.py")
validator = load_module("validator", "validate-dj-skills.py")


class SkillPackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="pd-skill-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.destination = self.root / "installed"

    def test_install_relocates_with_all_links_inside_package(self):
        self.assertEqual(installer.install(self.destination), list(installer.SKILL_NAMES))
        relocated = self.root / "elsewhere" / "skills"
        relocated.parent.mkdir()
        shutil.move(self.destination, relocated)
        self.assertGreater(validator.validate(relocated, self_contained=True), 0)
        self.assertEqual(sorted(p.name for p in relocated.iterdir()), sorted(installer.SKILL_NAMES))
        for entry in relocated.rglob("*.md"):
            self.assertNotIn(str(ROOT), entry.read_text())

    def test_existing_skill_refused_before_any_new_writes(self):
        for name in (*installer.SKILL_NAMES, *installer.LEGACY_NAMES):
            with self.subTest(name=name):
                destination = self.root / name
                existing = destination / name
                existing.mkdir(parents=True)
                sentinel = existing / "keep.txt"
                sentinel.write_text("existing user content")
                with self.assertRaises(ValueError):
                    installer.install(destination)
                self.assertEqual(sentinel.read_text(), "existing user content")
                self.assertEqual(list(destination.iterdir()), [existing])

    def test_unrelated_skill_is_preserved(self):
        unrelated = self.destination / "unrelated" / "SKILL.md"
        unrelated.parent.mkdir(parents=True)
        unrelated.write_text("user skill")
        installer.install(self.destination)
        self.assertEqual(unrelated.read_text(), "user skill")

    def test_broken_source_link_leaves_destination_untouched(self):
        source = self.root / "source"
        shutil.copytree(installer.SOURCE, source)
        entry = source / "pulsedeck-library/SKILL.md"
        entry.write_text(entry.read_text() + "\n[Missing](references/missing.md)\n")
        with patch.object(installer, "SOURCE", source):
            with self.assertRaisesRegex(ValueError, "Unpackaged reference"):
                installer.install(self.destination)
        self.assertFalse(self.destination.exists())

    def test_shared_reference_collision_refused(self):
        source = self.root / "source"
        shutil.copytree(installer.SOURCE, source)
        (source / "pulsedeck-library/references/mcp.md").write_text("different instructions")
        with patch.object(installer, "SOURCE", source):
            with self.assertRaisesRegex(ValueError, "Reference name collision"):
                installer.install(self.destination)
        self.assertFalse(self.destination.exists())

    def test_publish_failure_cleans_owned_folders_and_preserves_unrelated(self):
        self.destination.mkdir()
        sentinel = self.destination / "keep.txt"
        sentinel.write_text("unrelated")
        original_copy = shutil.copytree

        def fail_second_publication(source, target, *args, **kwargs):
            target = Path(target)
            if target.parent == self.destination and target.name == installer.SKILL_NAMES[1]:
                raise OSError("simulated destination failure")
            return original_copy(source, target, *args, **kwargs)

        with patch.object(installer.shutil, "copytree", side_effect=fail_second_publication):
            with self.assertRaisesRegex(OSError, "simulated destination failure"):
                installer.install(self.destination)
        self.assertEqual(list(self.destination.iterdir()), [sentinel])
        self.assertEqual(sentinel.read_text(), "unrelated")

    def test_validator_rejects_a_fourth_entrypoint(self):
        installer.install(self.destination)
        extra = self.destination / "extra"
        extra.mkdir()
        (extra / "SKILL.md").write_text("extra skill")
        with self.assertRaisesRegex(AssertionError, "exactly three"):
            validator.validate(self.destination)


if __name__ == "__main__":
    unittest.main()
