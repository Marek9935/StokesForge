#!/usr/bin/env bash
# #412: one line naming the compiler in use, for the release notes:
# "rustc 1.91.0 (f8297e351 2025-10-28) (LLVM 21.1.2, x86_64-pc-windows-msvc)".
# Read from `rustc -vV`; prints nothing and fails if rustc is missing.
set -euo pipefail
info="$(rustc -vV)"
first="$(printf '%s\n' "$info" | sed -n '1p' | tr -d '\r')"
host="$(printf '%s\n' "$info" | sed -n 's/^host: //p' | tr -d '\r')"
llvm="$(printf '%s\n' "$info" | sed -n 's/^LLVM version: //p' | tr -d '\r')"
case "$first" in rustc\ *) ;; *) echo "[rustc-note] ERROR: unexpected rustc -vV output" >&2; exit 1 ;; esac
extra="$host"
[ -n "$llvm" ] && extra="LLVM ${llvm}, ${host}"
echo "${first} (${extra})"
