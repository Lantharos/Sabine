#!/usr/bin/env python3
"""Pins the CEF build Sabine installs, by default the newest stable one.

Downloads every platform's Minimal archive to record its size and SHA-256,
after checking it against the SHA-1 the CEF build index lists for it.
"""

import argparse
import hashlib
import json
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PIN = ROOT / "crates/sabine-runtime/src/download/cef.json"
BUILDS = "https://cef-builds.spotifycdn.com"
PLATFORMS = ["linux64", "linuxarm64", "windows64", "windowsarm64", "macosarm64"]


def version_key(version):
    return [int(part) for part in version.split("+")[0].split(".")]


def minimal_archives(index, version):
    archives = {}
    for platform in PLATFORMS:
        builds = [build for build in index[platform]["versions"] if build["cef_version"] == version]
        files = [file for build in builds for file in build["files"] if file["type"] == "minimal"]
        if len(files) != 1:
            return None
        archives[platform] = files[0]
    return archives


def newest_stable(index):
    versions = {
        build["cef_version"]
        for build in index[PLATFORMS[0]]["versions"]
        if build.get("channel", "stable") == "stable"
    }
    for version in sorted(versions, key=version_key, reverse=True):
        if minimal_archives(index, version):
            return version
    raise ValueError("no stable CEF build has a Minimal archive for every platform")


def hash_archive(file):
    sha1, sha256, size = hashlib.sha1(), hashlib.sha256(), 0
    with urllib.request.urlopen(f"{BUILDS}/{urllib.request.quote(file['name'])}") as response:
        while chunk := response.read(1024 * 1024):
            sha1.update(chunk)
            sha256.update(chunk)
            size += len(chunk)
    if size != file["size"] or sha1.hexdigest() != file["sha1"]:
        raise ValueError(f"{file['name']} does not match the CEF build index")
    return {"name": file["name"], "size": size, "sha256": sha256.hexdigest()}


def main():
    parser = argparse.ArgumentParser(description="Pin the CEF build Sabine installs. Defaults to the newest stable build.")
    parser.add_argument("version", nargs="?")
    args = parser.parse_args()
    with urllib.request.urlopen(f"{BUILDS}/index.json") as response:
        index = json.load(response)
    version = args.version or newest_stable(index)
    files = minimal_archives(index, version)
    if not files:
        raise ValueError(f"CEF {version} has no Minimal archive for every platform")
    archives = {}
    for platform, file in files.items():
        print(f"Hashing {file['name']}", flush=True)
        archives[platform] = hash_archive(file)
    PIN.write_text(json.dumps({"version": version, "archives": archives}, indent=2) + "\n")
    print(f"Pinned CEF {version}")


if __name__ == "__main__":
    try:
        main()
    except ValueError as error:
        print(f"Pinning stopped: {error}", file=sys.stderr)
        sys.exit(1)
