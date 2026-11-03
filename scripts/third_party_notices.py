#!/usr/bin/env python3
"""Regenerate the dependency table in THIRD_PARTY_NOTICES.md.

The table records every registry crate in Cargo.lock with the license declared
by its manifest. Run this after a dependency change:

    python3 scripts/third_party_notices.py

CI only checks that every locked crate is named in the notices, so this script
is a maintenance helper; it never compiles anything.
"""

from __future__ import annotations

import glob
import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCK = ROOT / "Cargo.lock"
NOTICES = ROOT / "THIRD_PARTY_NOTICES.md"
BEGIN = "<!-- BEGIN DEPENDENCIES -->"
END = "<!-- END DEPENDENCIES -->"


def crate_dirs() -> list[pathlib.Path]:
    return [pathlib.Path(p) for p in glob.glob(str(pathlib.Path.home() / ".cargo/registry/src/*"))]


def license_for(name: str, version: str, roots: list[pathlib.Path]) -> str:
    for root in roots:
        manifest = root / f"{name}-{version}" / "Cargo.toml"
        if manifest.is_file():
            try:
                data = tomllib.loads(manifest.read_text())
            except (OSError, tomllib.TOMLDecodeError):
                continue
            package = data.get("package", {})
            license_text = package.get("license")
            if license_text:
                return license_text
            if package.get("license-file"):
                return f"see {package['license-file']}"
    return "unknown"


def dependency_block() -> str:
    lock = tomllib.loads(LOCK.read_text())
    roots = crate_dirs()
    rows = []
    for package in lock["package"]:
        if "source" not in package:
            continue
        name = package["name"]
        version = package["version"]
        rows.append((name, version, license_for(name, version, roots)))
    rows.sort(key=lambda row: (row[0].lower(), row[1]))
    lines = [
        BEGIN,
        "",
        f"{len(rows)} crates are used by BeamBurin (including build and test dependencies). The license shown is the",
        "one declared by the crate manifest; where a crate offers a choice, the",
        "permissive option (MIT or Apache-2.0) is used for this distribution.",
        "",
        "| Crate | Version | License |",
        "|---|---|---|",
    ]
    for name, version, license_text in rows:
        lines.append(f"| `{name}` | {version} | {license_text} |")
    lines.extend(["", END])
    return "\n".join(lines)


def main() -> int:
    block = dependency_block()
    text = NOTICES.read_text()
    start = text.index(BEGIN)
    end = text.index(END) + len(END)
    updated = text[:start] + block + text[end:]
    if "--check" in sys.argv:
        if updated != text:
            print("THIRD_PARTY_NOTICES.md is stale; run scripts/third_party_notices.py")
            return 1
        print("THIRD_PARTY_NOTICES.md is current")
        return 0
    NOTICES.write_text(updated)
    print(f"updated {NOTICES.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
