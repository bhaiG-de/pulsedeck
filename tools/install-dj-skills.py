#!/usr/bin/env python3
"""Copy the seven repository DJ skills with self-contained shared references."""
import argparse
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]
NAMES = ["track-inspection", "set-preparation", "prepare-next-track", "transition", "session-dj", "recording-and-export", "setup-and-troubleshooting"]

def install(destination):
    source = ROOT / ".agents/skills"
    names = ["pulsedeck-" + n for n in NAMES]
    for name in names:
        if not (source / name / "SKILL.md").is_file():
            raise ValueError(f"Missing source skill {name}")
        if (destination / name).exists():
            raise ValueError(f"Refusing to overwrite existing skill {destination / name}")
    destination.mkdir(parents=True, exist_ok=True)
    for name in names:
        target = destination / name
        shutil.copytree(source / name, target)
        shutil.copytree(source / "references", target / "references")
        entry = target / "SKILL.md"
        entry.write_text(entry.read_text().replace("../references/", "references/"))
        cli = target / "references/cli.md"
        cli.write_text(cli.read_text().replace("../../../research/cli-guide.md", str(ROOT / "research/cli-guide.md")))
    return names

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", required=True, type=Path)
    args = parser.parse_args()
    try:
        names = install(args.destination.expanduser().resolve())
    except ValueError as error:
        parser.exit(2, str(error) + "\n")
    print(f"Installed {len(names)} skills in {args.destination}; bind the CLI/session as described in references/cli.md")
