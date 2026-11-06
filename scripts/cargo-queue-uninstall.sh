#!/usr/bin/env bash
# Remove the local cargo queue (#476): the block of ~/.bashrc (restored byte for byte), the slice
# unit and the installed copies, after putting back the power profile FullIA changed (#477). The state
# folder ~/.local/state/stokesforge/cargo-queue is kept.
set -euo pipefail

data="$HOME/.local/share/stokesforge/cargo-queue"
unit="$HOME/.config/systemd/user/sfbuild.slice"
rc="$HOME/.bashrc"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Put the power profile back first (#477): FullIA may have set it and the record is the only memory of what
# was there. A failure never stops the removal; the record stays in the kept state folder.
python3 -I "$here/cargo-queue.py" profile-restore || true

if [ -f "$rc" ]; then
python3 -I - "$rc" "$data/bashrc-newline-added" <<'PY'
import os, sys
rc, flag = sys.argv[1:3]
BEGIN = "# >>> stokesforge cargo-queue (#476) >>>\n"
END = "# <<< stokesforge cargo-queue (#476) <<<\n"
with open(rc, newline="") as f:
    text = f.read()
if BEGIN not in text:
    sys.exit(0)
a = text.index(BEGIN)
b = text.index(END, a) + len(END)
rest = text[:a] + text[b:]
if b == len(text) and os.path.exists(flag) and rest.endswith("\n"):
    rest = rest[:-1]                 # the newline install added to a file that had none
with open(rc, "w", newline="") as f:
    f.write(rest)
PY
fi
systemctl --user revert sfbuild.slice > /dev/null 2>&1 || true
rm -f "$unit"
rm -rf "$data"
systemctl --user daemon-reload
echo "removed the block of $rc, $unit and $data (state folder kept)"
