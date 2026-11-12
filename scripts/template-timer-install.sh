#!/usr/bin/env bash
# Install the weekly template-target upkeep as a systemd user timer (#523).
# Run it yourself, once; undo with scripts/template-timer-uninstall.sh. Safe to run again.
#
# Every Sunday night (or at the next boot when the machine was off) `scripts/template-target.sh weekly`
# prunes incremental/ and release/ of worktrees paused for 3 days or more, then refreshes the template
# target when it is above the size threshold. Lowest CPU and disk priority; the builds it starts go
# through the cargo queue in the `low` class, so install scripts/cargo-queue-install.sh first.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Main checkout, whichever worktree this script is run from.
main="$(dirname "$(git -C "$here" rev-parse --path-format=absolute --git-common-dir)")"
unit_dir="$HOME/.config/systemd/user"
shim="$HOME/.local/share/stokesforge/cargo-queue/bin/cargo"

[ -x "$shim" ] || { echo "the cargo queue is not installed ($shim): run scripts/cargo-queue-install.sh first; nothing installed" >&2; exit 1; }
mkdir -p "$unit_dir"
cat > "$unit_dir/stokesforge-template.service" <<UNIT
[Unit]
Description=StokesForge template target upkeep (scripts/template-target.sh weekly, #523)

[Service]
Type=oneshot
WorkingDirectory=$main
ExecStart=/usr/bin/env bash $main/scripts/template-target.sh weekly
Nice=19
IOSchedulingClass=idle
UNIT
cat > "$unit_dir/stokesforge-template.timer" <<UNIT
[Unit]
Description=Weekly StokesForge template target upkeep (#523)

[Timer]
OnCalendar=Sun 03:00
Persistent=true
RandomizedDelaySec=30min

[Install]
WantedBy=timers.target
UNIT
systemctl --user daemon-reload
systemctl --user enable --now stokesforge-template.timer
cat <<MSG
installed: $unit_dir/stokesforge-template.{service,timer}
- Next run: systemctl --user list-timers stokesforge-template.timer
- Run it now: systemctl --user start stokesforge-template.service ; journalctl --user -u stokesforge-template.service
- The first template is built by hand: scripts/template-target.sh refresh (about 50 min at 2 jobs, cold).
MSG
