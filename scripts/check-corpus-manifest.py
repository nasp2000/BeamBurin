#!/usr/bin/env python3
"""Check the corpus manifest without downloading anything.

The manifest is `assets/external/corpus.tsv`. Remote entries are checked for
schema and, when the file is present, for size and SHA-256. Local Tier A
fixtures (`local:` URLs) must exist, match their size and hash, and every file
in `tests/fixtures/corpus` must be listed.

Usage:
    python3 scripts/check-corpus-manifest.py
    python3 scripts/check-corpus-manifest.py --strict-downloads
"""

from __future__ import annotations

import argparse
import hashlib
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "assets/external/corpus.tsv"
MANIFEST_VERSION = 3
EXTERNAL = ROOT / "assets/external/files"
FIXTURES = ROOT / "tests/fixtures/corpus"

COLUMNS = ["name", "sha256", "url", "format", "bytes", "license", "tier", "notes"]
FORMATS = {"svg", "dxf", "hpgl", "plt", "pdf", "bwp"}
TIERS = {"smoke", "full", "heavy"}
LICENSES = {
    "MIT",
    "MIT OR Apache-2.0",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "CC0-1.0",
    "CC-BY-4.0",
    "CC-BY-SA-2.5",
    "CC-BY-SA-3.0",
    "CC-BY-SA-4.0",
    "AGPL-3.0",
    "PD",
    "W3C",
    "NOASSERTION",
    "verify",
}
MAX_LOCAL_BYTES = 15 * 1024
MAX_LOCAL_TOTAL = 32 * 1024
MAX_REMOTE_BYTES = 5 * 1024 * 1024


class Entry:
    def __init__(self, line: int, fields: list[str]) -> None:
        self.line = line
        self.name, self.sha256, self.url, self.format, raw_bytes, self.license, self.tier, self.notes = fields
        self.bytes = int(raw_bytes)
        self.local = self.url.startswith("local:")

    @property
    def path(self) -> pathlib.Path:
        if self.local:
            return ROOT / self.url.removeprefix("local:")
        return EXTERNAL / self.name


def fail(messages: list[str], text: str) -> None:
    messages.append(text)


def load(messages: list[str]) -> list[Entry]:
    entries: list[Entry] = []
    text = MANIFEST.read_text(encoding="utf-8")
    if f"# manifest-version: {MANIFEST_VERSION}" not in text.splitlines():
        fail(messages, f"the manifest must declare # manifest-version: {MANIFEST_VERSION}")
    for number, line in enumerate(text.splitlines(), start=1):
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != len(COLUMNS):
            fail(messages, f"line {number}: expected {len(COLUMNS)} columns, found {len(fields)}")
            continue
        try:
            entries.append(Entry(number, fields))
        except ValueError:
            fail(messages, f"line {number}: bytes is not an integer: {fields[4]!r}")
    return entries


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def check_entry(entry: Entry, messages: list[str], strict: bool) -> None:
    prefix = f"line {entry.line} ({entry.name})"
    if not entry.name or "/" in entry.name or ".." in entry.name:
        fail(messages, f"{prefix}: invalid file name")
    if len(entry.sha256) != 64 or any(char not in "0123456789abcdef" for char in entry.sha256):
        fail(messages, f"{prefix}: sha256 is not a lower-case digest")
    if entry.format not in FORMATS:
        fail(messages, f"{prefix}: unknown format {entry.format!r}")
    extension = entry.name.rsplit(".", 1)[-1].lower()
    if extension != entry.format:
        fail(messages, f"{prefix}: format {entry.format!r} does not match .{extension}")
    if entry.license not in LICENSES:
        fail(messages, f"{prefix}: licence {entry.license!r} is not in the allowed set")
    if entry.tier not in TIERS:
        fail(messages, f"{prefix}: unknown tier {entry.tier!r}")
    if not entry.notes.strip():
        fail(messages, f"{prefix}: notes are empty")
    if entry.bytes <= 0:
        fail(messages, f"{prefix}: bytes must be positive")

    if entry.local:
        if not entry.url.startswith("local:tests/fixtures/corpus/"):
            fail(messages, f"{prefix}: local URL must point at tests/fixtures/corpus")
        if entry.tier != "smoke":
            fail(messages, f"{prefix}: local fixtures must use the smoke tier")
        if entry.bytes > MAX_LOCAL_BYTES:
            fail(messages, f"{prefix}: local fixture is larger than 15 KiB")
    else:
        if not entry.url.startswith("https://"):
            fail(messages, f"{prefix}: remote URL is not HTTPS")
        if entry.bytes > MAX_REMOTE_BYTES and entry.tier != "heavy":
            fail(messages, f"{prefix}: entries above 5 MiB must use the heavy tier")

    if entry.path.is_file():
        actual = entry.path.stat().st_size
        if actual != entry.bytes:
            fail(messages, f"{prefix}: size is {actual} bytes, manifest says {entry.bytes}")
        elif sha256(entry.path) != entry.sha256:
            fail(messages, f"{prefix}: SHA-256 does not match the file")
    elif entry.local:
        fail(messages, f"{prefix}: local fixture is missing at {entry.path.relative_to(ROOT)}")
    elif strict:
        fail(messages, f"{prefix}: remote file is missing; run scripts/fetch-external-corpus.sh")


def main() -> int:
    parser = argparse.ArgumentParser(description="Check the corpus manifest")
    parser.add_argument(
        "--strict-downloads",
        action="store_true",
        help="require every remote file to be present and match its hash",
    )
    args = parser.parse_args()

    messages: list[str] = []
    entries = load(messages)

    names: dict[str, Entry] = {}
    hashes: dict[str, Entry] = {}
    for entry in entries:
        if entry.name in names:
            fail(messages, f"line {entry.line}: duplicate name {entry.name}")
        names[entry.name] = entry
        if entry.sha256 in hashes:
            fail(messages, f"line {entry.line}: duplicate hash {entry.sha256[:12]}…")
        hashes[entry.sha256] = entry

    for entry in entries:
        check_entry(entry, messages, args.strict_downloads)

    local = [entry for entry in entries if entry.local]
    remote = [entry for entry in entries if not entry.local]
    local_bytes = sum(entry.bytes for entry in local)
    if local_bytes > MAX_LOCAL_TOTAL:
        fail(messages, f"local fixtures total {local_bytes} bytes; the limit is {MAX_LOCAL_TOTAL}")

    listed = {entry.name for entry in local}
    if FIXTURES.is_dir():
        for path in sorted(FIXTURES.iterdir()):
            if path.is_file() and path.name not in listed:
                fail(messages, f"tests/fixtures/corpus/{path.name} is not in the manifest")
    else:
        fail(messages, "tests/fixtures/corpus is missing")

    listed_remote = {entry.name for entry in remote}
    if EXTERNAL.is_dir():
        for path in sorted(EXTERNAL.iterdir()):
            if path.is_file() and path.name not in listed_remote:
                fail(messages, f"assets/external/files/{path.name} is not in the manifest")

    if messages:
        for message in messages:
            print(f"corpus-manifest: {message}", file=sys.stderr)
        print(f"corpus-manifest: {len(messages)} problem(s)", file=sys.stderr)
        return 1

    present = sum(1 for entry in remote if entry.path.is_file())
    print(
        "corpus-manifest: ok "
        f"(v{MANIFEST_VERSION}: {len(entries)} entries: {len(local)} local, {len(remote)} remote; "
        f"{present} remote file(s) present; {local_bytes} local bytes)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
