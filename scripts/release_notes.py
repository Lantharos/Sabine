#!/usr/bin/env python3
"""Print one version's notes from the cumulative changelog."""

import sys
from pathlib import Path


def main() -> None:
    version = sys.argv[1].removeprefix("v")
    lines = (Path(__file__).resolve().parent.parent / "CHANGELOG.md").read_text().splitlines()
    heading = f"# Sabine {version}"
    try:
        start = lines.index(heading)
    except ValueError as error:
        raise SystemExit(f"No changelog section for {version}") from error
    end = next((index for index in range(start + 1, len(lines)) if lines[index].startswith("# ")), len(lines))
    notes = "\n".join(lines[start:end]).strip()
    if notes == heading:
        raise SystemExit(f"Empty changelog section for {version}")
    print(notes)


if __name__ == "__main__":
    main()
