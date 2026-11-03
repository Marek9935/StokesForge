#!/usr/bin/env bash
# Install the local tracking page as a systemd user service (#441).
# Run it yourself, once; undo with scripts/suivi-uninstall.sh. Safe to run again.
set -euo pipefail

port=8441
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Main checkout, whichever worktree this script is run from.
main="$(dirname "$(git -C "$here" rev-parse --path-format=absolute --git-common-dir)")"
unit_dir="$HOME/.config/systemd/user"
unit="$unit_dir/stokesforge-suivi.service"

if ss -ltn | grep -Eq "[:.]$port[[:space:]]" && ! systemctl --user is-active --quiet stokesforge-suivi.service; then
    echo "port $port is already in use by something else (ss -ltnp | grep :$port); nothing installed" >&2
    exit 1
fi

mkdir -p "$unit_dir"
cat > "$unit" <<UNIT
[Unit]
Description=StokesForge work-package tracking page (scripts/suivi.py serve)
Documentation=file://$main/.claude/skills/stokesforge-verification/references/suivi.md
StartLimitIntervalSec=60
StartLimitBurst=5

[Service]
Type=simple
WorkingDirectory=$main
ExecStart=$(command -v python3) -I $main/scripts/suivi.py serve --port $port
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
UNIT
systemctl --user daemon-reload
systemctl --user enable --now stokesforge-suivi.service
systemctl --user restart stokesforge-suivi.service

for _ in $(seq 1 20); do
    if curl -fsS "http://127.0.0.1:$port/api/state" > /dev/null 2>&1; then
        echo "installed: http://127.0.0.1:$port/"
        exit 0
    fi
    sleep 0.5
done
echo "no answer on http://127.0.0.1:$port/api/state; see: journalctl --user -u stokesforge-suivi" >&2
exit 1
