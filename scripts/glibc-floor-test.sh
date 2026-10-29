#!/usr/bin/env bash
# Test of scripts/glibc-floor.sh (#389): no network, copies of system ELF files.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK="$HERE/glibc-floor.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fail=0
count=0
t() { # t <description> <command...>: the command must succeed
    local desc="$1"; shift; count=$((count + 1))
    "$@" >/dev/null 2>&1 || { echo "FAILED: $desc" >&2; fail=1; }
}
tn() { # tn <description> <command...>: the command must fail
    local desc="$1"; shift; count=$((count + 1))
    ! "$@" >/dev/null 2>&1 || { echo "FAILED: $desc" >&2; fail=1; }
}

# A system shared library linked against glibc, found through ldd.
so="$(ldd "$(command -v ls)" | awk '/libc\.so/ {print $3}' | head -n1)"
[ -f "$so" ] || { echo "glibc-floor-test: no libc.so found" >&2; exit 2; }

d="$tmp/dist"
mkdir -p "$d/lib" "$tmp/empty"
cp "$(command -v ls)" "$d/fakeapp-ls"
cp "$(type -P true)" "$d/fakeapp-true"
cp "$so" "$d/lib/libfake.so.6"
echo "GLIBC_99.0 is only text here" >"$d/notes.txt"
printf '#!/bin/sh\necho GLIBC_99.0\n' >"$d/script.sh"

t "floor 99.0 passes" "$CHECK" "$d" 99.0
tn "floor 2.0 fails" "$CHECK" "$d" 2.0
out="$("$CHECK" "$d" 2.0 2>&1 || true)"
count=$((count + 1))
case "$out" in *fakeapp-ls*) ;; *) echo "FAILED: executable not named" >&2; fail=1 ;; esac
count=$((count + 1))
case "$out" in *libfake.so.6*) ;; *) echo "FAILED: .so in lib/ not named" >&2; fail=1 ;; esac
count=$((count + 1))
case "$out" in *notes.txt*|*script.sh*) echo "FAILED: non-ELF file reported" >&2; fail=1 ;; esac
count=$((count + 1))
case "$("$CHECK" "$d" 99.0 2>&1)" in *"highest requirement GLIBC_"[0-9]*) ;; *) echo "FAILED: highest not printed" >&2; fail=1 ;; esac
count=$((count + 1))
case "$("$CHECK" "$d" 99.0 2>&1)" in *": 3 ELF files,"*) ;; *) echo "FAILED: non-ELF files counted or ELF missed" >&2; fail=1 ;; esac
mkdir "$tmp/text"; echo "GLIBC_99.0" >"$tmp/text/a.txt"
t "text file alone passes at floor 2.0" "$CHECK" "$tmp/text" 2.0
t "empty dir passes" "$CHECK" "$tmp/empty" 2.0
t "default floor accepts the empty dir" "$CHECK" "$tmp/empty"
tn "missing dir fails" "$CHECK" "$tmp/nope"

if [ "$fail" != 0 ]; then
    echo "glibc-floor-test: FAILED ($count checks)" >&2
    exit 1
fi
echo "glibc-floor-test: $count checks OK"
