#!/usr/bin/env python3
import json
import os
from pathlib import Path

PLATFORMS = {
    ("Linux", "X64"): "linux64",
    ("Linux", "ARM64"): "linuxarm64",
    ("Windows", "X64"): "windows64",
    ("Windows", "ARM64"): "windowsarm64",
    ("macOS", "ARM64"): "macosarm64",
}

root = Path(__file__).resolve().parent.parent
pin = json.loads((root / "crates/sabine-runtime/src/download/cef.json").read_text())
cef_version = pin["version"]
platform = PLATFORMS[os.environ["RUNNER_OS"], os.environ["RUNNER_ARCH"]]

if os.environ["RUNNER_OS"] == "Windows":
    data = Path(os.environ["LOCALAPPDATA"]) / "Sabine"
elif os.environ["RUNNER_OS"] == "macOS":
    data = Path.home() / "Library/Application Support/Sabine"
else:
    data = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share") / "sabine"

with open(os.environ["GITHUB_OUTPUT"], "a") as output:
    print(f"key=cef-{platform}-{cef_version}", file=output)
    print(f"directory={data / 'runtimes/cef' / f'{cef_version}-minimal'}", file=output)
