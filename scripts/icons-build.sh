#!/usr/bin/env bash
# Produit les SVG d'icônes de plotyx-app (une variante par icône et par rôle de
# couleur) et la planche-contact. Voir docs/icones.md.
#
#   scripts/icons-build.sh            # variantes seules
#   scripts/icons-build.sh --sheet    # + planche-contact (Inkscape)
#   scripts/icons-build.sh --add NOM  # copie NOM-duotone.svg depuis vendor/
#
# Entrées : assets/icons/src/*.svg (sous-ensemble Phosphor versionné),
# palette.tsv (rôles), icons.tsv (icône → rôles). Sortie : assets/icons/<nom>.<rôle>.svg.
set -euo pipefail
cd "$(dirname "$0")/.."
dir=crates/plotyx-app/assets/icons
out=${ICON_SHEET_DIR:-target/icons}

if [[ ${1:-} == --add ]]; then
  cp "vendor/phosphor-icons/SVGs/duotone/$2-duotone.svg" "$dir/src/"
  echo "copié : $2 — ajouter une ligne à $dir/icons.tsv"
  exit 0
fi

tab=$'\t'
declare -A stroke fill
while IFS="$tab" read -r role s f; do
  [[ -z $role || $role == \#* ]] && continue
  stroke[$role]=$s; fill[$role]=$f
done < "$dir/palette.tsv"

rm -f "$dir"/*.*.svg
variants=()
while IFS="$tab" read -r name roles; do
  [[ -z $name || $name == \#* ]] && continue
  for role in ${roles//,/ }; do
    [[ -n ${stroke[$role]:-} ]] || { echo "rôle inconnu : $role ($name)" >&2; exit 1; }
    sed -e "s/currentColor/${stroke[$role]}/g" \
        -e "s/opacity=\"0.2\"/fill=\"${fill[$role]}\" opacity=\"0.2\"/g" \
        "$dir/src/$name-duotone.svg" > "$dir/$name.$role.svg"
    variants+=("$name.$role")
  done
done < "$dir/icons.tsv"
echo "${#variants[@]} variantes dans $dir"

[[ ${1:-} == --sheet ]] || exit 0
command -v inkscape >/dev/null || { echo "inkscape introuvable" >&2; exit 1; }
mkdir -p "$out"
cell=96 cols=8
rows=$(( (${#variants[@]} + cols - 1) / cols ))
w=$(( cols * cell )) h=$(( rows * (cell + 16) ))
# Même planche en couleur et en niveaux de gris : la forme doit suffire.
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
echo "planche-contact : $out/sheet-color.png, $out/sheet-gray.png"
