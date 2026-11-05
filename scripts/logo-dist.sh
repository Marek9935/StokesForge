#!/usr/bin/env bash
# The logo in an official build (#388, #444).
#
# assets/logo/ is not under the GPL (its LICENSE): only the official packages
# carry the logo, the code builds and runs without it. This script is the only
# reader of that folder. It does nothing unless STOKESFORGE_DIST_LOGO=1, which
# the CI sets on the release jobs only (`release-linux`, `dist-windows`).
#
# Usage:
#   scripts/logo-dist.sh generate                    write the PNG trees and the
#                                                    .ico files under target/logo-dist
#   scripts/logo-dist.sh app-ico                     print the path of the application .ico
#   scripts/logo-dist.sh install <dir> linux|windows copy them into an assembled package
#
# linux: share/icons/hicolor/<size>/{apps,mimetypes}/*.png and the two scalable
#   SVG (read by install.sh and by the RPM/DEB packages).
# windows: only share/icons/hicolor/256x256/apps/stokesforge.png (what the
#   application reads at run time) and stokesforge-project.ico (the icon of
#   the .stf files, referenced by stokesforge.iss); the application .ico goes
#   into the executable through STOKESFORGE_EXE_ICON (crates/stokesforge-app/build.rs).
# Both write licenses/logo/LICENSE, the notice of the logo.
#
# Variables (tests): STOKESFORGE_LOGO_DIR (default assets/logo), STOKESFORGE_LOGO_OUT
# (default target/logo-dist), STOKESFORGE_LOGO_GEN (generator command; default: the
# stokesforge-logo-gen crate, built here).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOGO_DIR="${STOKESFORGE_LOGO_DIR:-$REPO_ROOT/assets/logo}"
OUT="${STOKESFORGE_LOGO_OUT:-$REPO_ROOT/target/logo-dist}"
ICON_SVG="$LOGO_DIR/00-final/stokesforge-icone-clair.svg"
PROJECT_SVG="$LOGO_DIR/00-final/stokesforge-fichier-stf-clair.svg"
APP_ICON=stokesforge
PROJECT_ICON=application-x-stokesforge-project

usage() {
    echo "Usage: scripts/logo-dist.sh generate | app-ico | install <dir> linux|windows" >&2
    exit 2
}
[[ $# -ge 1 ]] || usage

if [[ "${STOKESFORGE_DIST_LOGO:-0}" != "1" ]]; then
    exit 0 # not an official build: no logo, nothing to do
fi

die() { echo "[logo] ERROR: $*" >&2; exit 1; }

require_sources() {
    [[ -f "$ICON_SVG" && -f "$PROJECT_SVG" && -f "$LOGO_DIR/LICENSE" ]] \
        || die "STOKESFORGE_DIST_LOGO=1 but $LOGO_DIR has no icon sources or LICENSE (is assets/logo/ removed?)"
}

generator() {
    if [[ -n "${STOKESFORGE_LOGO_GEN:-}" ]]; then
        printf '%s' "$STOKESFORGE_LOGO_GEN"
        return
    fi
    (cd "$REPO_ROOT" && cargo build --locked --release -q -p stokesforge-logo-gen) >&2
    local exe="$REPO_ROOT/target/release/stokesforge-logo-gen"
    [[ -x "$exe" ]] || exe="$exe.exe"
    printf '%s' "$exe"
}

# A Windows path for a native program under Git Bash, the path itself elsewhere.
native() {
    if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s' "$1"; fi
}

case "$1" in
    generate)
        require_sources
        gen="$(generator)"
        rm -rf "$OUT"
        mkdir -p "$OUT"
        "$gen" --icon "$(native "$ICON_SVG")" --project "$(native "$PROJECT_SVG")" --out "$(native "$OUT")" \
            || die "the generator refused the logo sources"
        echo "[logo] generated under $OUT"
        ;;
    app-ico)
        [[ -s "$OUT/stokesforge.ico" ]] || die "no $OUT/stokesforge.ico: run 'logo-dist.sh generate' first"
        native "$OUT/stokesforge.ico"
        echo
        ;;
    install)
        [[ $# -eq 3 && -d "$2" ]] || usage
        require_sources
        [[ -d "$OUT/hicolor" ]] || die "nothing generated under $OUT: run 'logo-dist.sh generate' first"
        dest="${2%/}"
        case "$3" in
            linux)
                mkdir -p "$dest/share/icons"
                cp -a "$OUT/hicolor" "$dest/share/icons/"
                mkdir -p "$dest/share/icons/hicolor/scalable/apps" "$dest/share/icons/hicolor/scalable/mimetypes"
                cp "$ICON_SVG" "$dest/share/icons/hicolor/scalable/apps/$APP_ICON.svg"
                cp "$PROJECT_SVG" "$dest/share/icons/hicolor/scalable/mimetypes/$PROJECT_ICON.svg"
                ;;
            windows)
                mkdir -p "$dest/share/icons/hicolor/256x256/apps"
                cp "$OUT/hicolor/256x256/apps/$APP_ICON.png" "$dest/share/icons/hicolor/256x256/apps/"
                cp "$OUT/stokesforge-project.ico" "$dest/"
                ;;
            *) usage ;;
        esac
        mkdir -p "$dest/licenses/logo"
        cp "$LOGO_DIR/LICENSE" "$dest/licenses/logo/LICENSE"
        echo "[logo] installed into $dest ($3)"
        ;;
    *) usage ;;
esac
