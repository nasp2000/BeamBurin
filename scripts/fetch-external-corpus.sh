#!/usr/bin/env bash
# Download the hash-pinned external corpus before the scheduled corpus tests.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
MANIFEST="$ROOT/assets/external/corpus.tsv"
OUT="$ROOT/assets/external/files"
mkdir -p "$OUT"
while IFS=$'\t' read -r name hash url; do
  [ -z "${name:-}" ] && continue
  case "$name" in \#*) continue ;; esac
  destination="$OUT/$name"
  if [ -f "$destination" ] && printf '%s  %s\n' "$hash" "$destination" | sha256sum -c --status; then
    echo "cached $name"
    continue
  fi
  echo "download $name"
  curl --fail --location --retry 3 --silent --show-error --output "$destination.tmp" "$url"
  printf '%s  %s\n' "$hash" "$destination.tmp" | sha256sum -c -
  mv "$destination.tmp" "$destination"
done < "$MANIFEST"
