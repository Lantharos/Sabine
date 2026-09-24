#!/usr/bin/env python3
import argparse
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path

from release_versions import current_version, git, normalize_version, prepare_changes

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "Lantharos/Sabine"


def run(*args, capture=False):
    print("+ " + " ".join(args), flush=True)
    result = subprocess.run(args, cwd=ROOT, text=True, check=True, stdout=subprocess.PIPE if capture else None)
    return result.stdout.strip() if capture else None


def wait_workflow(workflow, revision):
    for _ in range(40):
        runs = json.loads(run("gh", "run", "list", "--repo", REPOSITORY, "--workflow", workflow,
                              "--event", "push", "--commit", revision, "--limit", "5", "--json", "databaseId", capture=True))
        if runs:
            run("gh", "run", "watch", str(runs[0]["databaseId"]), "--repo", REPOSITORY, "--exit-status", "--interval", "20")
            return
        time.sleep(3)
    raise ValueError(f"{workflow} has not appeared for {revision}; check GitHub Actions before retrying")


def check_sources():
    run("cargo", "fmt", "--all", "--check")
    run("cargo", "build", "--workspace", "--locked")
    run("cargo", "test", "--workspace", "--locked")
    run("cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings")
    if shutil.which("actionlint"):
        run("actionlint")


def main():
    parser = argparse.ArgumentParser(description="Prepare and publish a signed Sabine release. Defaults to the next build.")
    parser.add_argument("version", nargs="?")
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--dry-run", action="store_true", help="show version edits and publishing steps without changing files or remote state")
    modes.add_argument("--prepare", action="store_true", help="write version edits and run checks, without committing, tagging or pushing")
    args = parser.parse_args()
    current = current_version(ROOT)
    requested = args.version
    if requested is None and subprocess.run(
            ["git", "rev-parse", "--verify", f"refs/tags/v{current}"], cwd=ROOT, capture_output=True).returncode:
        requested = current
    version = normalize_version(requested, current)
    changes = prepare_changes(ROOT, version)
    tag = f"v{version}"
    print(f"Sabine {current_version(ROOT)} -> {version}")
    for name in changes:
        print(f"  update {name}")
    if args.dry_run:
        print(f"Would check sources, sign and push the release commit, wait for CI, sign and push {tag}, and wait for release artifacts.")
        return
    if git(ROOT, "branch", "--show-current") != "main":
        raise ValueError("release preparation requires the main branch")
    if git(ROOT, "status", "--porcelain"):
        raise ValueError("commit or discard existing changes before preparing a release")
    if not subprocess.run(["git", "rev-parse", "--verify", f"refs/tags/{tag}"], cwd=ROOT, capture_output=True).returncode:
        raise ValueError(f"{tag} already exists; inspect its release workflow instead of retagging")
    if not args.prepare:
        if not shutil.which("gh"):
            raise ValueError("GitHub CLI is required to validate and monitor publication")
        run("git", "fetch", "origin", "main", "--tags")
        if git(ROOT, "rev-parse", "HEAD") != git(ROOT, "rev-parse", "origin/main"):
            raise ValueError("local main must exactly match origin/main")
        if not subprocess.run(["git", "rev-parse", "--verify", f"refs/tags/{tag}"], cwd=ROOT, capture_output=True).returncode:
            raise ValueError(f"{tag} already exists on the remote")
        secrets = json.loads(run("gh", "secret", "list", "--repo", REPOSITORY, "--json", "name", capture=True))
        if not any(item["name"] == "SABINE_UPDATE_SIGNING_KEY" for item in secrets):
            raise ValueError("GitHub secret SABINE_UPDATE_SIGNING_KEY is not configured")
        immutable = json.loads(run("gh", "api", f"repos/{REPOSITORY}/immutable-releases", "-H", "X-GitHub-Api-Version: 2026-03-10", capture=True))
        if not immutable.get("enabled"):
            raise ValueError("immutable GitHub Releases must be enabled")
    for name, content in changes.items():
        (ROOT / name).write_text(content)
    check_sources()
    run("python3", "-B", "scripts/release_notes.py", tag, capture=True)
    run("git", "diff", "--check")
    if args.prepare:
        print(f"Prepared {tag}. Review and commit the changes; no tag or release was created.")
        return
    if changes:
        run("git", "add", "--", *changes)
        run("git", "commit", "-S", "-m", f"Prepare Sabine {version}")
        run("git", "push", "origin", "main")
    revision = git(ROOT, "rev-parse", "HEAD")
    wait_workflow("ci.yml", revision)
    run("git", "tag", "-s", tag, "-m", f"Sabine {version}")
    run("git", "push", "origin", tag)
    wait_workflow("release.yml", revision)
    print(f"Published {tag}; normal soak and promotion policy still applies.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, subprocess.CalledProcessError) as error:
        print(f"Release stopped: {error}", file=sys.stderr)
        sys.exit(1)
