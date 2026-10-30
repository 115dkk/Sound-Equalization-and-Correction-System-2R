#!/usr/bin/env python3
"""Release gate of release.yml: decides whether a push to main publishes,
and with which version (docs/adr/0006).

Versions are plain semver tags ``v<major>.<minor>.<patch>``. While the major
version is 0 every release is a GitHub prerelease; 0.x itself is what says
"prerelease", so no suffix is ever appended. 1.0.0, the first official
release, is declared by hand.

Decision policy:

* the workspace version has no tag yet             -> release it as is
                                                      (first release, or a
                                                      manual bump)
* tagged, shipped files changed since that tag     -> bump, commit the bump to
                                                      main, release it
* tagged, nothing shipped changed                  -> do nothing
  (``--force`` from a manual dispatch bumps anyway)
* the triggering commit is no longer main's head    -> do nothing; the run of
                                                      the newer push releases
                                                      everything shipped since
                                                      the last tag, this commit
                                                      included

The last rule makes "Re-run all jobs" of an old run harmless (it never mints a
second version for the same change). A failed release is retried with
"Re-run failed jobs", which keeps the gate's decision. If main moves while the
gate pushes its bump, the push is rejected and the gate releases nothing for
the same reason, instead of rebasing later commits into this release.

The crates.io crates (docs/adr/0016) and the ``secs2r`` Python package
(docs/adr/0019) carry a shared version, read from ``crates/secs-api/Cargo.toml``
and tagged ``api-v<version>`` once published. When the app releases, the same
rules apply to them: an untagged version is published as is, and packaged files
changed since the tag bump every published Rust crate and ``secs-py`` together
(and their exact ``=`` requirements and lock entries) in the same bump commit.
release.yml publishes them after the installers built.

The bump is minor when a commit that touched shipped files has a ``feat``
subject (``feat:``, ``feat(scope):``, ``feat!:``) or says ``BREAKING CHANGE``,
and patch otherwise. Before 1.0 a breaking change is still a minor bump.

A file ships when it is a build input of the released app: ``SHIP`` patterns
minus ``NOT_SHIPPED`` and anything under a ``tests``, ``benches``, ``fuzz`` or
``node_modules`` directory, plus ``SHIP_ALWAYS``. The upstream Python tree
(``src/``, ``tests/``, ``tools/``) is the oracle and never ships.

The pure functions are unit-tested in ``test_release_gate.py``; ``main`` wires
them to git and ``$GITHUB_OUTPUT``.
"""
from __future__ import annotations

import argparse
import fnmatch
import os
import re
import subprocess
import sys
from pathlib import Path

SHIP = (
    "crates/*",
    "apps/*",
    "bindings/python/*",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "LICENSE",
    # Compiled into every build (secs-service reports it).
    "UPSTREAM.toml",
)
# Shipped although they match NOT_SHIPPED.
SHIP_ALWAYS: tuple[str, ...] = ()
NOT_SHIPPED = (
    "*.md",
    "crates/secs-policy/*",
    "crates/secs-oracle/*",
    "*.test.ts",
    "*.test.tsx",
    ".gitignore",
    # The screen gallery, its screen tour and their screenshot scripts:
    # `npm run build`, which Tauri runs, builds index.html alone (vite --mode
    # app), so none of these reach an installer (docs/DESIGN.md §7).
    "apps/secs2r/ui/gallery.html",
    "apps/secs2r/ui/src/gallery/*",
    "apps/secs2r/ui/scripts/*",
    # Makes the icons next to it; the icons ship, the script does not.
    "apps/secs2r/icons/*.py",
)
NOT_SHIPPED_DIRS = ("tests", "benches", "fuzz", "node_modules")

TAG_RE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")
MINOR_SUBJECT_RE = re.compile(r"^(feat|minor)(\([^)]*\))?!?:")
BREAKING_RE = re.compile(r"^[a-z]+(\([^)]*\))?!:|BREAKING[ -]CHANGE", re.MULTILINE)

CARGO_TOML = Path("Cargo.toml")
CARGO_LOCK = Path("Cargo.lock")
# The fuzz workspace (fuzz/Cargo.toml) locks the same path crates.
FUZZ_LOCK = Path("fuzz/Cargo.lock")

# The crates.io crates (ADR 0016) share one version of their own, kept by the
# public crate's manifest and tagged api-vX.Y.Z when published. Files under
# these directories of a published crate are not in its package.
API_MANIFEST = Path("crates/secs-api/Cargo.toml")
PYTHON_MANIFEST = Path("bindings/python/Cargo.toml")
API_NOT_PACKAGED_DIRS = ("tests", "examples", "benches")


# ---------------------------------------------------------------------------
# Pure logic (unit-tested)
# ---------------------------------------------------------------------------
def _matches(path: str, patterns) -> bool:
    base = path.rsplit("/", 1)[-1]
    for pat in patterns:
        if pat.endswith("/*"):
            prefix = pat[:-2]
            if path == prefix or path.startswith(prefix + "/"):
                return True
        elif fnmatch.fnmatch(base, pat) or fnmatch.fnmatch(path, pat):
            return True
    return False


def ships(path: str) -> bool:
    if path in SHIP_ALWAYS:
        return True
    if not path or not _matches(path, SHIP) or _matches(path, NOT_SHIPPED):
        return False
    return not any(part in NOT_SHIPPED_DIRS for part in path.split("/")[:-1])


def shipped_files(changed) -> list:
    return [f for f in changed if ships(f)]


def parse_version(v: str) -> tuple[int, int, int]:
    m = re.match(r"^(\d+)\.(\d+)\.(\d+)$", v.strip())
    if not m:
        raise ValueError(f"not a plain major.minor.patch version: {v!r}")
    return tuple(int(x) for x in m.groups())


def latest_tag(tags) -> str | None:
    """Highest plain ``vX.Y.Z`` tag; suffixed tags are ignored."""
    valid = [t for t in tags if TAG_RE.match(t)]
    return max(valid, key=lambda t: parse_version(t[1:])) if valid else None


def bump_kind(messages) -> str:
    """``minor`` or ``patch`` from the full messages of the shipped commits."""
    for msg in messages:
        subject = msg.strip().splitlines()[0] if msg.strip() else ""
        if MINOR_SUBJECT_RE.match(subject) or BREAKING_RE.search(msg):
            return "minor"
    return "patch"


def bumped(version: str, kind: str) -> str:
    major, minor, patch = parse_version(version)
    if kind == "minor":
        return f"{major}.{minor + 1}.0"
    return f"{major}.{minor}.{patch + 1}"


def decide(version: str, tags, shipped: list, messages, force: bool):
    """Return ``(action, version)``: action is ``release``, ``bump`` or ``none``."""
    if f"v{version}" not in tags:
        return "release", version
    if not shipped and not force:
        return "none", version
    candidate = bumped(version, bump_kind(messages))
    while f"v{candidate}" in tags:
        candidate = bumped(candidate, "patch")
    return "bump", candidate


def is_stale(head: str, main_head: str) -> bool:
    """A trigger is stale when main has moved past it (or was rewritten)."""
    return bool(main_head) and head != main_head


def is_prerelease(version: str) -> bool:
    return parse_version(version)[0] == 0


def workspace_version(cargo_toml: str) -> str:
    """``[workspace.package].version`` of the root Cargo.toml."""
    section = None
    for line in cargo_toml.splitlines():
        header = re.match(r"^\s*\[([^\]]+)\]\s*$", line)
        if header:
            section = header.group(1).strip()
            continue
        m = re.match(r'^\s*version\s*=\s*"([^"]+)"', line)
        if m and section == "workspace.package":
            return m.group(1)
    raise ValueError("no [workspace.package] version in Cargo.toml")


def set_workspace_version(cargo_toml: str, new: str) -> str:
    lines = cargo_toml.splitlines(keepends=True)
    section = None
    for i, line in enumerate(lines):
        header = re.match(r"^\s*\[([^\]]+)\]\s*$", line)
        if header:
            section = header.group(1).strip()
            continue
        if section == "workspace.package" and re.match(r'^\s*version\s*=\s*"', line):
            lines[i] = re.sub(r'"[^"]+"', f'"{new}"', line, count=1)
            return "".join(lines)
    raise ValueError("no [workspace.package] version in Cargo.toml")


def _package_lines(manifest: str):
    """(index, line) of the lines in a manifest's [package] table."""
    section = None
    for i, line in enumerate(manifest.splitlines(keepends=True)):
        header = re.match(r"^\s*\[([^\]]+)\]\s*$", line)
        if header:
            section = header.group(1).strip()
            continue
        if section == "package":
            yield i, line


def manifest_publishes(manifest: str) -> bool:
    """Whether cargo may publish a crate: its [package] says neither
    ``publish = false`` nor ``publish.workspace = true`` (the workspace's
    ``publish`` is false)."""
    for _, line in _package_lines(manifest):
        if re.match(r"^\s*publish\s*=\s*false\b", line) or re.match(
                r"^\s*publish\s*(\.\s*workspace\s*=\s*true|=\s*\{\s*workspace\s*=\s*true)", line):
            return False
    return True


def package_field(manifest: str, field: str) -> str:
    """A literal string field (``name``, ``version``) of [package]."""
    for _, line in _package_lines(manifest):
        m = re.match(rf'^\s*{field}\s*=\s*"([^"]+)"', line)
        if m:
            return m.group(1)
    raise ValueError(f"no literal [package] {field}")


def set_package_version(manifest: str, new: str) -> str:
    lines = manifest.splitlines(keepends=True)
    for i, line in _package_lines(manifest):
        if re.match(r'^\s*version\s*=\s*"', line):
            lines[i] = re.sub(r'"[^"]+"', f'"{new}"', line, count=1)
            return "".join(lines)
    raise ValueError("no literal [package] version")


def dependency_package(line: str):
    """The package a one-line dependency entry points at: its ``package`` key
    when it renames the crate, else its own key; None for other lines.

    A version string alone says nothing about whose version it is: a
    third-party crate pinned with ``=`` can share the API's version number
    (``libloading = "=0.9.0"`` beside API 0.9.0), so every rewrite of exact
    requirements goes by the package name."""
    key = re.match(r"^\s*([A-Za-z0-9_-]+)\s*=", line)
    if not key:
        return None
    renamed = re.search(r'\bpackage\s*=\s*"([^"]+)"', line)
    return renamed.group(1) if renamed else key.group(1)


def set_exact_requirements(cargo_toml: str, names, old: str, new: str) -> str:
    """Move the ``=old`` requirements of the named crates in
    [workspace.dependencies] to ``=new``; other lines stay as they are."""
    lines = cargo_toml.splitlines(keepends=True)
    section = None
    for i, line in enumerate(lines):
        header = re.match(r"^\s*\[([^\]]+)\]\s*$", line)
        if header:
            section = header.group(1).strip()
            continue
        if section == "workspace.dependencies" and dependency_package(line) in names:
            lines[i] = line.replace(f'"={old}"', f'"={new}"')
    return "".join(lines)


def set_dependency_versions(manifest: str, names, old: str, new: str) -> str:
    """Move the named crates' exact requirements in one package's
    [dependencies] table."""
    lines = manifest.splitlines(keepends=True)
    section = None
    for i, line in enumerate(lines):
        header = re.match(r"^\s*\[([^\]]+)\]\s*$", line)
        if header:
            section = header.group(1).strip()
            continue
        if section == "dependencies" and dependency_package(line) in names:
            lines[i] = line.replace(f'"={old}"', f'"={new}"')
    return "".join(lines)


def api_files(changed, crate_dirs) -> list:
    """Changed files that are in a published crate's package."""
    out = []
    for path in changed:
        for crate in crate_dirs:
            if path.startswith(crate + "/"):
                inside = path[len(crate) + 1:].split("/")[:-1]
                if not any(part in API_NOT_PACKAGED_DIRS for part in inside):
                    out.append(path)
                break
    return out


def api_decide(version: str, tags, changed: list, messages):
    """``(action, version)`` for the crates.io crates: ``publish`` a version
    that has no ``api-v`` tag yet (the first release, or a publish that
    stopped half way), ``bump`` when packaged files changed since the tag,
    else ``none``. The bump follows ``bump_kind``; before 1.0 a minor bump is
    the one that may break callers."""
    if f"api-v{version}" not in tags:
        return "publish", version
    if not changed:
        return "none", version
    candidate = bumped(version, bump_kind(messages))
    while f"api-v{candidate}" in tags:
        candidate = bumped(candidate, "patch")
    return "bump", candidate


def set_lock_versions(lock: str, old: str, new: str) -> str:
    """Rewrite ``old`` to ``new`` for the workspace packages of a Cargo.lock.

    Registry and git packages carry a ``source`` line right after ``version``;
    workspace members do not, so a dependency that happens to share the
    version is left alone.
    """
    pattern = re.compile(
        r'(?m)^(\[\[package\]\]\r?\nname = "[^"]+"\r?\nversion = ")'
        + re.escape(old) + r'("\r?\n)(?!source = )'
    )
    new_lock, n = pattern.subn(lambda m: m.group(1) + new + m.group(2), lock)
    if n == 0:
        raise ValueError(f"no workspace package at {old} in Cargo.lock")
    return new_lock


def workspace_packages(lock: str, version: str) -> set[str]:
    """Names of the workspace packages of a Cargo.lock at ``version``."""
    return set(re.findall(
        r'(?m)^\[\[package\]\]\r?\nname = "([^"]+)"\r?\nversion = "'
        + re.escape(version) + r'"\r?\n(?!source = )',
        lock,
    ))


def set_member_versions(lock: str, names: set[str], new: str) -> str:
    """Set ``new`` for the named path packages of another Cargo.lock.

    The fuzz workspace locks the path crates it fuzzes. Its lock was left at
    an older version by earlier bumps, so the version it holds is replaced
    whatever it is; its own package and registry packages stay as they are
    (audit #92 TD-60).
    """
    pattern = re.compile(
        r'(?m)^(\[\[package\]\]\r?\nname = "([^"]+)"\r?\nversion = ")[^"]*("\r?\n)(?!source = )'
    )
    return pattern.sub(
        lambda m: m.group(1) + new + m.group(3) if m.group(2) in names else m.group(0),
        lock,
    )


# ---------------------------------------------------------------------------
# I/O wrapper (runs in CI)
# ---------------------------------------------------------------------------
def _run(*args, check=True):
    return subprocess.run(
        list(args), check=check, capture_output=True, text=True,
        encoding="utf-8", errors="replace",
    )


def _git(*args) -> str:
    return _run("git", *args).stdout.strip()


def _read(path: Path) -> str:
    with open(path, encoding="utf-8", newline="") as fh:
        return fh.read()


def _write(path: Path, text: str) -> None:
    with open(path, "w", encoding="utf-8", newline="") as fh:
        fh.write(text)


def bump_commit_message(new: str, api: str | None = None) -> str:
    """Message of the bump commit. The bump is pushed with the release deploy
    key, which starts push workflows (the workflow token did not), so the body
    carries ``[skip ci]``: the bump commit starts neither release.yml nor
    ci.yml, and the release it belongs to is built by the run that made it.
    ``api`` is the crates.io version the same commit moves to, if any."""
    body = f"api v{api}\n\n[skip ci]" if api else "[skip ci]"
    return f"chore(release): v{new}\n\n{body}"


def fetch_remote_main(*, tags: bool = False) -> str:
    """Fetch main into its remote-tracking ref and return that named ref."""
    args = ["git", "fetch"]
    if tags:
        args.extend(["--tags", "--force"])
    args.extend(["--quiet", "origin", "main:refs/remotes/origin/main"])
    fetched = _run(*args, check=False)
    if fetched.returncode != 0:
        print(fetched.stderr, file=sys.stderr, end="")
        raise SystemExit("git fetch failed")
    return _git("rev-parse", "origin/main")


def _commit_bump(old: str, new: str, started_head: str,
                 api: tuple[str, str, dict[str, Path]] | None = None) -> str | None:
    """Commit the bump and push it to main; None when main moved meanwhile.

    ``old == new`` leaves the app version alone. ``api`` is ``(old, new,
    {name: manifest})`` of the crates.io crates when they move too."""
    staged = [str(CARGO_TOML), str(CARGO_LOCK)]
    cargo_toml = _read(CARGO_TOML)
    lock = _read(CARGO_LOCK)
    fuzz_lock = _read(FUZZ_LOCK) if FUZZ_LOCK.exists() else None
    if old != new:
        cargo_toml = set_workspace_version(cargo_toml, new)
        members = workspace_packages(lock, old)
        lock = set_lock_versions(lock, old, new)
        if fuzz_lock is not None:
            fuzz_lock = set_member_versions(fuzz_lock, members, new)
    if api is not None:
        api_old, api_new, manifests = api
        for manifest in manifests.values():
            updated = set_package_version(_read(manifest), api_new)
            updated = set_dependency_versions(updated, set(manifests), api_old, api_new)
            _write(manifest, updated)
            staged.append(str(manifest))
        cargo_toml = set_exact_requirements(cargo_toml, set(manifests), api_old, api_new)
        lock = set_member_versions(lock, set(manifests), api_new)
        if fuzz_lock is not None:
            fuzz_lock = set_member_versions(fuzz_lock, set(manifests), api_new)
    _write(CARGO_TOML, cargo_toml)
    _write(CARGO_LOCK, lock)
    if fuzz_lock is not None:
        _write(FUZZ_LOCK, fuzz_lock)
        staged.append(str(FUZZ_LOCK))
    _run("git", "config", "user.name", "github-actions[bot]")
    _run("git", "config", "user.email",
         "41898282+github-actions[bot]@users.noreply.github.com")
    _run("git", "add", *staged)
    _run("git", "commit", "-m", bump_commit_message(new, api[1] if api else None))
    # The push goes over SSH with the release deploy key (the gate's checkout),
    # which the default branch's "Required checks" ruleset lets through; the
    # bump commit has no checks of its own. A rejected push is tolerated only
    # when a newer main commit explains it; authentication and network failures
    # must fail the release instead of becoming a successful no-op.
    push = _run("git", "push", "origin", "HEAD:main", check=False)
    if push.returncode == 0:
        return _git("rev-parse", "HEAD")
    main_head = fetch_remote_main()
    if is_stale(started_head, main_head):
        print(f"::warning::bump push rejected; main moved during the gate\n{push.stderr}")
        return None
    print(push.stderr, file=sys.stderr, end="")
    raise SystemExit("bump push failed while main was unchanged")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--force", action="store_true",
                        help="bump and release even when nothing shipped changed")
    args = parser.parse_args(argv)

    head = os.environ.get("GITHUB_SHA") or _git("rev-parse", "HEAD")
    main_head = fetch_remote_main(tags=True)
    tags = set(_git("tag", "-l", "v*").splitlines())
    version = workspace_version(_read(CARGO_TOML))
    current_tag = f"v{version}"
    previous = latest_tag(tags)

    shipped, messages = [], []
    if current_tag in tags:
        # --no-renames: a shipped file moved out of a shipped path must count.
        shipped = shipped_files(
            _git("diff", "--no-renames", "--name-only", current_tag, head).splitlines())
        if shipped:
            log = _git("log", "--no-merges", "--format=%B%x00", f"{current_tag}..{head}",
                       "--", *shipped)
            messages = [m for m in log.split("\x00") if m.strip()]

    action, new_version = decide(version, tags, shipped, messages, args.force)
    if is_stale(head, main_head):
        print(f"main is at {main_head[:12]}, past this trigger; its run releases instead")
        action, new_version = "none", version
    print(f"head={head[:12]} version={version} tagged={current_tag in tags} "
          f"previous={previous}")
    print(f"shipped changes: {shipped[:20]}")
    print(f"decision: {action} {new_version}")

    # The crates.io crates publish only with an app release: a packaged file
    # of theirs is always a shipped file of the app.
    api_action, api_version, api_manifests = "none", None, {}
    if API_MANIFEST.exists():
        api_version = package_field(_read(API_MANIFEST), "version")
        api_manifests = {
            package_field(_read(m), "name"): m
            for m in sorted(Path("crates").glob("*/Cargo.toml"))
            if manifest_publishes(_read(m))
        }
        version_manifests = dict(api_manifests)
        package_dirs = [manifest.parent.as_posix() for manifest in api_manifests.values()]
        if PYTHON_MANIFEST.exists():
            python_manifest = _read(PYTHON_MANIFEST)
            python_version = package_field(python_manifest, "version")
            if python_version != api_version:
                raise SystemExit(
                    f"{PYTHON_MANIFEST} version {python_version} does not match API {api_version}"
                )
            version_manifests[package_field(python_manifest, "name")] = PYTHON_MANIFEST
            package_dirs.append(PYTHON_MANIFEST.parent.as_posix())
        if action != "none":
            api_tags = set(_git("tag", "-l", "api-v*").splitlines())
            api_changed, api_messages = [], []
            if f"api-v{api_version}" in api_tags:
                api_changed = api_files(_git("diff", "--no-renames", "--name-only",
                                             f"api-v{api_version}", head).splitlines(), package_dirs)
                if api_changed:
                    log = _git("log", "--no-merges", "--format=%B%x00", f"api-v{api_version}..{head}",
                               "--", *api_changed)
                    api_messages = [m for m in log.split("\x00") if m.strip()]
            api_action, new_api = api_decide(api_version, api_tags, api_changed, api_messages)
            print(f"public API: {api_action} {new_api} (changed: {api_changed[:20]})")
            if api_action == "bump":
                api_bump = (api_version, new_api, version_manifests)
            api_version = new_api

    release_sha = head
    if action == "bump" or api_action == "bump":
        if os.environ.get("GITHUB_REF", "refs/heads/main") != "refs/heads/main":
            raise SystemExit("a bump commits to main; run on main")
        bumped_sha = _commit_bump(version, new_version, head,
                                  api_bump if api_action == "bump" else None)
        if bumped_sha is None:
            action, new_version, api_action = "none", version, "none"
        else:
            release_sha = bumped_sha
            print(f"bumped to v{new_version}; release_sha={release_sha}")

    payload = (
        f"should_release={'false' if action == 'none' else 'true'}\n"
        f"version={new_version}\n"
        f"tag=v{new_version}\n"
        f"prerelease={'true' if is_prerelease(new_version) else 'false'}\n"
        f"previous_tag={previous or ''}\n"
        f"release_sha={release_sha}\n"
        f"api_publish={'true' if action != 'none' and api_action != 'none' else 'false'}\n"
        f"api_version={api_version or ''}\n"
        f"api_tag={'api-v' + api_version if api_version else ''}\n"
    )
    out = os.environ.get("GITHUB_OUTPUT")
    if out:
        with open(out, "a", encoding="utf-8") as fh:
            fh.write(payload)
    print(payload, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
