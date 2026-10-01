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
  crates rendent des données structurées, l'application les met en phrase
  (section suivante).

## Erreurs : texte brut anglais en bas, phrase du catalogue dans l'application

Le `Display` (`#[error(...)]`) des crates du dessous (core, commands,
persistence, import, script, render-svg…) est **de l'anglais brut**, concis et
lisible par un utilisateur du CLI ou de `pyx` : il n'est jamais traduit et aucun
de ces crates ne dépend de Fluent. L'application met chaque erreur affichée en
phrase par `crates/plotyx-app/src/errors.rs` :

- une fonction `error_message(&Erreur) -> String` par famille
  (`errors::model`, `commands`, `session`, `file`, `import`, `formula`,
  `render`, `export`, `script` sous la feature `python`) ;
- chaque variante → une clé `error-<famille>-<cause>` du catalogue `en`, ses
  champs en arguments nommés (`tr!("error-file-corrupt", path = …, detail = …)`) ;
- le `match` est **exhaustif, sans `_`** : une variante nouvelle ne compile pas
  tant qu'on ne l'a pas associée. À défaut de phrase, `errors::fallback` renvoie
  son `Display` anglais (rien ne disparaît) ;
- le test de la famille construit une valeur de chaque variante et vérifie
  phrase non vide, sans jargon (R9), sans point final : en ajouter une pour la
  variante nouvelle ;
- un `detail` (cause décrite par le crate d'origine) ou un message du système
  est repris tel quel, en anglais ;
- **jamais** `to_string()` ni `{err}` d'une erreur montrée à l'utilisateur.

Rédaction : ce qui s'est passé, puis quoi faire seulement si c'est utile ;
Sentence case, pas de point final sur une ligne unique, ni « please », ni
excuse, ni jargon interne (*nœud, session, macro, commande, arbre*). Un message
faux ou ambigu se corrige dans le catalogue (`en`), pas dans le crate du dessous
— sauf si c'est le texte brut anglais lui-même qui est faux.

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
   (`.hint`, lue par `tr!("clé", "hint")`) ; celle d'une entrée grisée, qui dit
   pourquoi, est `.disabled`. Une action partagée entre menu, menu contextuel et
   barre d'état a **une** clé `action-*`. Rédiger le texte selon le glossaire (`docs/textes-interface.md`) :
   Title Case pour menus, boutons, onglets et titres ; Sentence case pour champs,
   cases à cocher, infobulles, messages.
2. **L'employer par `tr!`** : `tr!("menu-file")`, ou avec arguments nommés
   `tr!("witness-count", count = n)`. Renvoie un `String`. Dans `app.rs` :
   `use crate::tr;` ; dans `main.rs` : `plotyx_app::tr!(…)`.
   L'infobulle se lit par `tr!("clé", "hint")` (attribut `.hint` de la clé).
   Un nom choisi par l'utilisateur (*Book1*, une colonne) se passe en
   argument ou se concatène : jamais dans le catalogue (R15). Un test qui
   cherche un texte peint l'obtient par `tr!`, pas par un littéral.
3. **Un nombre qui change la phrase** (pluriel, accord) se règle dans le `.ftl`
   par un sélecteur Fluent (`{ $count -> [one] … *[other] … }`), jamais par un
   `if` dans le Rust ni par deux clés.
4. **Écrire la même clé dans `fr/plotyx_app.ftl`**, dans le même commit, selon
   la colonne française du glossaire : infinitif pour les consignes, initiale
   seule en majuscule, espace insécable (U+00A0) avant « : », « ? » et dans les
   « » ; mêmes arguments que `en` ; pluriel CLDR français (le singulier vaut
   pour 0 et 1). La complétude est **bloquante** : une clé absente de `fr`
   fait échouer `toute_cle_francaise_existe_en_anglais_avec_les_memes_arguments`.
5. Une **clé retirée** de `en` se retire aussi de `fr` (clé de `fr` absente de
   `en` : même test).

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

Le compilateur ne voit **pas** : une clé de `fr` absente de `en` (ou l'inverse),
des arguments différents d'une langue à l'autre — ce sont les tests de
`src/i18n.rs` (complétude `fr = en`, mêmes arguments) qui les attrapent ; ni un
littéral écrit en dur — c'est la garde ci-dessous.

### Choisir la langue

Par ordre : la variable `PLOTYX_LANG` (`en`, `fr`, `en-XA`), puis la préférence
`language` de `preferences.json` (dossier de configuration de PlotYx, à côté de
`couleurs.json` ; `{ "language": "fr" }`), sinon l'**anglais**. La langue du
système n'est **pas** suivie : décision de l'utilisateur (2026-10-01), l'interface
reste en anglais tant qu'on n'a pas choisi le français. Une valeur inconnue
passe au niveau suivant ; un fichier absent ou corrompu donne l'anglais — jamais
de panique (invariant 10).

**Changer de langue sans redémarrer** : menu Fichier › Langue
(`PlotyxApp::choose_language`) → `i18n::set_language` (les textes se redessinent
à l'image suivante) puis `Preferences::save_language`. `PLOTYX_LANG` l'emporte
encore au prochain démarrage. Un test qui compare un texte exact **épingle**
l'anglais (`set_language(English)`) pour ne pas dépendre de la préférence de
celui qui teste ; un test qui change la langue globale a son propre binaire
(`tests/language_switch.rs`).

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
tickets de conversion étendent la garde quand ils traitent ces zones. #43 l'a
fait pour les propriétés du graphe : `ZONE_43` (même fichier) interdit **tout**
littéral textuel dans ses fonctions (`strong`, `weak`, `color_field`,
`set_title`, argument de fonction libre…), hors identifiants (`[a-z0-9_./-]`) et
sigles de format (`SVG`) ; une fonction nouvelle de cette fenêtre s'y ajoute.
Dans cette zone, l'infobulle d'un champ est l'attribut `.hint` (`tr!("clé",
"hint")`) et `text_field(ui, libellé, infobulle, valeur)` la pose.

**La garde est absolue (#48) : aucune dérogation.** Les 213 littéraux d'avant #41
ont été convertis (#42 à #45) et la liste supprimée. Un littéral dans un de ces
appels fait échouer `aucun_libelle_en_dur`, quel que soit le fichier : on écrit
la clé dans `en`, jamais une exception. Un champ de format (`{code}`) n'est pas
du texte ; un texte posé par un chemin que la garde ne voit pas (un `&str` passé
en variable, un titre de `modal_dialog_with`) reste à la charge de la capture en
pseudo-langue.

`aucune_cle_du_catalogue_n_est_morte` (`src/i18n.rs`) est le pendant : une clé
de `en` dont le nom n'apparaît dans aucune source de `src/` échoue. Une clé
retirée du code se retire du catalogue dans le même commit.

**Une notion = une clé.** Un même texte au même sens dans deux zones se partage
(`common-auto`, `column-type-*`, `column-role*`, `column-long-name`,
`column-units`, `column-comments`, `plot-type-*`…) ; ne pas dupliquer pour
« rester dans sa zone ». Exceptions voulues : les clés `history-*` (la phrase
d'une action enregistrée peut évoluer seule) et les mots dont l'accord change en
français (*None* : Aucun / Aucune).

### Les tests de la mécanique

| Test | Ce qu'il tient |
|---|---|
| `i18n::tests::toute_cle_francaise_existe_en_anglais_avec_les_memes_arguments` | `fr = en` : mêmes clés, mêmes arguments (**bloquant**, #49) |
| `i18n::tests::la_completude_detecte_…` | le témoin : le comparateur voit une clé inconnue et un argument différent |
| `i18n::tests::la_pseudo_langue_entoure_et_allonge_chaque_message` | `[[ ]]`, +30 %, accents, sur chaque clé sans argument |
| `i18n::tests::un_catalogue_francais_vide_affiche_l_anglais` | `PLOTYX_LANG=fr` avec `fr` vide montre l'anglais |
| `i18n::tests::une_preference_corrompue_demarre_en_anglais` | invariant 10 pour `preferences.json` |
| `libelles::la_garde_voit_un_litteral_dans_chaque_forme_d_appel` | le témoin de la garde : sans lui, une garde aveugle passerait |
| `libelles::les_dependances_de_traduction_restent_dans_plotyx_app` | aucun crate du dessous ne dépend de Fluent |

