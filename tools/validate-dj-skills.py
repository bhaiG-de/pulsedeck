#!/usr/bin/env python3
"""Check repository skill packaging, local links and shell example syntax."""
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / ".agents/skills"
NAMES = ["track-inspection", "set-preparation", "prepare-next-track", "transition", "session-dj", "recording-and-export", "setup-and-troubleshooting"]
examples = 0
for suffix in NAMES:
    name = "pulsedeck-" + suffix
    folder = BASE / name
    entry = folder / "SKILL.md"
    text = entry.read_text()
    match = re.match(r"---\n(.*?)\n---\n", text, re.S)
    assert match, f"Missing skill frontmatter: {entry}"
    assert re.search(r"^name: " + re.escape(name) + r"$", match[1], re.M), entry
    assert re.search(r"^description: .+", match[1], re.M), entry
    assert (folder / "agents/openai.yaml").is_file(), f"Missing discovery metadata: {folder}"
for entry in [*BASE.rglob("*.md")]:
    text = entry.read_text()
    for target in re.findall(r"\[[^\]]+\]\(([^)]+)\)", text):
        if "://" in target or target.startswith("#"):
            continue
        target = target.split("#", 1)[0]
        assert (entry.parent / target).exists(), f"Broken local link in {entry}: {target}"
    for block in re.findall(r"```(?:sh|bash)\n(.*?)\n```", text, re.S):
        proc = subprocess.run(["bash", "-n"], input=block, text=True, capture_output=True)
        assert proc.returncode == 0, f"Invalid shell example in {entry}: {proc.stderr}"
        examples += 1
print(f"Seven skills packaged; local references resolve; {examples} shell example blocks parse. Live trial results validate behavior separately.")
