#!/usr/bin/env bash
# Remove the systemd user service of the tracking page (#441). The state folder
# ../_stokesforge/.suivi is kept.
set -euo pipefail
unit="$HOME/.config/systemd/user/stokesforge-suivi.service"
systemctl --user disable --now stokesforge-suivi.service || true
rm -f "$unit"
systemctl --user daemon-reload
echo "removed $unit (the state folder .suivi is kept)"
