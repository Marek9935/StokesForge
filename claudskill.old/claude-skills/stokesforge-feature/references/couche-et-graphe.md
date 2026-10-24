# Fiche : couche choisie, gestes sur un graphe, calques

Quand la lire : la fonctionnalité touche un graphe (geste continu, texte libre, ajout de série, export) ou l'on hésite entre `state.rs` et `src/app/`. Cette fiche ne contient aucune règle : elle renvoie.

## Où lire la règle

- Ligne de partage état / coquille : `docs/application/coquille-et-code.md` § Deux crates : l'état et la coquille ; `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes.
- Action qui a besoin de l'état de vue (zoom) : `docs/application/graphe.md` § Propriétés des graphiques (`GraphView::framed_spec`, `Action::ExportGraph`).
- Geste continu, édition de texte sur place : `docs/application/graphe.md` § Propriétés des graphiques (`annotation_interactions`) ; `docs/rendu.md` § Couleurs du texte et des échelles : l'encre automatique (placement du champ dans le cadre du rendu).
- Calque par indice, calque actif, ajout de séries : `.claude/conventions.md` § Interface : câblage à ne pas oublier (fenêtre détachée, calque, dialogue) ; `docs/application/graphe.md` § Glisser une sélection de colonnes vers un graphe.
- Graphe neuf sans titre : `.claude/conventions.md` § Graphes neufs et fenêtres détachées.
- Table couche -> lieu (modèle, vue, décision, dessin, rendu, import) : tableau de la section 2 du `SKILL.md`.

## À vérifier

- [ ] une action qui lit l'état de vue le reçoit dans l'`Action` (jamais lu dans `state.rs`)
- [ ] un geste continu n'émet qu'une action, à la validation ; l'aperçu vit dans `GraphView`
- [ ] le calque est visé par son indice dans `PlotSpec::panels` (calque actif si aucune cible cliquée)
- [ ] l'ajout de séries réutilise `drop_payload_action` / `drop_rows_action`
- [ ] graphe neuf : `PlotSpec::title` vide
- [ ] aucune règle métier dans `app` : elle serait invérifiable en CI
