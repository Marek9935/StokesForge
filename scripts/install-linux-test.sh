#!/usr/bin/env bash
# Test of scripts/install-linux.sh (#387): no network, no sudo, a temporary HOME
# (its name contains a space) and fake archives whose `stokesforge-app` answers
# `--version`. --system is tested under STOKESFORGE_INSTALL_ROOT.
#
# Usage: scripts/install-linux-test.sh                    fake archives
#        scripts/install-linux-test.sh --archive <tar.gz> real archive (the tag job)
# SC2016: `bash -c '...' _ args` expands its arguments inside, on purpose.
# SC2329: the helpers are called through `t` / `tn`.
# shellcheck disable=SC2016,SC2329
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER="$HERE/install-linux.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fail=0
count=0
t() { # t <description> <command...>: the command must succeed
    local desc="$1"
    shift
    count=$((count + 1))
    if ! "$@" >/dev/null 2>&1; then
        echo "FAILED: $desc" >&2
        fail=1
    fi
}
tn() { # tn <description> <command...>: the command must fail
    local desc="$1"
    shift
    count=$((count + 1))
    if "$@" >/dev/null 2>&1; then
        echo "FAILED: $desc" >&2
        fail=1
    fi
}
finish() {
    if [ "$fail" != 0 ]; then
        echo "install-linux-test: FAILED ($count checks)" >&2
        exit 1
    fi
    echo "install-linux-test: $count checks OK"
    exit 0
}

# Only the tools the installer needs: no optional tool (update-*-database…).
MINBIN="$tmp/minbin"
mkdir -p "$MINBIN"
for tool in basename dirname ln cp mv rm sort head tail mkdir readlink rmdir cat id; do
    ln -s "$(command -v "$tool")" "$MINBIN/$tool"
done
BASH_BIN="$(command -v bash)"
DESKTOP_VALIDATE="$(command -v desktop-file-validate || true)"

new_home() { # new_home <name>: a HOME whose name contains a space
    H="$tmp/home $1"
    mkdir -p "$H"
}
# inst <archive dir> [args]: installs for $H, stdin is not a terminal.
inst() {
    local a="$1"
    shift
    env -u XDG_DATA_HOME -u STOKESFORGE_INSTALL_ROOT HOME="$H" PATH="$MINBIN${EXTRA_PATH:+:$EXTRA_PATH}" \
        "$BASH_BIN" "$a/install.sh" "$@" </dev/null >"$tmp/out" 2>&1
}
installed_run() { # installed_run [args]: the installed copy's own script
    env -u XDG_DATA_HOME -u STOKESFORGE_INSTALL_ROOT HOME="$H" PATH="$MINBIN" \
        "$BASH_BIN" "$H/.local/share/stokesforge/current/install.sh" "$@" </dev/null >"$tmp/out" 2>&1
}
tree() { (cd "$1" && find . | sort); }
files() { (cd "$1" && find . -type f | sort); }

# Fake archive: folder with an executable fake application and the installer.
mkarchive() { # mkarchive <name> <version> [icon]
    local d="$tmp/archives/$1"
    mkdir -p "$d/lib/python"
    cat > "$d/stokesforge-app" <<APP
#!/bin/sh
printf '%s\n' "\$@" >> "\${STF_LOG:-/dev/null}"
if [ "\${1:-}" = --version ]; then echo "stokesforge-app $2"; exit 0; fi
exit 0
APP
    chmod +x "$d/stokesforge-app"
    : > "$d/lib/python/x.py"
    : > "$d/LICENSE.txt"
    cp "$INSTALLER" "$d/install.sh"
    chmod +x "$d/install.sh"
    if [ "${3:-}" = icon ]; then
        mkdir -p "$d/share/icons/hicolor/48x48/apps"
        : > "$d/share/icons/hicolor/48x48/apps/stokesforge.png"
    fi
    ARCH="$d"
}

desktop_exec() { # the Exec line of a .desktop file, decoded per the spec, with %f -> 'a b.stf'
    local line cmd
    line="$(grep '^Exec=' "$1")"
    cmd="${line#Exec=}"
    cmd="${cmd//%f/\'a b.stf\'}"
    eval "$cmd"
}

real_archive_mode() { # --archive <tar.gz>
    local archive="$1" version top
    [ -f "$archive" ] || { echo "no such archive: $archive" >&2; exit 1; }
    version="$(basename "$archive")"
    version="${version#StokesForge-}"
    version="${version%%-linux*}"
    version="${version%%-rc*}"
    mkdir -p "$tmp/real"
    tar -xzf "$archive" -C "$tmp/real"
    top="$(find "$tmp/real" -mindepth 1 -maxdepth 1 -type d | head -n1)"
    t "the archive carries install.sh" test -x "$top/install.sh"
    new_home real
    export STF_LOG="$tmp/stf.log"
    env -u XDG_DATA_HOME HOME="$H" "$top/install.sh" >"$tmp/out" 2>&1 || { cat "$tmp/out" >&2; fail=1; }
    t "real archive: the command answers its version" test "$("$H/.local/bin/stokesforge" --version)" = "stokesforge-app $version"
    t "real archive: menu entry" test -f "$H/.local/share/applications/stokesforge.desktop"
    if [ -n "$DESKTOP_VALIDATE" ]; then
        t "real archive: desktop-file-validate" "$DESKTOP_VALIDATE" "$H/.local/share/applications/stokesforge.desktop"
    fi
    env -u XDG_DATA_HOME HOME="$H" "$H/.local/share/stokesforge/current/install.sh" --uninstall >"$tmp/out" 2>&1 || { cat "$tmp/out" >&2; fail=1; }
    # Caches written by the system's own update-*-database tools are not ours.
    t "real archive: uninstall leaves no file" test -z "$(files "$H" | grep -v -e '/share/mime/' -e 'icon-theme.cache' -e 'mimeinfo.cache')"
    finish
}

if [ "${1:-}" = --archive ]; then
    real_archive_mode "${2:?usage: --archive <tar.gz>}"
fi

mkarchive a123 1.2.3; A123="$ARCH"
mkarchive a130 1.3.0; A130="$ARCH"
mkarchive a140 1.4.0; A140="$ARCH"
mkarchive a110 1.1.0; A110="$ARCH"
export STF_LOG="$tmp/stf.log"

# --- fresh install ---
new_home fresh
S="$H/.local/share"
t "fresh install succeeds" inst "$A123"
t "version folder" test -x "$S/stokesforge/1.2.3/stokesforge-app"
t "full folder copied" test -f "$S/stokesforge/1.2.3/lib/python/x.py"
t "current is a relative link to 1.2.3" test "$(readlink "$S/stokesforge/current")" = 1.2.3
t "command answers --version" test "$("$H/.local/bin/stokesforge" --version)" = "stokesforge-app 1.2.3"
t "desktop: MimeType" grep -qx 'MimeType=application/x-stokesforge-project;' "$S/applications/stokesforge.desktop"
t "desktop: name" grep -qx 'Name=StokesForge' "$S/applications/stokesforge.desktop"
t "desktop: category" grep -qx 'Categories=Science;' "$S/applications/stokesforge.desktop"
t "desktop: no icon line without icon" bash -c '! grep -q "^Icon=" "$1"' _ "$S/applications/stokesforge.desktop"
t "no icons folder without icon" test ! -e "$S/icons"
t "mime: glob *.stf" grep -q '<glob pattern="\*.stf"/>' "$S/mime/packages/stokesforge.xml"
t "mime: one glob only" test "$(grep -c '<glob' "$S/mime/packages/stokesforge.xml")" = 1
t "mime: comment" grep -q '<comment>StokesForge project</comment>' "$S/mime/packages/stokesforge.xml"
: > "$STF_LOG"
t "Exec runs the program with the file" desktop_exec "$S/applications/stokesforge.desktop"
t "Exec passes 'a b.stf' as one argument" grep -qx 'a b.stf' "$STF_LOG"
if [ -n "$DESKTOP_VALIDATE" ]; then
    t "desktop-file-validate" "$DESKTOP_VALIDATE" "$S/applications/stokesforge.desktop"
fi
t "PATH hint" grep -q 'not in your PATH' "$tmp/out"
t "no optional tool on PATH: no warning" bash -c '! grep -q warning "$1"' _ "$tmp/out"

# --- reinstall of the same version ---
before="$(tree "$H")"
t "reinstall succeeds" inst "$A123"
t "reinstall leaves an identical tree" test "$before" = "$(tree "$H")"
t "no nested or temporary folder" test -z "$(find "$S/stokesforge" -name '.*' -o -path '*/1.2.3/1.2.3' | head -n1)"

# --- refuses to install from its own installed copy ---
tn "source == destination refused" installed_run

# --- default update removes the previous version ---
t "update to 1.3.0" inst "$A130"
t "1.2.3 removed" test ! -e "$S/stokesforge/1.2.3"
t "current -> 1.3.0" test "$(readlink "$S/stokesforge/current")" = 1.3.0
t "no stokesforge-1.2.3 command" test ! -e "$H/.local/bin/stokesforge-1.2.3"
t "no stokesforge-1.2.3 entry" test ! -e "$S/applications/stokesforge-1.2.3.desktop"
t "command answers 1.3.0" test "$("$H/.local/bin/stokesforge" --version)" = "stokesforge-app 1.3.0"

# --- --keep-previous ---
new_home keep
S="$H/.local/share"
inst "$A123" >/dev/null
t "update with --keep-previous" inst "$A130" --keep-previous
t "1.2.3 folder kept" test -x "$S/stokesforge/1.2.3/stokesforge-app"
t "kept command answers 1.2.3" test "$("$H/.local/bin/stokesforge-1.2.3" --version)" = "stokesforge-app 1.2.3"
t "kept command targets the versioned folder" test "$(readlink "$H/.local/bin/stokesforge-1.2.3")" = "$S/stokesforge/1.2.3/stokesforge-app"
t "kept entry named with its version" grep -qx 'Name=StokesForge 1.2.3' "$S/applications/stokesforge-1.2.3.desktop"
t "kept entry runs the versioned folder" grep -q "^Exec=\"$S/stokesforge/1.2.3/stokesforge-app\"" "$S/applications/stokesforge-1.2.3.desktop"
t "main command is the newest" test "$("$H/.local/bin/stokesforge" --version)" = "stokesforge-app 1.3.0"
t "main entry is plain StokesForge" grep -qx 'Name=StokesForge' "$S/applications/stokesforge.desktop"
t "default update afterwards" inst "$A140"
t "kept 1.2.3 stays" test -x "$S/stokesforge/1.2.3/stokesforge-app"
t "previous current 1.3.0 removed" test ! -e "$S/stokesforge/1.3.0"
t "kept command stays" test -L "$H/.local/bin/stokesforge-1.2.3"
t "no stokesforge-1.3.0 command" test ! -e "$H/.local/bin/stokesforge-1.3.0"
t "no stokesforge-1.3.0 entry" test ! -e "$S/applications/stokesforge-1.3.0.desktop"

new_home keepsame
S="$H/.local/share"
inst "$A123" >/dev/null
t "--keep-previous on the same version" inst "$A123" --keep-previous
t "same version: one folder" test "$(find "$S/stokesforge" -mindepth 1 -maxdepth 1 | wc -l)" = 2
t "same version: no versioned command" test ! -e "$H/.local/bin/stokesforge-1.2.3"
t "same version: no versioned entry" test ! -e "$S/applications/stokesforge-1.2.3.desktop"

# --- older version, uninstall of one version ---
new_home older
S="$H/.local/share"
inst "$A123" >/dev/null
tn "older version without terminal and without --yes is refused" inst "$A110"
t "the refusal names the newer version" grep -q '1.2.3' "$tmp/out"
t "nothing installed by the refusal" test ! -e "$S/stokesforge/1.1.0"
t "older version with --yes" inst "$A110" --yes
t "installed alongside" test -x "$S/stokesforge/1.1.0/stokesforge-app"
t "current unchanged" test "$(readlink "$S/stokesforge/current")" = 1.2.3
t "stokesforge still the newest" test "$("$H/.local/bin/stokesforge" --version)" = "stokesforge-app 1.2.3"
t "own command" test "$("$H/.local/bin/stokesforge-1.1.0" --version)" = "stokesforge-app 1.1.0"
t "own entry" grep -qx 'Name=StokesForge 1.1.0' "$S/applications/stokesforge-1.1.0.desktop"
tn "--uninstall of the current version refused" installed_run --uninstall 1.2.3
t "current still there" test -x "$S/stokesforge/1.2.3/stokesforge-app"
t "--uninstall 1.1.0" installed_run --uninstall 1.1.0
t "only that version removed" test ! -e "$S/stokesforge/1.1.0"
t "its command removed" test ! -e "$H/.local/bin/stokesforge-1.1.0"
t "its entry removed" test ! -e "$S/applications/stokesforge-1.1.0.desktop"
t "the current version stays" test "$("$H/.local/bin/stokesforge" --version)" = "stokesforge-app 1.2.3"
tn "--uninstall of an absent version fails" installed_run --uninstall 9.9.9

# --- recovery copies, preferences and projects survive ---
new_home data
S="$H/.local/share"
mkdir -p "$S/stokesforge/recovery" "$H/.config/stokesforge"
echo r > "$S/stokesforge/recovery/r.stf"
echo p > "$H/.config/stokesforge/preferences.json"
echo project > "$H/proj.stf"
inst "$A123" >/dev/null
t "update keeps recovery" inst "$A130"
t "recovery copy kept by an update" test -f "$S/stokesforge/recovery/r.stf"
t "update keeps preferences" test -f "$H/.config/stokesforge/preferences.json"
t "uninstall from the installed copy exits 0" installed_run --uninstall
t "recovery kept by uninstall" test -f "$S/stokesforge/recovery/r.stf"
t "preferences kept by uninstall" test -f "$H/.config/stokesforge/preferences.json"
t "project kept" test -f "$H/proj.stf"
t "everything else removed" test "$(files "$H")" = "$(printf '%s\n' ./.config/stokesforge/preferences.json ./.local/share/stokesforge/recovery/r.stf ./proj.stf)"
t "no stokesforge command left" test -z "$(find "$H/.local/bin" -name 'stokesforge*')"

# --- XDG_DATA_HOME ---
new_home xdg
XDG="$tmp/xdg data"
t "install with XDG_DATA_HOME" env -u STOKESFORGE_INSTALL_ROOT HOME="$H" XDG_DATA_HOME="$XDG" PATH="$MINBIN" "$BASH_BIN" "$A123/install.sh"
t "installed under XDG_DATA_HOME" test -x "$XDG/stokesforge/1.2.3/stokesforge-app"
t "entry under XDG_DATA_HOME" test -f "$XDG/applications/stokesforge.desktop"
t "Exec under XDG_DATA_HOME" grep -q "^Exec=\"$XDG/stokesforge/current/stokesforge-app\"" "$XDG/applications/stokesforge.desktop"
t "nothing under HOME/.local/share" test ! -e "$H/.local/share"
t "command still in HOME/.local/bin" test "$("$H/.local/bin/stokesforge" --version)" = "stokesforge-app 1.2.3"
t "uninstall with XDG_DATA_HOME" env HOME="$H" XDG_DATA_HOME="$XDG" PATH="$MINBIN" "$BASH_BIN" "$XDG/stokesforge/current/install.sh" --uninstall
t "XDG uninstall leaves no file" test -z "$(files "$XDG")"

# --- optional tools ---
new_home tools
S="$H/.local/share"
FAKETOOLS="$tmp/faketools"
mkdir -p "$FAKETOOLS"
for tool in update-desktop-database update-mime-database gtk-update-icon-cache; do
    printf '#!/bin/sh\necho "%s $*" >> "%s"\nexit "${FAKE_TOOL_EXIT:-0}"\n' "$tool" "$tmp/tools.log" > "$FAKETOOLS/$tool"
    chmod +x "$FAKETOOLS/$tool"
done
mkarchive a123i 1.2.3 icon; A123I="$ARCH"
: > "$tmp/tools.log"
EXTRA_PATH="$FAKETOOLS" inst "$A123I" >/dev/null
t "update-desktop-database called on the applications folder" grep -qx "update-desktop-database $S/applications" "$tmp/tools.log"
t "update-mime-database called on the mime folder" grep -qx "update-mime-database $S/mime" "$tmp/tools.log"
t "gtk-update-icon-cache called on the icon theme" grep -qx "gtk-update-icon-cache -f -t $S/icons/hicolor" "$tmp/tools.log"
t "icon copied" test -f "$S/icons/hicolor/48x48/apps/stokesforge.png"
t "desktop: Icon line with an icon" grep -qx 'Icon=stokesforge' "$S/applications/stokesforge.desktop"
if [ -n "$DESKTOP_VALIDATE" ]; then
    t "desktop-file-validate with icon" "$DESKTOP_VALIDATE" "$S/applications/stokesforge.desktop"
fi
FAKE_TOOL_EXIT=1 EXTRA_PATH="$FAKETOOLS" inst "$A130" >/dev/null && rc=0 || rc=$?
t "failing optional tools do not fail the install" test "$rc" = 0
t "a warning is printed" grep -q 'warning' "$tmp/out"
installed_run --uninstall >/dev/null
t "uninstall removes the icon" test -z "$(find "$S/icons" -type f 2>/dev/null)"

# --- broken archives ---
new_home broken
mkarchive noapp 1.2.3; rm "$ARCH/stokesforge-app"; NOAPP="$ARCH"
mkarchive garbage 1.2.3; printf '#!/bin/sh\necho nonsense\n' > "$ARCH/stokesforge-app"; GARBAGE="$ARCH"
mkarchive failing 1.2.3; printf '#!/bin/sh\nexit 3\n' > "$ARCH/stokesforge-app"; FAILING="$ARCH"
tn "archive without the application" inst "$NOAPP"
tn "--version answering garbage" inst "$GARBAGE"
tn "--version failing" inst "$FAILING"
t "broken archives install nothing" test -z "$(tree "$H" | grep -v '^\.$')"

# --- --system under a test root ---
new_home system
ROOT="$tmp/root"
sysinst() { # sysinst <archive> [args]
    local a="$1"
    shift
    env -u XDG_DATA_HOME HOME="$H" STOKESFORGE_INSTALL_ROOT="$ROOT" PATH="$MINBIN${EXTRA_PATH:+:$EXTRA_PATH}" \
        "$BASH_BIN" "$a/install.sh" --system "$@" </dev/null >"$tmp/out" 2>&1
}
t "--system under a test root" sysinst "$A123"
t "system: version folder" test -x "$ROOT/opt/stokesforge/1.2.3/stokesforge-app"
t "system: current" test "$(readlink "$ROOT/opt/stokesforge/current")" = 1.2.3
t "system: command answers --version" test "$("$ROOT/usr/local/bin/stokesforge" --version)" = "stokesforge-app 1.2.3"
t "system: entry" grep -q "^Exec=\"$ROOT/opt/stokesforge/current/stokesforge-app\"" "$ROOT/usr/local/share/applications/stokesforge.desktop"
t "system: mime" test -f "$ROOT/usr/local/share/mime/packages/stokesforge.xml"
t "system: nothing in HOME" test -z "$(tree "$H" | grep -v '^\.$')"
t "system: --keep-previous" sysinst "$A130" --keep-previous
t "system: kept version" test "$("$ROOT/usr/local/bin/stokesforge-1.2.3" --version)" = "stokesforge-app 1.2.3"
t "system: --uninstall 1.2.3" sysinst "$A130" --uninstall 1.2.3
t "system: kept version removed" test ! -e "$ROOT/opt/stokesforge/1.2.3"
t "system: --uninstall" sysinst "$A130" --uninstall
t "system: uninstall empties everything" test -z "$(files "$ROOT")"
t "system: uninstall removes the mime file" test ! -e "$ROOT/usr/local/share/mime/packages/stokesforge.xml"

# --- --system refuses a folder owned by a package ---
for pm in rpm dpkg; do
    FAKEPM="$tmp/fake-$pm"
    mkdir -p "$FAKEPM"
    printf '#!/bin/sh\nexit 0\n' > "$FAKEPM/$pm"
    chmod +x "$FAKEPM/$pm"
    ROOT="$tmp/root-$pm"
    EXTRA_PATH="$FAKEPM" sysinst "$A123" && rc=0 || rc=$?
    t "$pm owning the folder: install refused" test "$rc" != 0
    t "$pm owning the folder: nothing written" test ! -e "$ROOT"
    printf '#!/bin/sh\nexit 1\n' > "$FAKEPM/$pm"
    EXTRA_PATH="$FAKEPM" sysinst "$A123" && rc=0 || rc=$?
    t "$pm not owning the folder: install OK" test "$rc" = 0
done

# --- --system without root and without the test variable ---
if [ "$(id -u)" != 0 ]; then
    rc=0
    env -u STOKESFORGE_INSTALL_ROOT HOME="$H" PATH="$MINBIN" "$BASH_BIN" "$A123/install.sh" --system </dev/null >"$tmp/out" 2>&1 || rc=$?
    t "--system as a normal user is refused" test "$rc" != 0
    t "the refusal says to use sudo" grep -q 'sudo' "$tmp/out"
fi

# --- the whole body sits in main, called on the last line ---
new_home truncated
mkdir -p "$tmp/trunc"
head -n -1 "$INSTALLER" > "$tmp/trunc/install.sh"
t "a truncated script (no last line) does nothing" env HOME="$H" PATH="$MINBIN" "$BASH_BIN" "$tmp/trunc/install.sh" --uninstall
t "a truncated script writes nothing" test -z "$(tree "$H" | grep -v '^\.$')"

# --- options ---
rc=0
env HOME="$H" PATH="$MINBIN" "$BASH_BIN" "$A123/install.sh" --bogus >"$tmp/out" 2>&1 || rc=$?
t "unknown option: exit 2" test "$rc" = 2
t "unknown option: usage" grep -q 'Usage' "$tmp/out"
t "--help exits 0" env HOME="$H" PATH="$MINBIN" "$BASH_BIN" "$A123/install.sh" --help

if command -v shellcheck >/dev/null 2>&1; then
    # Not through `t`: the findings are the useful part of a failure.
    for f in "$INSTALLER" "${BASH_SOURCE[0]}"; do
        count=$((count + 1))
        if ! shellcheck "$f" >&2; then
            echo "FAILED: shellcheck: $f" >&2
            fail=1
        fi
    done
fi
finish
