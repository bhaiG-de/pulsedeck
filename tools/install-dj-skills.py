#!/usr/bin/env python3
"""Install the three DJ skills with rebased, self-contained shared references."""
import argparse
import os
from pathlib import Path
import re
import shutil
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / ".agents/skills"
SKILL_NAMES = ("pulsedeck-library", "djing-art", "pulsedeck-control")
LEGACY_NAMES = tuple("pulsedeck-" + suffix for suffix in (
    "track-inspection", "set-preparation", "prepare-next-track", "transition",
    "session-dj", "recording-and-export", "setup-and-troubleshooting",
))
LINK = re.compile(r"(\[[^\]]+\]\()([^)]+)(\))")


def install(destination):
    destination = Path(destination).expanduser().resolve()
    for name in SKILL_NAMES:
        if not (SOURCE / name / "SKILL.md").is_file():
            raise ValueError(f"Missing source skill {name}")
    # Check every collision before creating or writing any destination folder.
    for name in (*SKILL_NAMES, *LEGACY_NAMES):
        if (destination / name).exists() or (destination / name).is_symlink():
            raise ValueError(
                f"Refusing to overwrite existing skill {destination / name}; "
                "use a fresh destination or explicitly remove the old package first"
            )

    with tempfile.TemporaryDirectory(prefix="pulsedeck-skills-") as temporary:
        staged = Path(temporary).resolve()
        for name in SKILL_NAMES:
            target = staged / name
            shutil.copytree(SOURCE / name, target)
            pending = [(p, target / p.relative_to(SOURCE / name))
                       for p in (SOURCE / name).rglob("*.md")]
            seen = set()
            while pending:
                source_file, output_file = pending.pop()
                if source_file in seen:
                    continue
                seen.add(source_file)

                def rebase(match):
                    link = match[2]
                    if "://" in link or link.startswith("#"):
                        return match[0]
                    path, separator, anchor = link.partition("#")
                    resolved = (source_file.parent / path).resolve()
                    if not resolved.is_file() or not resolved.is_relative_to(SOURCE):
                        raise ValueError(f"Unpackaged reference in {source_file}: {link}")
                    shared = SOURCE / "references"
                    if resolved.is_relative_to(shared):
                        linked_output = target / "references" / resolved.relative_to(shared)
                        if resolved not in seen:
                            if linked_output.exists():
                                raise ValueError(f"Reference name collision: {linked_output}")
                            linked_output.parent.mkdir(parents=True, exist_ok=True)
                            pending.append((resolved, linked_output))
                    else:
                        linked_output = staged / resolved.relative_to(SOURCE)
                    relative = os.path.relpath(linked_output, output_file.parent)
                    suffix = separator + anchor if separator else ""
                    return match[1] + Path(relative).as_posix() + suffix + match[3]

                output_file.write_text(LINK.sub(rebase, source_file.read_text()))

        # Check closure before publishing: no installed link needs the checkout.
        for entry in staged.rglob("*.md"):
            for match in LINK.finditer(entry.read_text()):
                link = match[2]
                if "://" in link or link.startswith("#"):
                    continue
                resolved = (entry.parent / link.split("#", 1)[0]).resolve()
                if not resolved.is_relative_to(staged) or not resolved.is_file():
                    raise ValueError(f"Broken installed reference in {entry}: {link}")

        destination.mkdir(parents=True, exist_ok=True)
        published = []
        try:
            for name in SKILL_NAMES:
                target = destination / name
                target.mkdir()  # Refuse a concurrent collision rather than merge.
                published.append(target)
                shutil.copytree(staged / name, target, dirs_exist_ok=True)
        except OSError:
            for target in published:
                shutil.rmtree(target)
            raise
    return list(SKILL_NAMES)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", required=True, type=Path)
    args = parser.parse_args()
    try:
        names = install(args.destination)
    except (ValueError, OSError) as error:
        parser.exit(2, str(error) + "\n")
    print(f"Installed {len(names)} skills in {args.destination}; "
          "use pulsedeck-control/references/mcp.md for connected tool access")
