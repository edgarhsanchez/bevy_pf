#!/usr/bin/env python3
"""Stamp the workspace version from a release tag.

The git tag is the single source of truth for what a release is called, so
the manifests are rewritten from it at release time rather than hand-edited
and kept in sync by hope. Two places have to move together or the publish
produces a broken crate:

  [workspace.package]      version = "X"   -- what each crate is published AS
  [workspace.dependencies] version = "X"   -- what bevy_pf REQUIRES of its
                                              siblings; cargo strips `path`
                                              when packaging and resolves
                                              this from crates.io

Miss the second and bevy_pf 0.2.0 ships demanding bevy_pf_xaml 0.1.0 — which
either resolves to a stale crate or fails outright.

Usage: set-version.py <version>   (no leading "v")
"""

import pathlib
import re
import subprocess
import sys

SEMVER = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)

# The crates whose requirement on each other must track the release version.
SIBLINGS = ("bevy_pf_xaml", "bevy_pf_macros", "bevy_pf")


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <version>", file=sys.stderr)
        return 2
    version = sys.argv[1]
    if not SEMVER.match(version):
        print(
            f"::error::'{version}' is not a semantic version "
            "(expected MAJOR.MINOR.PATCH, optionally -prerelease / +build)",
            file=sys.stderr,
        )
        return 1

    manifest = pathlib.Path("Cargo.toml")
    text = manifest.read_text()

    # [workspace.package] version — scoped to that table so the several other
    # `version = ` lines in the file (dependency requirements) are untouched.
    text, package_hits = re.subn(
        r'(\[workspace\.package\][^\[]*?\bversion\s*=\s*)"[^"]*"',
        rf'\g<1>"{version}"',
        text,
        count=1,
        flags=re.DOTALL,
    )
    if package_hits != 1:
        print("::error::could not find [workspace.package] version", file=sys.stderr)
        return 1

    # The sibling requirements in [workspace.dependencies].
    for crate in SIBLINGS:
        text, hits = re.subn(
            rf'(^{re.escape(crate)}\s*=\s*\{{[^}}]*?\bversion\s*=\s*)"[^"]*"',
            rf'\g<1>"{version}"',
            text,
            count=1,
            flags=re.MULTILINE | re.DOTALL,
        )
        if hits != 1:
            print(
                f"::error::could not find a version requirement for {crate} "
                "in [workspace.dependencies]",
                file=sys.stderr,
            )
            return 1

    manifest.write_text(text)

    # Trust nothing: ask cargo what the manifests now say. A regex that
    # silently matched the wrong line would otherwise publish a wrong
    # version, which cannot be taken back.
    meta = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        capture_output=True,
        text=True,
    )
    if meta.returncode != 0:
        print(f"::error::manifest is not loadable after edit:\n{meta.stderr}", file=sys.stderr)
        return 1
    import json

    wrong = [
        f"{p['name']}={p['version']}"
        for p in json.loads(meta.stdout)["packages"]
        if p["version"] != version
    ]
    if wrong:
        print(f"::error::these crates did not take the version: {', '.join(wrong)}", file=sys.stderr)
        return 1

    print(f"stamped every workspace crate at {version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
