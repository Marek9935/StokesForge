---
name: plotyx-textes-interface
description: Écrire ou modifier tout texte visible de PlotYx — libellé de menu, de bouton, de champ, d'onglet, titre de fenêtre, infobulle, message d'état ou d'erreur affiché, nom d'axe ou d'unité. À utiliser dès qu'un littéral destiné à l'utilisateur est ajouté, changé ou traduit, avant d'écrire le texte : le glossaire anglais ↔ français (`docs/textes-interface.md`) est la source unique des termes, des majuscules et du ton. Une notion = un mot, partout.
---

# Textes d'interface

Source unique : [`docs/textes-interface.md`](../../../docs/textes-interface.md)
(glossaire, règles R1-R16, exemples avant/après). Ce skill dit **quand** le
lire et **dans quel ordre** s'en servir ; il ne recopie pas le glossaire.

> Le guide est un **brouillon en attente de validation** (#40). Un terme listé
> dans sa section « À trancher » n'est pas arrêté : demander avant d'écrire un
> texte qui en dépend.

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

## Mécanique

*À compléter par #41* : catalogues Fluent `en`/`fr`, macro `fl!`, choix de la
langue, pseudo-langue de test, gardes.
