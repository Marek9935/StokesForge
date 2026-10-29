#!/bin/sh
# Pre-install script of the RPM and DEB packages (#390). The folder
# /opt/stokesforge belongs to the package; a copy installed there by
# `install.sh --system` (it leaves a `current` link) would be overwritten
# half way, so the installation is refused. Upgrading the package itself is fine.
set -e
if [ -L /opt/stokesforge/current ]; then
    echo "stokesforge: /opt/stokesforge holds an installation made by install.sh --system." >&2
    echo "stokesforge: remove it first: <archive folder>/install.sh --system --uninstall" >&2
    exit 1
fi
exit 0
