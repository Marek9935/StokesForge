#!/usr/bin/env bash
# Test of the Linux RPM and DEB packages (#390).
#
# Usage: scripts/linux-packages-test.sh          pure part (job `fmt`): no nfpm,
#            a fake nfpm records what the packages script gives it
#        scripts/linux-packages-test.sh --nfpm   real nfpm on a fake distribution,
#            then rpm and dpkg-deb on the packages (job `linux-packages`)
# The container tests of the real packages are in the CI job `test-linux-packages`.
# SC2016: `bash -c '...' _ args` expands its arguments inside, on purpose.
# SC2329 (SC2317 before shellcheck 0.11): the helpers are called through `t` / `tn`.
# shellcheck disable=SC2016,SC2317,SC2329
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$HERE")"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
PKG="$HERE/release-linux-packages.sh"

fail=0
count=0
t() { # t <description> <command...>: the command must succeed
    local desc="$1"
    shift
    count=$((count + 1))
    if ! "$@" > /dev/null 2>&1; then
        echo "FAILED: $desc" >&2
        fail=1
    fi
}
tn() { # tn <description> <command...>: the command must fail
    local desc="$1"
    shift
    count=$((count + 1))
    if "$@" > /dev/null 2>&1; then
        echo "FAILED: $desc" >&2
        fail=1
    fi
}
finish() {
    if [ "$fail" != 0 ]; then
        echo "linux-packages-test: FAILED ($count checks)" >&2
        exit 1
    fi
    echo "linux-packages-test: $count checks OK"
    exit 0
}
has() { grep -qF -- "$1" "$2"; } # fixed text, not a pattern
hasnt() { ! grep -qF -- "$1" "$2"; }

# Fake distribution: fake application, selftest, python folder, notice, and an
# install.sh as `release-linux.sh` leaves it there.
mkdist() { # mkdist <dir> [icon]
    local d="$1"
    mkdir -p "$d/lib/python"
    printf '#!/bin/sh\nif [ "${1:-}" = --version ]; then echo "stokesforge-app 1.2.3"; fi\nexit 0\n' > "$d/stokesforge-app"
    cp "$d/stokesforge-app" "$d/stokesforge-selftest"
    chmod +x "$d/stokesforge-app" "$d/stokesforge-selftest"
    : > "$d/lib/python/x.py"
    : > "$d/LICENSE"
    cp "$HERE/install-linux.sh" "$d/install.sh"
    if [ "${2:-}" = icon ]; then
        mkdir -p "$d/share/icons/hicolor/48x48/apps"
        : > "$d/share/icons/hicolor/48x48/apps/stokesforge.png"
    fi
}

# --- version mapping (pure function) ---
export STOKESFORGE_PKG_SOURCE_ONLY=1
# shellcheck source=release-linux-packages.sh
source "$PKG"
unset STOKESFORGE_PKG_SOURCE_ONLY
t "v1.2.3 -> 1.2.3" test "$(pkg_version v1.2.3)" = 1.2.3
t "v1.2.3-rc4 -> 1.2.3~rc4" test "$(pkg_version v1.2.3-rc4)" = '1.2.3~rc4'
t "1.2.3 without v" test "$(pkg_version 1.2.3)" = 1.2.3
tn "garbage version refused" pkg_version v1.2
tn "other suffix refused" pkg_version v1.2.3-beta1

# --- the packages script, with a fake nfpm that keeps what it is given ---
FAKE="$tmp/fake-nfpm"
cat > "$FAKE" <<'SH'
#!/usr/bin/env bash
# fake nfpm: package --config <file> --packager <fmt> --target <file>
cfg="" fmt="" target=""
while [ $# -gt 0 ]; do
    case "$1" in
        --config) cfg="$2"; shift ;;
        --packager) fmt="$2"; shift ;;
        --target) target="$2"; shift ;;
    esac
    shift
done
snap="$(dirname "$target")/snapshot-$fmt"
rm -rf "$snap"
cp -a "$(dirname "$cfg")" "$snap"
echo "$NFPM_PKG_VERSION" > "$snap/version"
: > "$target"
SH
chmod +x "$FAKE"

run_pkg() { # run_pkg <dist> <out> <tag> [switch]
    NFPM="$FAKE" STOKESFORGE_DIST="$1" STOKESFORGE_PKG_OUT="$2" PUBLISH_LINUX_PACKAGES="${4:-0}" "$PKG" "$3"
}

mkdist "$tmp/d-plain"
t "packages script (no icon, rc tag)" run_pkg "$tmp/d-plain" "$tmp/o-plain" v1.2.3-rc4
snap="$tmp/o-plain/snapshot-rpm"
t "rpm and deb files named after the tag, no ~" test -f "$tmp/o-plain/StokesForge-1.2.3-rc4-linux-x86_64.rpm" -a -f "$tmp/o-plain/StokesForge-1.2.3-rc4-linux-x86_64.deb"
t "nfpm gets the ~ version" test "$(cat "$snap/version")" = '1.2.3~rc4'
t "install.sh is not staged" test ! -e "$snap/opt/stokesforge/install.sh"
t "the program is staged" test -x "$snap/opt/stokesforge/stokesforge-app"
t "python folder staged" test -f "$snap/opt/stokesforge/lib/python/x.py"
t "desktop: MimeType" has 'MimeType=application/x-stokesforge-project;' "$snap/usr/share/applications/stokesforge.desktop"
t "desktop: quoted Exec in /opt" has 'Exec="/opt/stokesforge/stokesforge-app" %f' "$snap/usr/share/applications/stokesforge.desktop"
t "desktop: no Icon= without icon" hasnt 'Icon=' "$snap/usr/share/applications/stokesforge.desktop"
t "mime: glob *.stf" has '<glob pattern="*.stf"/>' "$snap/usr/share/mime/packages/stokesforge.xml"
t "no icons folder without icon" test ! -e "$snap/usr/share/icons"
t "config: no icon entry" hasnt 'hicolor' "$snap/nfpm.yaml"
t "config: placeholders filled" hasnt '@STAGE@' "$snap/nfpm.yaml"
t "config: preinst script path filled" has "$REPO_ROOT/scripts/linux-packages/preinst.sh" "$snap/nfpm.yaml"
t "final tag -> plain version" bash -c 'NFPM="$1" STOKESFORGE_DIST="$2" STOKESFORGE_PKG_OUT="$3" PUBLISH_LINUX_PACKAGES=0 "$4" v1.2.3 && test "$(cat "$3/snapshot-deb/version")" = 1.2.3' _ "$FAKE" "$tmp/d-plain" "$tmp/o-final" "$PKG"

mkdist "$tmp/d-icon" icon
t "packages script (icon)" run_pkg "$tmp/d-icon" "$tmp/o-icon" v1.2.3 1
snap="$tmp/o-icon/snapshot-deb"
t "icon staged under usr/share" test -f "$snap/usr/share/icons/hicolor/48x48/apps/stokesforge.png"
t "desktop: Icon=stokesforge with icon" has 'Icon=stokesforge' "$snap/usr/share/applications/stokesforge.desktop"
t "config: icon entry with its folder" has 'dst: /usr/share/icons/hicolor/48x48/apps/stokesforge.png' "$snap/nfpm.yaml"

tn "publication switch 1 without icon stops" run_pkg "$tmp/d-plain" "$tmp/o-guard" v1.2.3 1
tn "bad tag refused" run_pkg "$tmp/d-plain" "$tmp/o-bad" v1.2
tn "incomplete distribution refused" run_pkg "$tmp/nodist" "$tmp/o-nodist" v1.2.3

# --- the staging mode of the installer: nothing but the staging folder ---
MINBIN="$tmp/minbin"
mkdir -p "$MINBIN" "$tmp/home"
for tool in basename dirname ln cp mv rm sort head tail mkdir readlink rmdir cat id; do
    ln -s "$(command -v "$tool")" "$MINBIN/$tool"
done
BASH_BIN="$(command -v bash)"
t "stage-integration succeeds in a clean environment" env -i HOME="$tmp/home" PATH="$MINBIN" \
    "$BASH_BIN" "$HERE/install-linux.sh" --stage-integration "$tmp/stage-only/usr/share" /opt/stokesforge/stokesforge-app "$tmp/d-icon"
t "stage-integration wrote the desktop file" test -f "$tmp/stage-only/usr/share/applications/stokesforge.desktop"
t "stage-integration copied the icon" test -f "$tmp/stage-only/usr/share/icons/hicolor/48x48/apps/stokesforge.png"
t "stage-integration left HOME empty" test -z "$(ls -A "$tmp/home")"
t "stage-integration wrote only inside the staging folder" test "$(find "$tmp/stage-only" -type f | wc -l)" = 3
tn "stage-integration refuses a quote in the Exec path" env -i HOME="$tmp/home" PATH="$MINBIN" \
    "$BASH_BIN" "$HERE/install-linux.sh" --stage-integration "$tmp/stage-q" '/opt/a"b/stokesforge-app' "$tmp/d-icon"
tn "stage-integration needs three arguments" env -i HOME="$tmp/home" PATH="$MINBIN" \
    "$BASH_BIN" "$HERE/install-linux.sh" --stage-integration "$tmp/stage-n" /opt/x

# --- the release links: rpm and deb only when the switch is 1 ---
ver="$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_ROOT/crates/stokesforge-id/Cargo.toml" | head -n1)"
dry() { # dry <switch>
    STOKESFORGE_RELEASE_DRYRUN=1 PUBLISH_LINUX_PACKAGES="$1" "$HERE/release-publish.sh" "v${ver}-rc9"
}
dry 0 > "$tmp/pub0" 2>&1 || true
dry 1 > "$tmp/pub1" 2>&1 || true
t "publish dry run lists the archive (switch 0)" has "StokesForge-${ver}-rc9-linux-x86_64.tar.gz" "$tmp/pub0"
t "switch 0: no rpm link" hasnt '.rpm' "$tmp/pub0"
t "switch 0: no deb link" hasnt '.deb' "$tmp/pub0"
t "switch 1: rpm link" has "StokesForge-${ver}-rc9-linux-x86_64.rpm" "$tmp/pub1"
t "switch 1: deb link" has "StokesForge-${ver}-rc9-linux-x86_64.deb" "$tmp/pub1"
# release-upload.sh knows both formats and names the producer of a missing file.
mkdir -p "$tmp/repo/scripts" "$tmp/repo/dist"
cp "$HERE/release-upload.sh" "$tmp/repo/scripts/"
for format in rpm deb; do
    t "upload $format: missing file names the packages script" bash -c '"$1" "$2" v1.2.3 2>&1 | grep -qF "x86_64.$2 missing, run scripts/release-linux-packages.sh"' _ "$tmp/repo/scripts/release-upload.sh" "$format"
    : > "$tmp/repo/dist/StokesForge-1.2.3-linux-x86_64.$format"
    t "upload $format: dry run prints the registry URL" bash -c 'STOKESFORGE_RELEASE_DRYRUN=1 "$1" "$2" v1.2.3 | grep -q "/packages/generic/stokesforge/1.2.3/StokesForge-1.2.3-linux-x86_64.$2$"' _ "$tmp/repo/scripts/release-upload.sh" "$format"
done

# --- the real nfpm on a fake distribution, then the package tools ---
if [ "${1:-}" = --nfpm ]; then
    command -v nfpm > /dev/null || { echo "nfpm not on PATH" >&2; exit 1; }
    command -v rpm > /dev/null || { echo "rpm not on PATH" >&2; exit 1; }
    command -v dpkg-deb > /dev/null || { echo "dpkg-deb not on PATH" >&2; exit 1; }
    real="$tmp/real"
    mkdist "$real/dist" icon
    # Old modification times: they must survive in the packages (the .pyc).
    touch -d '2020-01-02 03:04:05' "$real/dist/lib/python/x.py"
    mkdir -p "$real/dist/licenses"
    : > "$real/dist/THIRD-PARTY-LICENSES.txt"
    NFPM=nfpm STOKESFORGE_DIST="$real/dist" STOKESFORGE_PKG_OUT="$real/out" PUBLISH_LINUX_PACKAGES=1 "$PKG" v1.2.3-rc4 > /dev/null
    rpmf="$real/out/StokesForge-1.2.3-rc4-linux-x86_64.rpm"
    debf="$real/out/StokesForge-1.2.3-rc4-linux-x86_64.deb"
    t "rpm built" test -f "$rpmf"
    t "deb built" test -f "$debf"
    dpkg-deb -f "$debf" > "$real/control"
    dpkg-deb -c "$debf" > "$real/deb-list"
    t "deb: version 1.2.3~rc4" grep -q '^Version: 1.2.3~rc4' "$real/control"
    t "deb: amd64" grep -qx 'Architecture: amd64' "$real/control"
    for dep in libvulkan1 libwayland-client0 libwayland-egl1 libxkbcommon0 libxkbcommon-x11-0 libx11-6 libx11-xcb1 libxcb1 libxcursor1 libxi6 libegl1; do
        t "deb depends on $dep" grep -q "^Depends:.*\\b$dep\\b" "$real/control"
    done
    t "deb recommends mesa-vulkan-drivers" grep -qx 'Recommends: mesa-vulkan-drivers' "$real/control"
    t "deb: everything root/root" bash -c '! grep -v " root/root " "$1"' _ "$real/deb-list"
    t "deb: /opt/stokesforge/ is a directory entry" grep -q '^drwxr-xr-x root/root .* \./opt/stokesforge/$' "$real/deb-list"
    t "deb: nothing group or world writable" bash -c '! awk "{print \$1}" "$1" | grep -E "^.{5}w|^.{8}w"' _ "$real/deb-list"
    t "deb: /usr/bin/stokesforge is a link" grep -q '^l.* \./usr/bin/stokesforge -> /opt/stokesforge/stokesforge-app$' "$real/deb-list"
    t "deb: no install.sh" bash -c '! grep -q install.sh "$1"' _ "$real/deb-list"
    t "deb: copyright file" grep -q '\./usr/share/doc/stokesforge/copyright$' "$real/deb-list"
    t "deb: icon" grep -q '\./usr/share/icons/hicolor/48x48/apps/stokesforge.png$' "$real/deb-list"
    t "deb: modification time of the sources kept" grep -q ' 2020-01-02 .* \./opt/stokesforge/lib/python/x.py$' "$real/deb-list"
    rpm -qpR "$rpmf" > "$real/rpm-req"
    for so in 'libvulkan.so.1()(64bit)' 'libwayland-client.so.0()(64bit)' 'libwayland-egl.so.1()(64bit)' 'libxkbcommon.so.0()(64bit)' 'libxkbcommon-x11.so.0()(64bit)' 'libX11.so.6()(64bit)' 'libX11-xcb.so.1()(64bit)' 'libxcb.so.1()(64bit)' 'libXcursor.so.1()(64bit)' 'libXi.so.6()(64bit)' 'libEGL.so.1()(64bit)'; do
        t "rpm requires $so" grep -qxF "$so" "$real/rpm-req"
    done
    t "rpm requires nothing from lib/python" bash -c '! grep -qi python "$1"' _ "$real/rpm-req"
    t "rpm provides no .so" bash -c '! rpm -qp --provides "$1" | grep -q "\.so"' _ "$rpmf"
    t "rpm version" test "$(rpm -qp --qf '%{VERSION}-%{RELEASE}' "$rpmf")" = '1.2.3~rc4-1'
    t "rpm arch" test "$(rpm -qp --qf '%{ARCH}' "$rpmf")" = x86_64
    t "rpm license" test "$(rpm -qp --qf '%{LICENSE}' "$rpmf")" = GPL-3.0-or-later
    rpm -qpl --dump "$rpmf" > "$real/rpm-list"
    t "rpm: /opt/stokesforge is a directory of the package" grep -q '^/opt/stokesforge [0-9]* [0-9]* [0-9a-f]* 04[0-9]* root root' "$real/rpm-list"
    t "rpm: no install.sh" bash -c '! grep -q install.sh "$1"' _ "$real/rpm-list"
    t "rpm: modification time of the sources kept" grep -q "^/opt/stokesforge/lib/python/x.py [0-9]* $(date -d '2020-01-02 03:04:05' +%s) " "$real/rpm-list"
    t "rpm: nothing group or world writable" bash -c '! awk "{print \$5}" "$1" | grep -E "^[0-9]*[2367][0-7]?[2367]?$" | grep -v "^1"' _ "$real/rpm-list"
    t "rpm: preinstall script" bash -c 'rpm -qp --scripts "$1" | grep -q "install.sh --system"' _ "$rpmf"
fi
finish
