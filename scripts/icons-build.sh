#!/usr/bin/env bash
# Produces the icon SVGs of stokesforge-app (one variant per icon and per color
# role) and the contact sheet. See docs/icons.md.
#
#   scripts/icons-build.sh            # variants only
#   scripts/icons-build.sh --sheet    # + contact sheet (Inkscape)
#   scripts/icons-build.sh --add NAME # copies NAME-duotone.svg from vendor/
#
# Inputs: assets/icons/src/*.svg (versioned Phosphor subset), palette.tsv
# (roles), icons.tsv (icon → roles). Output: assets/icons/<name>.<role>.svg.
set -euo pipefail
cd "$(dirname "$0")/.."
dir=crates/stokesforge-app/assets/icons
out=${ICON_SHEET_DIR:-target/icons}

if [[ ${1:-} == --add ]]; then
  src="vendor/phosphor-icons/SVGs/duotone/${2:?usage: --add NAME}-duotone.svg"
  [[ -f $src ]] || { echo "not found: $src" >&2; exit 1; }
  cp "$src" "$dir/src/"
  echo "copied: $2; add a line to $dir/icons.tsv"
  exit 0
fi

tab=$'\t'
declare -A stroke fill
while IFS="$tab" read -r role s f; do
  [[ -z $role || $role == \#* ]] && continue
  stroke[$role]=$s; fill[$role]=$f
done < "$dir/palette.tsv"

# Check everything before touching anything: all roles known, all sources
# present (all problems are reported), then bake into a temporary directory
# and swap it in only once every variant succeeded.
bad=0
while IFS="$tab" read -r name roles; do
  [[ -z $name || $name == \#* ]] && continue
  [[ -f $dir/src/$name-duotone.svg ]] || { echo "missing source: $dir/src/$name-duotone.svg" >&2; bad=1; }
  for role in ${roles//,/ }; do
    [[ -n ${stroke[$role]:-} ]] || { echo "unknown role: $role ($name)" >&2; bad=1; }
  done
done < "$dir/icons.tsv"
(( bad == 0 )) || { echo "nothing replaced in $dir" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
variants=()
while IFS="$tab" read -r name roles; do
  [[ -z $name || $name == \#* ]] && continue
  for role in ${roles//,/ }; do
    sed -e "s/currentColor/${stroke[$role]}/g" \
        -e "s/opacity=\"0.2\"/fill=\"${fill[$role]}\" opacity=\"0.2\"/g" \
        "$dir/src/$name-duotone.svg" > "$tmp/$name.$role.svg"
    variants+=("$name.$role")
  done
done < "$dir/icons.tsv"
rm -f "$dir"/*.*.svg
mv "$tmp"/*.svg "$dir/"
echo "${#variants[@]} variants in $dir"

[[ ${1:-} == --sheet ]] || exit 0
command -v inkscape >/dev/null || { echo "inkscape not found" >&2; exit 1; }
mkdir -p "$out"
cell=96 cols=8
rows=$(( (${#variants[@]} + cols - 1) / cols ))
w=$(( cols * cell )) h=$(( rows * (cell + 16) ))
# Same sheet in color and in grayscale: the shape must be enough.
for mode in color gray; do
  {
    echo "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"$w\" height=\"$h\">"
    echo "<defs><filter id=\"g\"><feColorMatrix type=\"saturate\" values=\"0\"/></filter></defs>"
    echo "<rect width=\"$w\" height=\"$h\" fill=\"#F5F5F5\"/>"
    [[ $mode == gray ]] && echo "<g filter=\"url(#g)\">" || echo "<g>"
    i=0
    for v in "${variants[@]}"; do
      x=$(( (i % cols) * cell )) y=$(( (i / cols) * (cell + 16) ))
      echo "<image x=\"$((x+24))\" y=\"$((y+4))\" width=\"48\" height=\"48\" href=\"$PWD/$dir/$v.svg\"/>"
      echo "<image x=\"$((x+36))\" y=\"$((y+56))\" width=\"24\" height=\"24\" href=\"$PWD/$dir/$v.svg\"/>"
      echo "<text x=\"$((x+48))\" y=\"$((y+94))\" font-size=\"8\" font-family=\"sans-serif\" text-anchor=\"middle\">$v</text>"
      i=$((i+1))
    done
    echo "</g></svg>"
  } > "$out/sheet-$mode.svg"
  inkscape "$out/sheet-$mode.svg" --export-type=png --export-filename="$out/sheet-$mode.png" >/dev/null 2>&1
done
echo "contact sheet: $out/sheet-color.png, $out/sheet-gray.png"
