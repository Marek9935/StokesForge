#!/usr/bin/env bash
# Install test of a StokesForge RPM or DEB (#390), run as root INSIDE a
# container (fedora, debian, ubuntu), by the CI job `test-linux-packages` and by
# hand with podman:
#   podman run --rm -v "$PWD:/repo:ro" -v <folder of the files>:/p:ro fedora:42 \
#       /repo/scripts/linux-packages-install-test.sh /p/StokesForge-X.Y.Z-linux-x86_64.rpm /p/StokesForge-X.Y.Z-linux-x86_64.tar.gz
# Environment: STOKESFORGE_PKG_TEST_NOWINDOW=1 skips the window (fake
# application of the local trials); STOKESFORGE_PKG_TEST_NOLINT=1 skips
# rpmlint / lintian.
set -euo pipefail

PKG="${1:?usage: linux-packages-install-test.sh <package.rpm|.deb> <archive.tar.gz>}"
ARCHIVE="${2:?usage: linux-packages-install-test.sh <package.rpm|.deb> <archive.tar.gz>}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[ "$(id -u)" = 0 ] || { echo "run as root, in a container" >&2; exit 1; }

case "$PKG" in
    *.rpm) FMT=rpm ;;
    *.deb) FMT=deb ;;
    *) echo "unexpected package $PKG" >&2; exit 1 ;;
esac
version="$(basename "$PKG")"
version="${version#StokesForge-}"
version="${version%%-linux*}"
version="${version%%-rc*}"

step() { printf '\n== %s\n' "$*"; }
ok() { printf 'ok: %s\n' "$*"; }
die() { printf 'FAILED: %s\n' "$*" >&2; exit 1; }

if [ "$FMT" = rpm ]; then
    pm_install() { dnf install -y "$@"; }
    pm_remove() { dnf remove -y stokesforge; }
else
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    pm_install() { apt-get install -y --no-install-recommends "$@"; }
    pm_remove() { apt-get remove -y stokesforge; }
fi

# The distribution's own triggers refresh the menu, MIME and icon databases:
# they must be there before the package, as on a desktop.
step "tools of a desktop"
if [ "$FMT" = rpm ]; then
    pm_install desktop-file-utils shared-mime-info gtk-update-icon-cache >/dev/null
else
    pm_install desktop-file-utils shared-mime-info >/dev/null
fi

step "install with the package dependencies only"
pm_install "$PKG"
[ -x /usr/bin/stokesforge ] || die "/usr/bin/stokesforge missing"
[ "$(stokesforge --version)" = "stokesforge-app $version" ] || die "version answer: $(stokesforge --version)"
ok "stokesforge --version"
desktop-file-validate /usr/share/applications/stokesforge.desktop
ok "desktop-file-validate"
grep -q '^application/x-stokesforge-project=.*stokesforge.desktop' /usr/share/applications/mimeinfo.cache \
    || die "mimeinfo.cache does not map the type to stokesforge.desktop"
grep -q '\*\.stf' /usr/share/mime/globs2 || die "*.stf missing from the MIME globs"
ok "menu entry and .stf type registered by the distribution triggers"
if [ "$FMT" = rpm ]; then rpm -qf /opt/stokesforge >/dev/null; else dpkg -S /opt/stokesforge >/dev/null; fi
ok "the package owns /opt/stokesforge"

step "window under a virtual screen, Mesa software Vulkan"
if [ "${STOKESFORGE_PKG_TEST_NOWINDOW:-}" = 1 ]; then
    echo "skipped (STOKESFORGE_PKG_TEST_NOWINDOW=1)"
else
    if [ "$FMT" = rpm ]; then
        pm_install xorg-x11-server-Xvfb xauth mesa-vulkan-drivers >/dev/null
    else
        pm_install xvfb xauth mesa-vulkan-drivers >/dev/null
    fi
    cfg="$(mktemp -d)"
    xvfb-run -a sh -c 'env -i PATH=/usr/bin:/bin HOME=/tmp DISPLAY="$DISPLAY" XAUTHORITY="${XAUTHORITY:-}" STOKESFORGE_CONFIG_DIR="$1" STOKESFORGE_APP_AUTOQUIT_MS=1500 /usr/bin/stokesforge' _ "$cfg" \
        || die "the window did not open and close (exit code $?)"
    rm -rf "$cfg"
    ok "window opened and closed with exit 0"
fi

step "selftest as root, then removal leaves nothing"
/opt/stokesforge/stokesforge-selftest > /dev/null
ok "stokesforge-selftest"

step "the installer of the archive refuses a folder owned by the package"
work="$(mktemp -d)"
tar -xzf "$ARCHIVE" -C "$work"
inst="$(echo "$work"/StokesForge-*/install.sh)"
if "$inst" --system --yes > "$work/out" 2>&1; then die "install.sh --system overwrote the package"; fi
grep -q 'belongs to an installed package' "$work/out" || die "unexpected refusal: $(cat "$work/out")"
ok "install.sh --system --yes refused"

step "removal"
pm_remove
[ ! -e /opt/stokesforge ] || die "/opt/stokesforge is still there: $(find /opt/stokesforge | head)"
[ ! -e /usr/bin/stokesforge ] || die "/usr/bin/stokesforge is still there"
[ ! -e /usr/share/applications/stokesforge.desktop ] || die "the menu entry is still there"
[ ! -e /usr/share/mime/packages/stokesforge.xml ] || die "the MIME file is still there"
if [ "$FMT" = deb ]; then
    dpkg -P stokesforge > /dev/null 2>&1 || true
    ! dpkg -l stokesforge 2> /dev/null | grep -q '^[a-z][a-z] ' || die "stokesforge is still known to dpkg after purge"
fi
if [ "$FMT" = rpm ]; then ! grep -q stokesforge /usr/share/applications/mimeinfo.cache 2>/dev/null || die "mimeinfo.cache still names stokesforge"; fi
ok "remove leaves nothing"

step "the package refuses to install over install.sh --system"
"$inst" --system --yes > "$work/out" 2>&1 || die "install.sh --system failed: $(cat "$work/out")"
if pm_install "$PKG" > "$work/out" 2>&1; then die "the package overwrote an install.sh --system installation"; fi
[ -L /opt/stokesforge/current ] || die "the system installation was damaged by the refused package"
"$inst" --system --uninstall --yes > /dev/null 2>&1 || "$inst" --system --uninstall >/dev/null
[ ! -e /opt/stokesforge ] || die "the system installation was not removed"
ok "package refused in preinst, installation intact"

step "reinstall of the package itself is accepted"
pm_install "$PKG"
if [ "$FMT" = rpm ]; then dnf reinstall -y "$PKG" >/dev/null; else apt-get install -y --reinstall "$PKG" >/dev/null; fi
[ "$(stokesforge --version)" = "stokesforge-app $version" ] || die "after reinstall"
pm_remove
ok "reinstall"

if [ "${STOKESFORGE_PKG_TEST_NOLINT:-}" != 1 ]; then
    step "package linter"
    if [ "$FMT" = rpm ]; then
        pm_install rpmlint >/dev/null
        rpmlint -c "$HERE/linux-packages/rpmlint.toml" "$PKG" || die "rpmlint reports errors"
    else
        pm_install lintian >/dev/null
        suppress="$(grep -v '^#' "$HERE/linux-packages/lintian-suppress.txt" | paste -sd, -)"
        lintian --fail-on error --suppress-tags "$suppress" "$PKG" || die "lintian reports errors"
    fi
    ok "lint"
fi
echo
echo "linux-packages-install-test: all steps OK"
