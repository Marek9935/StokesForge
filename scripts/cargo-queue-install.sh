#!/usr/bin/env bash
# Install the local cargo queue and build modes (#476). Run it yourself; undo with
# scripts/cargo-queue-uninstall.sh. Safe to run again.
#
# Puts a COPY of scripts/cargo-queue.py in front of the real cargo (a bug or a merge in main
# must not break `cargo`), a minimal user slice unit, and a marked block as the very last
# lines of ~/.bashrc (~/.cargo/env is sourced there, so the shim must come after it).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
data="$HOME/.local/share/stokesforge/cargo-queue"
bin="$data/bin"
unit_dir="$HOME/.config/systemd/user"
unit="$unit_dir/sfbuild.slice"
rc="$HOME/.bashrc"

mkdir -p "$bin" "$unit_dir"
for name in cargo cargo-queue; do
    cp "$here/cargo-queue.py" "$bin/$name.new"
    chmod 755 "$bin/$name.new"
    mv -f "$bin/$name.new" "$bin/$name"      # replaced, never rewritten in place
done

cat > "$unit" <<'UNIT'
[Unit]
Description=StokesForge builds (cargo-queue, #476)
# The limits of the current mode are set at run time by cargo-queue.py (MODES table).
# No ManagedOOMMemoryPressure on purpose: systemd-oomd must not kill builds under MemoryHigh pressure.

[Slice]
# No limits here: every queued build applies the current mode to the slice.
UNIT
systemctl --user daemon-reload

python3 -I - "$rc" "$data/bashrc-newline-added" <<'PY'
import os, sys
rc, flag = sys.argv[1:3]
BEGIN = "# >>> stokesforge cargo-queue (#476) >>>\n"
END = "# <<< stokesforge cargo-queue (#476) <<<\n"
BLOCK = BEGIN + 'export PATH="$HOME/.local/share/stokesforge/cargo-queue/bin:$PATH"\n' + END
try:
    with open(rc, newline="") as f:
        text = f.read()
except FileNotFoundError:
    text = ""
had = BEGIN in text
if text.endswith(BLOCK):
    sys.exit(0)                      # already the last lines: nothing to touch
if had:                              # something was appended after it: move it to the end
    a = text.index(BEGIN)
    b = text.index(END, a) + len(END)
    text = text[:a] + text[b:]
    if os.path.exists(flag):
        os.remove(flag)              # the newline we added now ends a line that has content after it
elif text and not text.endswith("\n"):
    text += "\n"
    open(flag, "w").close()          # remembered so that uninstall restores the file byte for byte
with open(rc, "w", newline="") as f:
    f.write(text + BLOCK)
PY

cat <<MSG
installed: $bin/cargo (copy of scripts/cargo-queue.py), $unit, block at the end of $rc

- Only NEW terminals and agent sessions are covered. Running ones stay outside until restarted.
- GUI applications (editor, IDE) need a new login to see the new PATH.
- SF_CARGO_QUEUE=off cargo ...   skips the queue for one command.
- Rerun this script after a change of scripts/cargo-queue.py (the copy is not updated by itself).
- To get the mode control on the tracking page: systemctl --user restart stokesforge-suivi
- Modes from a terminal: cargo-queue mode full|dev|background, cargo-queue status
- FullIA also switches the power profile to performance on mains and puts the previous one back on leaving.
MSG
