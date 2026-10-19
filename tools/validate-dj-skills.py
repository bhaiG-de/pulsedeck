#!/usr/bin/env python3
"""Validate three-skill packaging, local references and example syntax."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("dj_skill_installer", ROOT / "tools/install-dj-skills.py")
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)
SKILL_NAMES = installer.SKILL_NAMES


def validate(base, self_contained=False):
    base = Path(base).resolve()
    entries = list(base.rglob("SKILL.md"))
    expected = {base / name / "SKILL.md" for name in SKILL_NAMES}
    assert set(entries) == expected, f"Expected exactly three skill entrypoints: {entries}"
    for name in SKILL_NAMES:
        folder = base / name
        entry = folder / "SKILL.md"
        text = entry.read_text()
        match = re.match(r"---\n(.*?)\n---\n", text, re.S)
        assert match, f"Missing skill frontmatter: {entry}"
        assert re.search(r"^name: " + re.escape(name) + r"$", match[1], re.M), entry
        assert re.search(r"^description: .+", match[1], re.M), entry
        metadata = (folder / "agents/openai.yaml").read_text()
        assert f"${name}" in metadata, f"Missing invocation metadata: {folder}"
    examples = 0
    for entry in base.rglob("*.md"):
        text = entry.read_text()
        assert "[TODO:" not in text, f"Unfinished scaffold: {entry}"
        for target in re.findall(r"\[[^\]]+\]\(([^)]+)\)", text):
            if "://" in target or target.startswith("#"):
                continue
            resolved = (entry.parent / target.split("#", 1)[0]).resolve()
            assert resolved.exists(), f"Broken local link in {entry}: {target}"
            if self_contained:
                assert resolved.is_relative_to(base), f"Checkout dependency: {entry}: {target}"
        for language, block in re.findall(r"```(sh|bash|json|jsonl)\n(.*?)\n```", text, re.S):
            if language in ("sh", "bash"):
                proc = subprocess.run(["bash", "-n"], input=block, text=True, capture_output=True)
                assert proc.returncode == 0, f"Invalid shell example in {entry}: {proc.stderr}"
            elif language == "jsonl":
                for line in block.splitlines():
                    if line.strip():
                        json.loads(line)
            else:
                json.loads(block)
            examples += 1
    return examples


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT / ".agents/skills")
    parser.add_argument("--self-contained", action="store_true")
    args = parser.parse_args()
    count = validate(args.root, args.self_contained)
    print(f"Three skills packaged; local references resolve; {count} example blocks parse. "
          "Musical behavior and live execution require separate validation.")
