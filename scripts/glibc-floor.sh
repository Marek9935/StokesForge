#!/usr/bin/env bash
# Glibc floor of a Linux distribution folder (#389): every ELF file under <dir>
# (executables, .so, Python extensions) must need no GLIBC_x.y symbol above the
# floor. Prints the highest requirement found; exit 1 and the offending files
# otherwise.
#
# Usage: scripts/glibc-floor.sh <dir> [floor]    (default floor: scripts/glibc-floor.env)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=glibc-floor.env
source "$HERE/glibc-floor.env"
dir="${1:?usage: glibc-floor.sh <dir> [floor]}"
floor="${2:-$GLIBC_FLOOR}"
[ -d "$dir" ] || { echo "glibc-floor: not a directory: $dir" >&2; exit 2; }
command -v readelf >/dev/null || { echo "glibc-floor: readelf (binutils) is missing" >&2; exit 2; }

highest=""
bad=0
scanned=0
while IFS= read -r -d '' f; do
    # ELF magic: 0x7f 'E' 'L' 'F' (no dependency on `file`).
    [ "$(head -c4 "$f" 2>/dev/null | od -An -c | tr -d ' ')" = '177ELF' ] || continue
    scanned=$((scanned + 1))
    max="$(readelf --dyn-syms -W "$f" 2>/dev/null | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/^GLIBC_//' | sort -uV | tail -n1 || true)"
    [ -n "$max" ] || continue
    if [ -z "$highest" ] || [ "$(printf '%s\n%s\n' "$highest" "$max" | sort -V | tail -n1)" != "$highest" ]; then
        highest="$max"
    fi
    if [ "$(printf '%s\n%s\n' "$floor" "$max" | sort -V | tail -n1)" != "$floor" ]; then
        echo "glibc-floor: $f needs GLIBC_$max (floor $floor)" >&2
        bad=1
    fi
done < <(find "$dir" -type f -print0)

echo "glibc-floor: $scanned ELF files, highest requirement GLIBC_${highest:-none} (floor $floor)"
[ "$bad" = 0 ] || exit 1
