#!/usr/bin/env bash
# Test of scripts/logo-dist.sh (#388), with a fake generator: nothing here reads
# the real logo. Also checks the opt-in of the CI and that no generated raster
# is committed. Run by the job `fmt`.
# SC2016: `bash -c '...' _ args` expands its arguments inside, on purpose.
# SC2317/SC2329: the helpers are called through `t` / `tn`.
# shellcheck disable=SC2016,SC2317,SC2329
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$HERE")"
SCRIPT="$HERE/logo-dist.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

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

# Fake logo folder: the two sources.
LOGO="$tmp/logo"
mkdir -p "$LOGO/00-final"
echo '<svg/>' > "$LOGO/00-final/stokesforge-icone-clair.svg"
echo '<svg/>' > "$LOGO/00-final/stokesforge-fichier-stf-clair.svg"

# Fake generator: records its call and writes the layout of the real one.
GEN="$tmp/fake-gen"
cat > "$GEN" <<'SH'
#!/usr/bin/env bash
out=""
while [ $# -gt 0 ]; do
    case "$1" in --out) out="$2"; shift ;; esac
    shift
done
echo called >> "$out/../gen-called"
for s in 16 256; do
    mkdir -p "$out/hicolor/${s}x${s}/apps" "$out/hicolor/${s}x${s}/mimetypes"
    : > "$out/hicolor/${s}x${s}/apps/stokesforge.png"
    : > "$out/hicolor/${s}x${s}/mimetypes/application-x-stokesforge-project.png"
done
echo ico > "$out/stokesforge.ico"
echo ico > "$out/stokesforge-project.ico"
SH
chmod +x "$GEN"

run() { # run <opt-in value> <args...>
    local optin="$1"
    shift
    env STOKESFORGE_DIST_LOGO="$optin" STOKESFORGE_LOGO_DIR="$LOGO" \
        STOKESFORGE_LOGO_OUT="$tmp/out/gen" STOKESFORGE_LOGO_GEN="$GEN" "$SCRIPT" "$@"
}

# --- no opt-in: nothing is written, the generator is not called ---
mkdir -p "$tmp/out" "$tmp/pkg-none"
t "no opt-in: generate succeeds" run 0 generate
t "no opt-in: install succeeds" run 0 install "$tmp/pkg-none" linux
t "no opt-in: app-ico succeeds" run 0 app-ico
t "no opt-in: generator not called" test ! -e "$tmp/out/gen-called"
t "no opt-in: nothing generated" test ! -e "$tmp/out/gen"
t "no opt-in: package untouched" test -z "$(ls -A "$tmp/pkg-none")"
t "no opt-in even without the logo folder" env STOKESFORGE_DIST_LOGO=0 STOKESFORGE_LOGO_DIR="$tmp/absent" \
    STOKESFORGE_LOGO_OUT="$tmp/out/gen" STOKESFORGE_LOGO_GEN="$GEN" "$SCRIPT" generate

# --- opt-in with assets/logo/ removed: the build stops ---
tn "opt-in without the logo folder fails" env STOKESFORGE_DIST_LOGO=1 STOKESFORGE_LOGO_DIR="$tmp/absent" \
    STOKESFORGE_LOGO_OUT="$tmp/out/gen2" STOKESFORGE_LOGO_GEN="$GEN" "$SCRIPT" generate
tn "opt-in without the logo folder fails to install" env STOKESFORGE_DIST_LOGO=1 STOKESFORGE_LOGO_DIR="$tmp/absent" \
    STOKESFORGE_LOGO_OUT="$tmp/out/gen" STOKESFORGE_LOGO_GEN="$GEN" "$SCRIPT" install "$tmp/pkg-none" linux

# --- opt-in, Linux ---
t "opt-in: generate" run 1 generate
t "opt-in: the generator ran" test -e "$tmp/out/gen-called"
mkdir -p "$tmp/pkg-linux"
t "opt-in: install linux" run 1 install "$tmp/pkg-linux" linux
P="$tmp/pkg-linux/share/icons/hicolor"
t "linux: apps png" test -f "$P/256x256/apps/stokesforge.png"
t "linux: mimetypes png" test -f "$P/256x256/mimetypes/application-x-stokesforge-project.png"
t "linux: small size too" test -f "$P/16x16/apps/stokesforge.png"
t "linux: scalable app svg" test -f "$P/scalable/apps/stokesforge.svg"
t "linux: scalable mime svg" test -f "$P/scalable/mimetypes/application-x-stokesforge-project.svg"
t "linux: no .ico in the package" test -z "$(find "$tmp/pkg-linux" -name '*.ico')"

# --- opt-in, Windows: only the 256 png and the project .ico ---
mkdir -p "$tmp/pkg-win"
t "opt-in: install windows" run 1 install "$tmp/pkg-win" windows
t "windows: 256 png" test -f "$tmp/pkg-win/share/icons/hicolor/256x256/apps/stokesforge.png"
t "windows: project ico" test -f "$tmp/pkg-win/stokesforge-project.ico"
t "windows: only the 256 png and the project ico" test "$(cd "$tmp/pkg-win" && find . -type f | sort | tr '\n' ' ')" = "./share/icons/hicolor/256x256/apps/stokesforge.png ./stokesforge-project.ico "
t "app-ico prints the application ico" test -s "$(run 1 app-ico)"
tn "unknown platform refused" run 1 install "$tmp/pkg-win" macos
tn "unknown command refused" run 1 frobnicate

# --- the CI carries the opt-in in exactly the two release jobs ---
jobs_with_opt_in() { # the jobs of ci.yml that set STOKESFORGE_DIST_LOGO to 1
    awk '/^  [a-z0-9-]+:[[:space:]]*$/ { job = $1 } /STOKESFORGE_DIST_LOGO: *"1"/ { print job }' "$1" | sort -u | tr '\n' ' '
}
t "ci.yml: opt-in in release-linux and dist-windows only" test "$(jobs_with_opt_in "$REPO_ROOT/.github/workflows/ci.yml")" = "dist-windows: release-linux: "
# The witness of the check itself: an opt-in in one more job is seen.
printf 'jobs:\n  fmt:\n    env:\n      STOKESFORGE_DIST_LOGO: "1"\n' > "$tmp/fake-ci.yml"
t "the opt-in check sees a third job" test "$(jobs_with_opt_in "$tmp/fake-ci.yml")" = "fmt: "

# --- nothing generated is committed ---
t "no .ico committed" test -z "$(git -C "$REPO_ROOT" ls-files '*.ico' )"
t "no hicolor raster committed" test -z "$(git -C "$REPO_ROOT" ls-files | grep -E '(^|/)hicolor/.*\.png$' || true)"

if [ "$fail" != 0 ]; then
    echo "logo-dist-test: FAILED ($count checks)" >&2
    exit 1
fi
echo "logo-dist-test: $count checks OK"
