#!/usr/bin/env bash
# StokesForge installer for Linux (#387). Shipped in the release archive as
# `StokesForge-<version>/install.sh` (source: scripts/install-linux.sh); it
# installs the folder it sits in. No package manager, no network.
#
#   ./install.sh                   install for the current user
#   ./install.sh --keep-previous   keep the previous version beside the new one
#   ./install.sh --system          install for all users (needs sudo)
#   ./install.sh --uninstall       remove everything this script installed
#   ./install.sh --uninstall X.Y.Z remove one kept version only
#   ./install.sh --yes             do not ask (needed when stdin is not a terminal)
#
# Per user:  ${XDG_DATA_HOME:-~/.local/share}/stokesforge/<version>/ (+ `current`),
#            ~/.local/bin/stokesforge, menu entry and MIME type under the data dir.
# System:    /opt/stokesforge/<version>/ (+ `current`), /usr/local/bin/stokesforge,
#            /usr/local/share/{applications,mime/packages,icons}.
# Never touched: projects, ~/.config/stokesforge, and the `recovery` folder that
# the application keeps in the data dir `stokesforge/` (invariant: only version
# folders, `current` and the files created here are ever removed).
#
# Test-only: STOKESFORGE_INSTALL_ROOT=<dir> with --system replaces the root `/`
# by <dir> (Exec and links included) and lifts the root requirement.
#
# Packaging-only (not in `usage`): the RPM and DEB packages (#390) reuse this
# script as the single writer of the menu entry, MIME type and icons:
#   install.sh --stage-integration <share dir> <exec path> <version dir>
# writes <share dir>/{applications,mime/packages,icons} for a program that will
# live at <exec path>, with the icons found in <version dir>/share/icons. It
# changes nothing else: no root, no package manager, no cache refresh, no copy.
set -euo pipefail

VERSION_RE='^[0-9]+\.[0-9]+\.[0-9]+$'
MIME_TYPE='application/x-stokesforge-project'

say() { printf 'install: %s\n' "$*"; }
warn() { printf 'install: warning: %s\n' "$*" >&2; }
die() { printf 'install: error: %s\n' "$*" >&2; exit 1; }

usage() {
    cat <<'USAGE'
Usage: install.sh [--system] [--keep-previous] [--yes]
       install.sh [--system] --uninstall [X.Y.Z]
       install.sh --help

  --system         install for all users in /opt/stokesforge (run with sudo)
  --keep-previous  keep the previous version (command stokesforge-X.Y.Z and
                   menu entry "StokesForge X.Y.Z") instead of removing it
  --uninstall      remove everything installed by this script; with X.Y.Z,
                   only that kept version (not the current one)
  --yes            answer yes to questions (required without a terminal)
USAGE
}

# Paths: APPDIR (versions), BIN (commands), SHARE (applications, mime, icons).
setup_paths() {
    if [ "$SYSTEM" = 1 ]; then
        local root=""
        if [ -n "${STOKESFORGE_INSTALL_ROOT:-}" ]; then
            root="${STOKESFORGE_INSTALL_ROOT%/}"
        elif [ "$(id -u)" != 0 ]; then
            die "--system needs administrator rights; re-run with sudo"
        fi
        APPDIR="$root/opt/stokesforge"
        BIN="$root/usr/local/bin"
        SHARE="$root/usr/local/share"
    else
        local data="${XDG_DATA_HOME:-$HOME/.local/share}"
        APPDIR="$data/stokesforge"
        BIN="$HOME/.local/bin"
        SHARE="$data"
    fi
    case "$APPDIR$BIN" in
        *[\"\`\$\\]* | *$'\n'*) die "paths with quotes, backquotes, dollars or backslashes are not supported" ;;
    esac
}

# True when an RPM or DEB package owns the system folder.
package_owned() {
    if command -v rpm >/dev/null 2>&1 && rpm -qf "$APPDIR" >/dev/null 2>&1; then
        return 0
    fi
    if command -v dpkg >/dev/null 2>&1 && dpkg -S "$APPDIR" >/dev/null 2>&1; then
        return 0
    fi
    return 1
}

installed_versions() {
    local d name
    for d in "$APPDIR"/*/; do
        [ -d "$d" ] || continue
        name="$(basename "$d")"
        if [[ "$name" =~ $VERSION_RE ]]; then
            printf '%s\n' "$name"
        fi
    done
}

current_version() {
    if [ -L "$APPDIR/current" ]; then
        basename "$(readlink "$APPDIR/current")"
    fi
}

# True when $1 sorts strictly before $2 (version order).
older_than() {
    [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n1)" = "$1" ]
}

write_desktop() { # write_desktop <file> <name> <exec path> <with icon: 0|1>
    local file="$1" name="$2" exe="$3" icon="$4"
    local esc="${exe//%/%%}"
    {
        printf '[Desktop Entry]\nType=Application\nName=%s\n' "$name"
        printf 'Exec="%s" %%f\n' "$esc"
        printf 'Categories=Science;\nMimeType=%s;\nTerminal=false\n' "$MIME_TYPE"
        if [ "$icon" = 1 ]; then
            printf 'Icon=stokesforge\n'
        fi
    } > "$file"
}

write_mime() {
    cat > "$SHARE/mime/packages/stokesforge.xml" <<XML
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="$MIME_TYPE">
    <comment>StokesForge project</comment>
    <glob pattern="*.stf"/>
  </mime-type>
</mime-info>
XML
}

# Link $1 -> $2, unless $1 is a file of the user's own.
link_command() {
    if [ -e "$1" ] && [ ! -L "$1" ]; then
        warn "$1 exists and is not a link: left as it is"
        return 0
    fi
    ln -sfn "$2" "$1"
}

refresh_caches() {
    if [ -d "$SHARE/applications" ] && command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$SHARE/applications" >/dev/null 2>&1 || warn "update-desktop-database failed"
    fi
    if [ -d "$SHARE/mime/packages" ] && command -v update-mime-database >/dev/null 2>&1; then
        update-mime-database "$SHARE/mime" >/dev/null 2>&1 || warn "update-mime-database failed"
    fi
    if [ -d "$SHARE/icons/hicolor" ] && command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "$SHARE/icons/hicolor" >/dev/null 2>&1 || warn "gtk-update-icon-cache failed"
    fi
}

has_icon() { # an icon of the application inside the installed version folder $1
    local f
    for f in "$1"/share/icons/hicolor/*/apps/stokesforge.png "$1"/share/icons/hicolor/*/apps/stokesforge.svg; do
        [ -f "$f" ] && return 0
    done
    return 1
}

install_icons() { # install_icons <version folder>
    local f rel
    for f in "$1"/share/icons/hicolor/*/apps/stokesforge.png "$1"/share/icons/hicolor/*/apps/stokesforge.svg; do
        [ -f "$f" ] || continue
        rel="${f#"$1"/share/icons/}"
        mkdir -p "$SHARE/icons/$(dirname "$rel")"
        cp -f "$f" "$SHARE/icons/$rel"
    done
}

add_versioned() { # add_versioned <version>: own command and menu entry, on its folder
    local v="$1" icon=0
    has_icon "$APPDIR/$v" && icon=1
    link_command "$BIN/stokesforge-$v" "$APPDIR/$v/stokesforge-app"
    write_desktop "$SHARE/applications/stokesforge-$v.desktop" "StokesForge $v" "$APPDIR/$v/stokesforge-app" "$icon"
}

remove_versioned() { # remove_versioned <version>: its command and entry (not its folder)
    local l="$BIN/stokesforge-$1"
    if [ -L "$l" ]; then
        rm -f "$l"
    fi
    rm -f "$SHARE/applications/stokesforge-$1.desktop"
}

do_install() {
    local src exe_out v
    src="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
    if [ ! -f "$src/stokesforge-app" ] || [ ! -x "$src/stokesforge-app" ]; then
        die "$src/stokesforge-app is missing or not executable: broken archive"
    fi
    exe_out="$("$src/stokesforge-app" --version 2>/dev/null)" || die "'stokesforge-app --version' failed: broken archive"
    v="${exe_out#stokesforge-app }"
    [[ "$exe_out" == "stokesforge-app "* && "$v" =~ $VERSION_RE ]] || die "unexpected answer of 'stokesforge-app --version': $exe_out"

    if [ -d "$APPDIR" ]; then
        local real
        real="$(cd "$APPDIR" && pwd -P)"
        case "$src/" in
            "$real"/*) die "this folder is already an installed copy ($src); unpack the archive elsewhere" ;;
        esac
    fi
    if [ "$SYSTEM" = 1 ] && package_owned; then
        die "$APPDIR belongs to an installed package; use the package manager"
    fi

    local cur newest alongside=0
    cur="$(current_version)"
    newest="$(installed_versions | sort -V | tail -n1)"
    if [ -n "$newest" ] && [ "$v" != "$cur" ] && older_than "$v" "$newest"; then
        alongside=1
        say "version $newest is already installed; $v is older."
        if [ "$YES" != 1 ]; then
            [ -t 0 ] || die "refusing to install an older version without a terminal; use --yes to install $v alongside"
            local answer=""
            read -r -p "Install $v alongside $newest? [y/N] " answer || true
            case "$answer" in y | Y | yes | YES) ;; *) die "cancelled" ;; esac
        fi
    fi

    mkdir -p "$APPDIR" "$BIN" "$SHARE/applications" "$SHARE/mime/packages"
    TMP_PATH="$APPDIR/.$v.tmp"
    rm -rf "$TMP_PATH"
    cp -a "$src/." "$TMP_PATH"
    if [ -e "$APPDIR/$v" ]; then
        rm -rf "$APPDIR/.$v.old"
        mv "$APPDIR/$v" "$APPDIR/.$v.old"
        mv "$TMP_PATH" "$APPDIR/$v"
        rm -rf "$APPDIR/.$v.old"
    else
        mv "$TMP_PATH" "$APPDIR/$v"
    fi

    local icon=0
    if has_icon "$APPDIR/$v"; then
        icon=1
        install_icons "$APPDIR/$v"
    fi
    write_mime

    if [ "$alongside" = 1 ]; then
        add_versioned "$v"
        say "installed $v alongside (command stokesforge-$v, menu entry \"StokesForge $v\"); the current version stays $cur"
    else
        ln -sfn "$v" "$APPDIR/current"
        link_command "$BIN/stokesforge" "$APPDIR/current/stokesforge-app"
        write_desktop "$SHARE/applications/stokesforge.desktop" "StokesForge" "$APPDIR/current/stokesforge-app" "$icon"
        remove_versioned "$v"
        if [ -n "$cur" ] && [ "$cur" != "$v" ]; then
            if [ "$KEEP" = 1 ]; then
                add_versioned "$cur"
                say "kept $cur (command stokesforge-$cur, menu entry \"StokesForge $cur\")"
            else
                rm -rf "${APPDIR:?}/$cur"
                remove_versioned "$cur"
                say "removed $cur"
            fi
        fi
        say "installed $v"
    fi
    refresh_caches
    case ":$PATH:" in
        *":$BIN:"*) ;;
        *) say "note: $BIN is not in your PATH; add it to start 'stokesforge' from a terminal" ;;
    esac
}

owned_link() { # a link of ours: points into the install folder
    [ -L "$1" ] && case "$(readlink "$1")" in "$APPDIR"/*) return 0 ;; esac
    return 1
}

do_uninstall_version() {
    local v="$1" cur
    [ -d "$APPDIR/$v" ] || die "version $v is not installed"
    cur="$(current_version)"
    [ "$v" != "$cur" ] || die "$v is the current version; use --uninstall without a version to remove everything"
    rm -rf "${APPDIR:?}/$v"
    remove_versioned "$v"
    refresh_caches
    say "removed $v"
}

do_uninstall_all() {
    local d f
    if [ -d "$APPDIR" ]; then
        for d in "$APPDIR"/*/ "$APPDIR"/.*.tmp "$APPDIR"/.*.old; do
            [ -d "$d" ] || continue
            d="${d%/}"
            if [[ "$(basename "$d")" =~ $VERSION_RE ]] || [[ "$(basename "$d")" =~ ^\.[0-9.]+\.(tmp|old)$ ]]; then
                rm -rf "$d"
            fi
        done
        rm -f "$APPDIR/current"
        # The data folder also holds `recovery/`: only an empty one goes.
        rmdir "$APPDIR" 2>/dev/null || true
    fi
    for f in "$BIN"/stokesforge*; do
        if owned_link "$f"; then
            rm -f "$f"
        fi
    done
    rm -f "$SHARE/applications"/stokesforge.desktop "$SHARE/applications"/stokesforge-*.desktop
    rm -f "$SHARE/mime/packages/stokesforge.xml"
    rm -f "$SHARE"/icons/hicolor/*/apps/stokesforge.png "$SHARE"/icons/hicolor/*/apps/stokesforge.svg
    refresh_caches
    say "uninstalled; projects and ~/.config/stokesforge are untouched"
}

# Packaging mode (see the header): the integration files only, under $1.
stage_integration() { # stage_integration <share dir> <exec path> <version dir>
    [ $# = 3 ] || { usage >&2; exit 2; }
    SHARE="${1%/}"
    case "$2" in
        *[\"\`\$\\]* | *$'\n'*) die "paths with quotes, backquotes, dollars or backslashes are not supported" ;;
    esac
    local icon=0
    mkdir -p "$SHARE/applications" "$SHARE/mime/packages"
    if has_icon "$3"; then
        icon=1
        install_icons "$3"
    fi
    write_mime
    write_desktop "$SHARE/applications/stokesforge.desktop" "StokesForge" "$2" "$icon"
}

main() {
    if [ "${1:-}" = --stage-integration ]; then
        shift
        stage_integration "$@"
        return 0
    fi
    SYSTEM=0 KEEP=0 YES=0 UNINSTALL=0 UNVER="" TMP_PATH=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --system) SYSTEM=1 ;;
            --keep-previous) KEEP=1 ;;
            --yes | -y) YES=1 ;;
            --uninstall)
                UNINSTALL=1
                if [ $# -gt 1 ] && [[ "$2" != -* ]]; then
                    UNVER="$2"
                    shift
                fi
                ;;
            -h | --help) usage; exit 0 ;;
            *) usage >&2; exit 2 ;;
        esac
        shift
    done
    if [ -n "$UNVER" ] && ! [[ "$UNVER" =~ $VERSION_RE ]]; then
        usage >&2
        exit 2
    fi
    setup_paths
    trap 'if [ -n "$TMP_PATH" ]; then rm -rf "$TMP_PATH"; fi' EXIT
    if [ "$SYSTEM" = 1 ] && [ "$UNINSTALL" = 1 ] && [ -e "$APPDIR" ] && package_owned; then
        die "$APPDIR belongs to an installed package; use the package manager"
    fi
    if [ "$UNINSTALL" = 1 ]; then
        if [ -n "$UNVER" ]; then
            do_uninstall_version "$UNVER"
        else
            do_uninstall_all
        fi
    else
        do_install
    fi
}

main "$@"
