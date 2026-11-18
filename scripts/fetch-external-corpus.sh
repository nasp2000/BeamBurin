#!/usr/bin/env bash
# Download the hash-pinned external corpus before the scheduled corpus tests.
#
# Usage:
#   scripts/fetch-external-corpus.sh            download or reuse every remote file
#   scripts/fetch-external-corpus.sh --check    verify sizes and hashes, download nothing
#   scripts/fetch-external-corpus.sh --prune    remove files that are not in the manifest
#
# Local Tier A entries use a `local:` URL and are never downloaded; --check still
# verifies their size and hash.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
MANIFEST="$ROOT/assets/external/corpus.tsv"
OUT="$ROOT/assets/external/files"

check=0
prune=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --check) check=1 ;;
    --prune) prune=1 ;;
    -h|--help)
      sed -n '2,11p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "fetch-external-corpus: unknown option: $1" >&2
      exit 2
      ;;
  esac
  shift
done

download=0
reused=0
missing=0
failed=0
removed=0

verify() {
  local path=$1 hash=$2 bytes=$3 name=$4
  if [ ! -f "$path" ]; then
    echo "missing $name"
    return 1
  fi
  if [ "${bytes:-0}" -gt 0 ]; then
    local actual
    actual=$(stat -c '%s' "$path")
    if [ "$actual" -ne "$bytes" ]; then
      echo "size mismatch $name: expected $bytes, found $actual"
      return 1
    fi
  fi
  if ! printf '%s  %s\n' "$hash" "$path" | sha256sum -c --status; then
    echo "hash mismatch $name"
    return 1
  fi
  return 0
}

if [ "$check" -eq 0 ]; then
  mkdir -p "$OUT"
fi

while IFS=$'\t' read -r name hash url format bytes license tier notes; do
  [ -z "${name:-}" ] && continue
  case "$name" in \#*) continue ;; esac

  case "$url" in
    local:*)
      if [ "$check" -eq 1 ]; then
        if [ ! -f "$ROOT/${url#local:}" ]; then
          echo "missing $name"
          missing=$((missing + 1))
        elif ! verify "$ROOT/${url#local:}" "$hash" "${bytes:-0}" "$name"; then
          failed=$((failed + 1))
        fi
      fi
      continue
      ;;
  esac

  if [ "$check" -eq 1 ]; then
    if [ ! -f "$OUT/$name" ]; then
      echo "missing $name"
      missing=$((missing + 1))
    elif ! verify "$OUT/$name" "$hash" "${bytes:-0}" "$name"; then
      failed=$((failed + 1))
    fi
    continue
  fi

  destination="$OUT/$name"
  if [ -f "$destination" ] && verify "$destination" "$hash" "${bytes:-0}" "$name" >/dev/null 2>&1; then
    echo "cached $name"
    reused=$((reused + 1))
    continue
  fi

  echo "download $name"
  if ! curl --fail --location --retry 3 --retry-all-errors --retry-delay 2 \
      --max-time 120 --silent --show-error --output "$destination.tmp" "$url"; then
    rm -f "$destination.tmp"
    failed=$((failed + 1))
    continue
  fi
  if ! verify "$destination.tmp" "$hash" "${bytes:-0}" "$name"; then
    rm -f "$destination.tmp"
    failed=$((failed + 1))
    continue
  fi
  mv "$destination.tmp" "$destination"
  download=$((download + 1))
done < "$MANIFEST"

if [ "$prune" -eq 1 ] && [ -d "$OUT" ]; then
  keep=$(mktemp)
  while IFS=$'\t' read -r name hash url format bytes license tier notes; do
    [ -z "${name:-}" ] && continue
    case "$name" in \#*) continue ;; esac
    case "$url" in local:*) continue ;; esac
    printf '%s\n' "$name" >> "$keep"
  done < "$MANIFEST"
  while IFS= read -r path; do
    name=$(basename "$path")
    if ! grep -Fqx "$name" "$keep"; then
      echo "remove $name"
      rm -f "$path"
      removed=$((removed + 1))
    fi
  done < <(find "$OUT" -maxdepth 1 -type f -print)
  rm -f "$keep"
fi

echo "downloaded=$download reused=$reused missing=$missing failed=$failed removed=$removed"
if [ "$check" -eq 1 ] && { [ "$missing" -gt 0 ] || [ "$failed" -gt 0 ]; }; then
  echo "fetch-external-corpus: the corpus is incomplete; run without --check to download it" >&2
  exit 1
fi
if [ "$failed" -gt 0 ]; then
  exit 1
fi
