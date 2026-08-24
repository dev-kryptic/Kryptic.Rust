#!/usr/bin/env python3
"""Resolve the crates.io version for kryptic-daemon-client.

Patch (third number) auto-increments from the latest crates.io release when
the incoming Cargo.toml version keeps the same major and minor.

If this commit already changed major or minor (1.1.0, 2.0.0, …), that version
is published as-is. The first publish (crate not on crates.io yet) also
keeps the incoming version.
"""

from __future__ import annotations

import json
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

PACKAGE_ID = "kryptic-daemon-client"
REGISTRY_URL = f"https://crates.io/api/v1/crates/{PACKAGE_ID}"
CARGO_TOML = Path("Cargo.toml")
STABLE = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")
VERSION_LINE = re.compile(r'^version\s*=\s*"([^"]+)"\s*$', re.MULTILINE)


def parse(version: str) -> tuple[int, int, int] | None:
    match = STABLE.match(version.strip())
    if not match:
        return None
    return int(match.group(1)), int(match.group(2)), int(match.group(3))


def read_crate_version() -> str:
    text = CARGO_TOML.read_text(encoding="utf-8")
    match = VERSION_LINE.search(text)
    if not match:
        raise SystemExit(f"No version found in {CARGO_TOML}")
    return match.group(1).strip()


def latest_published() -> tuple[int, int, int] | None:
    request = urllib.request.Request(
        REGISTRY_URL,
        headers={"User-Agent": "kryptic-daemon-client-release (https://kryptic.dev)"},
    )
    try:
        with urllib.request.urlopen(request) as response:
            payload = json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise
    crate = payload.get("crate") or {}
    latest = crate.get("max_stable_version") or crate.get("max_version")
    return parse(latest) if latest else None


def render(version: tuple[int, int, int]) -> str:
    return f"{version[0]}.{version[1]}.{version[2]}"


def main() -> None:
    incoming_raw = sys.argv[1] if len(sys.argv) > 1 and sys.argv[1] else read_crate_version()
    incoming = parse(incoming_raw)
    if incoming is None:
        raise SystemExit(f"Incoming version must be major.minor.patch, got: {incoming_raw}")

    published = latest_published()
    if published is None:
        resolved = incoming
        reason = "first publish, keep incoming version"
    elif incoming[0] != published[0] or incoming[1] != published[1]:
        resolved = incoming
        reason = "major or minor changed, keep incoming version"
    else:
        resolved = (published[0], published[1], published[2] + 1)
        reason = "same major.minor, bump patch"

    print(f"incoming={render(incoming)}", file=sys.stderr)
    print(
        f"published={render(published) if published else '(none)'}",
        file=sys.stderr,
    )
    print(f"reason={reason}", file=sys.stderr)
    print(render(resolved))


if __name__ == "__main__":
    main()
