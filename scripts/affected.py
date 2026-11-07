#!/usr/bin/env python3
"""Runs the tests a branch's changes can affect, in one of the two workspaces (P-154).

    python3 scripts/affected.py main [--base REF] [--dry-run] [-- <cargo test args>]
    python3 scripts/affected.py bevy [--base REF] [--dry-run] [-- <cargo test args>]

The changes are the branch's since it left REF (`origin/main` by default), with what isn't committed
yet. A change in a crate tests that crate and every crate that depends on it, in either
workspace. A change to data that tests read tests the crates that read it. A change to the
docs tests nothing. Anything else, such as a manifest, the lock file or the toolchain, tests
the whole workspace. `--all` tests the whole workspace whatever changed.

It prints what it runs and why, then runs `cargo test` on those packages in the workspace's
directory; with nothing affected it says so and runs nothing.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parent.parent
WORKSPACES = {"main": ROOT, "bevy": ROOT / "crates" / "bevy"}

# Data that tests read, and the packages whose tests read it, those alone.
READERS = [
    ("content/sample/builder.toml", ["factional-builder"]),
    ("docs/examples/", ["factional-content"]),
    ("scenarios/", ["factional-cli"]),
    ("schema/", ["factional-content", "factional-cli"]),
]

# The Bevy workspace's own configuration, which touches all of it and none of the main one.
BEVY_WIDE = {"Cargo.toml", "Cargo.lock", "clippy.toml", "rust-toolchain.toml"}

EVERYTHING = "everything"


def is_doc(path: str) -> bool:
    """Whether `path` is something no cargo test reads: prose, `.gitignore`, or these scripts,
    which have tests of their own."""
    pure = PurePosixPath(path)
    if path.startswith("docs/examples/"):
        return False
    return (
        path.startswith("docs/")
        or path.startswith("scripts/")
        or path == ".gitignore"
        or (len(pure.parts) == 1 and pure.suffix == ".md")
    )


def is_bevy_wide(path: str) -> bool:
    """Whether `path` is the Bevy workspace's own configuration."""
    prefix = "crates/bevy/"
    if not path.startswith(prefix):
        return False
    rest = path[len(prefix) :]
    return rest in BEVY_WIDE or rest.startswith(".cargo/")


def plan(files, packages, workspace):
    """What to test in `workspace` for the changed `files`.

    `packages` maps each package's name to its directory, from the repository's root, its
    workspace and the names of the packages it depends on, as {name: (dir, workspace, deps)}.
    Returns EVERYTHING, or the sorted names of the packages to test, with why for each file.
    """
    members = {name for name, (_, ws, _) in packages.items() if ws == workspace}
    seeds: set[str] = set()
    readers_only: set[str] = set()
    whole = False
    why = []
    by_dir = sorted(
        ((dir_, name) for name, (dir_, _, _) in packages.items()),
        key=lambda pair: -len(pair[0]),
    )
    for path in files:
        if is_doc(path):
            why.append(f"{path}: no cargo test reads it")
            continue
        readers = next((names for prefix, names in READERS if path.startswith(prefix)), None)
        if readers is not None:
            readers_only.update(readers)
            why.append(f"{path}: read by {', '.join(readers)}")
            continue
        if is_bevy_wide(path):
            why.append(f"{path}: the Bevy workspace's own configuration")
            if workspace == "bevy":
                whole = True
            continue
        owner = next(
            (name for dir_, name in by_dir if path.startswith(dir_ + "/")),
            None,
        )
        if owner is not None:
            seeds.add(owner)
            why.append(f"{path}: in {owner}")
            continue
        why.append(f"{path}: not in a crate, so everything")
        whole = True
    if whole:
        return EVERYTHING, why
    # Every package depending, at any remove, on one changed.
    affected = set(seeds)
    grew = True
    while grew:
        grew = False
        for name, (_, _, deps) in packages.items():
            if name not in affected and affected & set(deps):
                affected.add(name)
                grew = True
    return sorted((affected | readers_only) & members), why


def packages_of(workspace_dir: Path, workspace: str):
    """Each member of the workspace in `workspace_dir`: its directory and path dependencies."""
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=workspace_dir,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    found = {}
    for package in json.loads(out)["packages"]:
        dir_ = Path(package["manifest_path"]).parent.relative_to(ROOT).as_posix()
        deps = sorted({dep["name"] for dep in package["dependencies"] if dep.get("path")})
        found[package["name"]] = (dir_, workspace, deps)
    return found


def changed(base: str):
    """The files changed since the branch left `base`, and those not yet committed."""

    def lines(*args):
        out = subprocess.run(
            ["git", *args], cwd=ROOT, check=True, capture_output=True, text=True
        ).stdout
        return [line for line in out.splitlines() if line]

    files = set(lines("diff", "--name-only", f"{base}...HEAD"))
    files.update(lines("diff", "--name-only", "HEAD"))
    files.update(lines("ls-files", "--others", "--exclude-standard"))
    return sorted(files)


def main(argv):
    args, passed = (argv[: argv.index("--")], argv[argv.index("--") + 1 :]) if "--" in argv else (argv, [])
    if not args or args[0] not in WORKSPACES:
        print(__doc__, file=sys.stderr)
        return 2
    workspace = args[0]
    base = "origin/main"
    if "--base" in args:
        base = args[args.index("--base") + 1]
    dry = "--dry-run" in args
    if "--all" in args:
        chosen, why = EVERYTHING, ["--all"]
    else:
        packages = {}
        for name, dir_ in WORKSPACES.items():
            packages.update(packages_of(dir_, name))
        chosen, why = plan(changed(base), packages, workspace)
    for line in why:
        print(f"  {line}", file=sys.stderr)
    if chosen == EVERYTHING:
        command = ["cargo", "test", "--workspace", *passed]
    elif chosen:
        command = ["cargo", "test", *[f"-p={name}" for name in chosen], *passed]
    else:
        print(f"{workspace}: nothing affected, so no tests run", file=sys.stderr)
        return 0
    print(f"{workspace}: {' '.join(command)}", file=sys.stderr)
    if dry:
        return 0
    return subprocess.run(command, cwd=WORKSPACES[workspace], env=os.environ).returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
