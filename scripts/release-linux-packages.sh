#!/usr/bin/env bash
# Packs the Linux distribution, already assembled and tested by
# `scripts/app-dist.sh`, into an RPM and a DEB package (#390):
#   dist/StokesForge-<version>-linux-x86_64.rpm
#   dist/StokesForge-<version>-linux-x86_64.deb
# One nfpm configuration (scripts/nfpm.yaml) for both formats; nothing is
# rebuilt. The program goes to /opt/stokesforge/, the menu entry, MIME type and
# icons are written by `install.sh --stage-integration` (single writer).
#
# Usage: scripts/release-linux-packages.sh vX.Y.Z | vX.Y.Z-rcN
#   (or STOKESFORGE_VERSION=X.Y.Z). Environment:
#   STOKESFORGE_DIST      folder to pack (default dist/stokesforge-app)
#   STOKESFORGE_PKG_OUT   output folder (default dist/)
#   PUBLISH_LINUX_PACKAGES  overrides scripts/linux-packages.env (the switch)
#   NFPM                  the nfpm command (default: nfpm on PATH)
# With the publication switch at 1 and no icon in the distribution, it stops.
#
# `pkg_version` is pure and can be sourced (STOKESFORGE_PKG_SOURCE_ONLY=1).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# vX.Y.Z -> X.Y.Z ; vX.Y.Z-rcN -> X.Y.Z~rcN (an rc sorts before the final).
pkg_version() {
    local v="${1#v}"
    if [[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        printf '%s\n' "$v"
    elif [[ "$v" =~ ^([0-9]+\.[0-9]+\.[0-9]+)-(rc[0-9]+)$ ]]; then
        printf '%s~%s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}"
    else
        return 1
    fi
}

if [ "${STOKESFORGE_PKG_SOURCE_ONLY:-}" = 1 ]; then
    return 0 2> /dev/null || exit 0
fi

# shellcheck source=linux-packages.env
source "$REPO_ROOT/scripts/linux-packages.env"
DIST="${STOKESFORGE_DIST:-$REPO_ROOT/dist/stokesforge-app}"
OUT="${STOKESFORGE_PKG_OUT:-$REPO_ROOT/dist}"
NFPM="${NFPM:-nfpm}"
VERSION="${1:+${1#v}}"
VERSION="${VERSION:-${STOKESFORGE_VERSION:-}}"
err() { echo "[packages] ERROR: $*" >&2; exit 1; }

[ -n "$VERSION" ] || err "version missing (argument vX.Y.Z or STOKESFORGE_VERSION)"
PKG_VERSION="$(pkg_version "$VERSION")" || err "unexpected version \"$VERSION\" (expected X.Y.Z or X.Y.Z-rcN)"
for needed in stokesforge-app stokesforge-selftest lib/python; do
    [ -e "$DIST/$needed" ] || err "$DIST/$needed missing, run scripts/app-dist.sh"
done
ls "$DIST" | grep -qi licen || err "no license notice in $DIST"
has_icon=0
for f in "$DIST"/share/icons/hicolor/*/apps/stokesforge.png "$DIST"/share/icons/hicolor/*/apps/stokesforge.svg; do
    [ -f "$f" ] && has_icon=1
done
if [ "${PUBLISH_LINUX_PACKAGES}" = 1 ] && [ "$has_icon" = 0 ]; then
    err "the packages are to be published but $DIST has no icon (#388)"
fi

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/opt" "$OUT"
# The whole folder, except install.sh (the package manager installs, and
# `release-linux.sh` may already have copied the installer into the folder).
# `-p` keeps the modification times: the bytecode (.pyc) shipped on purpose
# must stay valid, or Python would rewrite it as root, leaving unowned files.
cp -a "$DIST" "$stage/opt/stokesforge"
rm -f "$stage/opt/stokesforge/install.sh"
"$REPO_ROOT/scripts/install-linux.sh" --stage-integration "$stage/usr/share" \
    /opt/stokesforge/stokesforge-app "$stage/opt/stokesforge"

# nfpm.yaml plus one entry per icon file (a glob would flatten the folders).
config="$stage/nfpm.yaml"
sed -e "s|@STAGE@|$stage|g" -e "s|@SCRIPTS@|$REPO_ROOT/scripts/linux-packages|g" \
    "$REPO_ROOT/scripts/nfpm.yaml" > "$config"
while IFS= read -r icon; do
    rel="${icon#"$stage"/}"
    printf '  - src: %s\n    dst: /%s\n' "$icon" "$rel" >> "$config"
done < <(find "$stage/usr/share/icons" -type f 2> /dev/null | sort)

export NFPM_PKG_VERSION="$PKG_VERSION"
for format in rpm deb; do
    target="$OUT/StokesForge-${VERSION}-linux-x86_64.$format"
    "$NFPM" package --config "$config" --packager "$format" --target "$target" > /dev/null
    [ -f "$target" ] || err "$target was not produced"
    echo "[packages] $target"
done
