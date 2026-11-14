#!/usr/bin/env bash
# Remove the weekly template-target timer (#523). The template itself (../_stokesforge/.template) is kept.
set -euo pipefail
unit_dir="$HOME/.config/systemd/user"
systemctl --user disable --now stokesforge-template.timer > /dev/null 2>&1 || true
rm -f "$unit_dir/stokesforge-template.timer" "$unit_dir/stokesforge-template.service"
systemctl --user daemon-reload
echo "removed the stokesforge-template timer and service (the template folder is kept)"
