---
name: plotyx-textes-interface
description: Écrire ou modifier tout texte visible de PlotYx — libellé de menu, de bouton, de champ, d'onglet, titre de fenêtre, infobulle, message d'état ou d'erreur affiché, nom d'axe ou d'unité. À utiliser dès qu'un littéral destiné à l'utilisateur est ajouté, changé ou traduit, avant d'écrire le texte : le glossaire anglais ↔ français (`docs/textes-interface.md`) est la source unique des termes, des majuscules et du ton. Une notion = un mot, partout.
---

# Textes d'interface

Source unique : [`docs/textes-interface.md`](../../../docs/textes-interface.md)
(glossaire, règles R1-R16, exemples avant/après). Ce skill dit **quand** le
lire et **dans quel ordre** s'en servir ; il ne recopie pas le glossaire.

> Le guide est **validé** (2026-10-01, #40) ; ses choix sont dans sa section 4.
> Un terme absent du glossaire se propose à l'utilisateur, il ne s'invente pas.

## Quand

Dès qu'une tâche ajoute, modifie ou supprime un texte que l'utilisateur voit :
menu, menu contextuel, bouton, onglet, champ, case à cocher, infobulle, barre
d'état, titre de fenêtre, message d'erreur affiché, libellé d'historique, nom
d'axe, d'unité ou de grandeur. Aussi quand un texte est **repris** d'un
fichier ou d'une colonne : il suit les mêmes règles s'il est écrit par PlotYx.

## Ordre de travail

1. **Chercher le terme dans le glossaire** (section 2). Il existe : l'employer
   tel quel, jamais un « refusé ». Il manque : le proposer à l'utilisateur
   avec son anglais, son français, sa définition d'une phrase et ses refusés ;
   ne pas inventer un synonyme.
2. **Écrire l'anglais d'abord** (R1), puis le français.
3. **Appliquer les règles** : majuscules selon l'élément (R3), verbe pour une
   commande (R4), « Show X » pour une visibilité (R5), unité entre parenthèses
   (R6), libellé court et infobulle précise (R7).
4. **Un message dit ce qui s'est passé, puis quoi faire** (R9), sans jargon
   interne (*session, nœud, arbre, id, fil, spec, commande, jeton, recette*).
5. **Nombres et accords** : jamais de « (s) » (R10).
6. **Une règle ou un terme nouveau** : l'ajouter au glossaire dans le même
   commit (règle transversale de `.claude/skills/README.md`).

## Ce qu'on ne fait pas

- Un littéral français dans le code d'une interface : l'anglais est la source.
- Un synonyme du glossaire « parce que ça sonne mieux ici ».
- Traduire les noms choisis par l'utilisateur ou par défaut (*Book1*, *Sheet1*).
- Écrire un texte d'erreur d'un crate du dessous pour l'utilisateur : ces
  crates rendent des données structurées, l'application les met en phrase.

## Mécanique : Fluent, `tr!`, pseudo-langue, garde

L'interface est écrite **d'abord en anglais** ; le français est une traduction
(chantier #39). Tout passe par Fluent, dans `plotyx-app` seulement. Le pourquoi
est en tête de `crates/plotyx-app/src/i18n.rs` ; la vue d'ensemble dans
`docs/application.md` § « Interface multilingue ».

### Ajouter un texte

1. **Écrire la clé dans le catalogue de référence**
   `crates/plotyx-app/i18n/en/plotyx_app.ftl`, jamais ailleurs d'abord. Nom en
   minuscules et tirets, `zone-objet[-précision]` : `menu-file`,
   `plot-axis-log-scale`. L'infobulle d'un contrôle est un attribut de sa clé
   (`.hint`). Rédiger le texte selon le glossaire (`docs/textes-interface.md`) :
   Title Case pour menus, boutons, onglets et titres ; Sentence case pour champs,
   cases à cocher, infobulles, messages.
2. **L'employer par `tr!`** : `tr!("menu-file")`, ou avec arguments nommés
   `tr!("witness-count", count = n)`. Renvoie un `String`. Dans `app.rs` :
   `use crate::tr;` ; dans `main.rs` : `plotyx_app::tr!(…)`.
3. **Un nombre qui change la phrase** (pluriel, accord) se règle dans le `.ftl`
   par un sélecteur Fluent (`{ $count -> [one] … *[other] … }`), jamais par un
   `if` dans le Rust ni par deux clés.
4. **Ne pas toucher à `fr/plotyx_app.ftl`** pour une clé nouvelle : le français
   se rédige avec #49, et une clé absente de `fr` reprend l'anglais.
5. Une **clé retirée** de `en` se retire aussi de `fr` : une clé de `fr` absente
   de `en` fait échouer le test de complétude.

**Libellés d'historique** (#46) : une clé `history-*` par variante de
`HistoryLabel` (et par forme : cible connue ou non), branchée dans
`src/history_text.rs`. Title Case, verbe d'abord ; un nom choisi par
l'utilisateur est entre « “ ” », une colonne se dit « Column A » (lettre de la
grille). Le `Display` anglais de `plotyx-commands` doit dire la même chose que
le catalogue : `tests/history_text.rs` le vérifie.

Un test qui compare une langue précise bâtit son chargeur et écrit
`tr!(in loader; "clé", arg = …)` : même vérification à la compilation, sans
dépendre de `PLOTYX_LANG`.

`tr!` est l'**unique point d'entrée** (il enveloppe `fl!`) : un changement de
mécanique ne doit pas toucher les appels. Ne jamais appeler `fl!` ni le
chargeur directement dans l'interface.

### Ce que le compilateur vérifie, et ce qu'il ne vérifie pas

`fl!` lit `i18n/en/plotyx_app.ftl` à la compilation : **une clé inexistante ou un
argument que le message ne déclare pas ne compile pas.** Vérifié à la main le
2026-10-01 (écriture jetée, puis retirée, d'un `tr!("clef-inexistante")` dans
`main.rs`) :

```text
error: fl!() `message_id` validation failed. `message_id` of "clef-inexistante"
does not exist in the `fallback_language` ("en")
```

et pour un argument : `fl!() argument `nombre` does not exist in the fluent
message. Available arguments: `count`.` On ne peut pas l'écrire en test (un test
ne vérifie pas qu'un code *ne compile pas*) : si la mécanique change, **refaire
cet essai à la main** et mettre la date ici.

Le compilateur ne voit **pas** : une clé de `fr` absente de `en`, des arguments
différents d'une langue à l'autre — ce sont les tests de `src/i18n.rs`
(complétude `fr ⊆ en` avec les mêmes arguments) qui les attrapent ; ni un
littéral écrit en dur — c'est la garde ci-dessous.

### Choisir la langue

Par ordre : la variable `PLOTYX_LANG` (`en`, `fr`, `en-XA`), puis la préférence
`language` de `preferences.json` (dossier de configuration de PlotYx, à côté de
`couleurs.json` ; `{ "language": "fr" }`), sinon l'**anglais**. La langue du
système n'est **pas** suivie (#49). Une valeur inconnue passe au niveau suivant ;
un fichier absent ou corrompu donne l'anglais — jamais de panique (invariant 10).
La langue est choisie une fois, à la première lecture du chargeur global : un
test ne la change pas, il bâtit son propre chargeur (`i18n::build_loader`).

### La pseudo-langue `en-XA`

Fabriquée à la volée depuis `en` (`src/i18n/pseudo.rs`), sans fichier à tenir :
lettres accentuées, texte allongé de 30 %, message encadré de `[[ ]]`. Lancer
l'application avec `PLOTYX_LANG=en-XA` (et la capture, voir
`plotyx-bug-report/references/voir-l-interface.md`) :

- un texte **sans** `[[ ]]` n'est pas passé par le catalogue : un littéral est
  resté dans le code ;
- un `]]` **absent** est un texte tronqué par la mise en page ;
- les arguments (`{ $count }`) ne sont jamais touchés.

Le cadre est en ASCII (`[[ ]]`) parce que Noto Sans n'a pas `⟦ ⟧` ; il est
posé en littéral Fluent `{ "[[" }`, jamais en texte (un `[` en tête de ligne
se lirait comme une clé de variante).

### La garde `aucun_libelle_en_dur`

`crates/plotyx-app/tests/libelles.rs` lit toutes les sources de `src/` et refuse
un littéral textuel dans `label`, `button`, `menu_button`, `checkbox`, `radio`,
`radio_value`, `selectable_value`, `selectable_label`, `toggle_value`,
`on_hover_text`, `heading`, `hint_text`, `selected_text`, `with_title`, dans les
`Button::new(…)`, `RichText::new(…)`, `Label::new(…)`, `Checkbox::new(…)`,
`CollapsingHeader::new(…)`, et dans les tables `fn label()` / `fn title()`. Un
littéral sans lettre (`"→"`, `"{}"`) ou la clé d'un `tr!` ne compte pas. Elle ne
voit pas un `&str` passé en variable ni un titre de `modal_dialog_with` : les
tickets #43-#44 étendent `APPELS` quand ils traitent ces zones.

**La liste de dérogations (`DEROGATIONS`) est une dette, pas un état normal.**
Elle recense l'existant d'avant #41 (213 littéraux, 14 zones), par fichier et par
zone, chaque zone portant le ticket qui la convertit (#42-#45) :

- elle **ne grossit jamais** : un texte neuf passe par `tr!`, même si la fonction
  voisine figure dans la liste — **un code qui y figure n'est pas un modèle** ;
- quand un ticket convertit une zone, il **abaisse le compte** (ou retire la
  zone) dans le même commit : le test échoue aussi quand il y a *moins* de
  littéraux que déclaré, pour que la réduction se voie dans le diff ;
- elle doit être **vide** à #48 ; le test est alors sans dérogation.

### Les tests de la mécanique

| Test | Ce qu'il tient |
|---|---|
| `i18n::tests::toute_cle_francaise_existe_en_anglais_avec_les_memes_arguments` | `fr ⊆ en`, mêmes arguments ; **affiche** (`--nocapture`) les clés `en` pas encore traduites, sans échouer avant #49 |
| `i18n::tests::la_completude_detecte_…` | le témoin : le comparateur voit une clé inconnue et un argument différent |
| `i18n::tests::la_pseudo_langue_entoure_et_allonge_chaque_message` | `[[ ]]`, +30 %, accents, sur chaque clé sans argument |
| `i18n::tests::un_catalogue_francais_vide_affiche_l_anglais` | `PLOTYX_LANG=fr` avec `fr` vide montre l'anglais |
| `i18n::tests::une_preference_corrompue_demarre_en_anglais` | invariant 10 pour `preferences.json` |
| `libelles::la_garde_voit_un_litteral_dans_chaque_forme_d_appel` | le témoin de la garde : sans lui, une garde aveugle passerait |
| `libelles::les_dependances_de_traduction_restent_dans_plotyx_app` | aucun crate du dessous ne dépend de Fluent |

