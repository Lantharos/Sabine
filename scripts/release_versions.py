import json
import re
import subprocess
import tomllib
from pathlib import Path


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def current_version(root):
    version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    major, build, patch = map(int, version.split("."))
    if patch != 0:
        raise ValueError("Sabine package versions must use MAJOR.BUILD.0")
    return f"{major}.{build}"


def normalize_version(value, current):
    if value is None:
        major, build = map(int, current.split("."))
        return f"{major}.{build + 1}"
    match = re.fullmatch(r"v?(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:\.0)?", value)
    if not match:
        raise ValueError("version must be MAJOR.BUILD, optionally prefixed with v or ending in .0")
    version = f"{match[1]}.{match[2]}"
    if tuple(map(int, version.split("."))) < tuple(map(int, current.split("."))):
        raise ValueError(f"cannot release {version} over {current}")
    return version


HOST_PROTOCOL = re.compile(r'HOST_PROTOCOL_VERSION: &str = "([^"]+)"')
HOST_PROTOCOL_FILE = "crates/sabine-host/src/protocol.rs"


def host_protocol_changed(root):
    released = git(root, "describe", "--tags", "--abbrev=0", "--match", "v*")
    before = HOST_PROTOCOL.search(git(root, "show", f"{released}:{HOST_PROTOCOL_FILE}"))[1]
    after = HOST_PROTOCOL.search((root / HOST_PROTOCOL_FILE).read_text())[1]
    return before != after


def prepare_changes(root, version):
    current = current_version(root)
    old_package, package = f"{current}.0", f"{version}.0"
    changes = {}

    def add(name, text):
        if (root / name).read_text() != text:
            changes[name] = text

    members = set()
    for manifest in (root / "crates").glob("*/Cargo.toml"):
        metadata = tomllib.loads(manifest.read_text())["package"]
        if metadata.get("version") == {"workspace": True}:
            members.add(metadata["name"])
    lines = (root / "Cargo.toml").read_text().splitlines(keepends=True)
    section = ""
    for index, line in enumerate(lines):
        if line.startswith("["):
            section = line.strip()
        key = line.split("=", 1)[0].strip()
        if (section == "[workspace.package]" and key == "version") or (
                section == "[workspace.dependencies]" and key in members):
            lines[index] = line.replace(f'version = "{old_package}"', f'version = "{package}"')
    add("Cargo.toml", "".join(lines))
    lock = (root / "Cargo.lock").read_text()
    blocks = lock.split("[[package]]")
    for index, block in enumerate(blocks[1:], 1):
        metadata = tomllib.loads(block)
        if metadata["name"] in members and "source" not in metadata:
            blocks[index] = re.sub(r'^version = "[^"]+"', f'version = "{package}"', block, count=1, flags=re.M)
    add("Cargo.lock", "[[package]]".join(blocks))
    for name in ["package.json", "packages/sabine/package.json"]:
        text = (root / name).read_text()
        old = json.loads(text)["version"]
        add(name, text.replace(f'"version": "{old}"', f'"version": "{package}"', 1))
    major, build = version.split(".")
    name = "crates/sabine-service/src/types/mod.rs"
    text = (root / name).read_text()
    for constant, value in [("VERSION", f'"{version}"'), ("MAJOR", major), ("BUILD", build)]:
        text, count = re.subn(rf'(pub const SABINE_{constant}: [^=]+ = )[^;]+;', rf'\g<1>{value};', text)
        if count != 1:
            raise ValueError(f"expected one SABINE_{constant} constant")
    if host_protocol_changed(root):
        text, count = re.subn(r'(pub const MIN_SUPPORTED_APP_BUILD: u32 = )\d+;', rf'\g<1>{build};', text)
        if count != 1:
            raise ValueError("expected one MIN_SUPPORTED_APP_BUILD constant")
    add(name, text)
    for name in ["README.md", "packages/sabine/README.md", ".github/workflows/windows-runtime-check.yml"]:
        text = (root / name).read_text()
        text = re.sub(rf'(?<![\d.]){re.escape(old_package)}(?![\d.])', package, text)
        text = re.sub(rf'(?<![\d.]){re.escape(current)}(?![\d.])', version, text)
        add(name, text)
    name = "CHANGELOG.md"
    text = (root / name).read_text()
    if text.startswith("# Unreleased\n"):
        text = text.replace("# Unreleased\n", f"# Sabine {version}\n", 1)
    elif not text.startswith(f"# Sabine {version}\n"):
        tag = f"v{current}"
        if subprocess.run(["git", "-C", str(root), "rev-parse", "--verify", f"refs/tags/{tag}"], capture_output=True).returncode:
            raise ValueError("add release notes under '# Unreleased' in CHANGELOG.md")
        subjects = git(root, "log", "--format=%s", f"{tag}..HEAD").splitlines()
        if not subjects:
            raise ValueError("no changes to release")
        text = f"# Sabine {version}\n\n" + "".join(f"- {subject}\n" for subject in subjects) + "\n" + text
    add(name, text)
    return changes
