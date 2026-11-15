#!/usr/bin/env python3
import json
import os
import re
import urllib.request
from pathlib import Path

INDEX_URL = "https://cef-builds.spotifycdn.com/index.json"
PLATFORMS = {
    ("Linux", "X64"): "linux64",
    ("Linux", "ARM64"): "linuxarm64",
    ("Windows", "X64"): "windows64",
    ("Windows", "ARM64"): "windowsarm64",
    ("macOS", "ARM64"): "macosarm64",
}
CHANNELS = {"stable": 0, None: 0, "beta": 1, "dev": 2, "canary": 2}

root = Path(__file__).resolve().parent.parent
runtime_source = (root / "crates/sabine-runtime/src/lib.rs").read_text()
minimum_major = int(re.search(r"pub const MIN_CEF_MAJOR: u32 = (\d+);", runtime_source)[1])
platform = PLATFORMS[os.environ["RUNNER_OS"], os.environ["RUNNER_ARCH"]]


def numbers(version):
    return [int(part) for part in re.split(r"[.+\-_]", version) if part.isascii() and part.isdigit()]


def major(version):
    head = re.split(r"[.+]", version)[0]
    return int(head) if head.isascii() and head.isdigit() else 0


with urllib.request.urlopen(INDEX_URL, timeout=60) as response:
    versions = json.load(response)[platform]["versions"]
candidates = [
    version for version in versions
    if major(version["cef_version"]) >= minimum_major
    and any(file["type"] == "minimal" for file in version["files"])
]
latest = max(candidates, key=lambda version: (-CHANNELS.get(version.get("channel"), 3), numbers(version["cef_version"])))
cef_version = latest["cef_version"]

if os.environ["RUNNER_OS"] == "Windows":
    data = Path(os.environ["LOCALAPPDATA"]) / "Sabine"
elif os.environ["RUNNER_OS"] == "macOS":
    data = Path.home() / "Library/Application Support/Sabine"
else:
    data = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share") / "sabine"

with open(os.environ["GITHUB_OUTPUT"], "a") as output:
    print(f"key=cef-{platform}-{cef_version}", file=output)
    print(f"directory={data / 'runtimes/cef' / f'{cef_version}-minimal'}", file=output)
