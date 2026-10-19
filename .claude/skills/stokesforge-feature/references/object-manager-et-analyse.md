# Fiche : Object Manager, sélection des contenus, corrections d'analyse

Quand la lire : la tâche touche l'Object Manager, la sélection d'un graphe, le glisser-déposer d'objets ou une correction définitive d'analyse.

## Où lire la règle

- Object Manager (#81-85), réglages de vue (#139), sélection partagée, `with_graph_view` : `.claude/conventions.md` § Object Manager, panneaux fixes, menus ; `docs/application.md` § Gestionnaire d'objets (Object Manager.
- Série, groupe, glisser par la poignée : `.claude/conventions.md` § Séries et groupes ; `docs/application.md` § Gestionnaire d'objets (Object Manager.
- Isolation des préférences dans les tests : `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, piège `cfg(test)`.
- Corrections définitives (#212), `ReplaceAnalysisData` : `.claude/conventions.md` § Analyse (#181) ; tests : `docs/tests.md` § Remplacement définitif d'une correction (#212).

## À vérifier

- [ ] aucune sélection parallèle dans le panneau : la sélection reste dans `GraphView`
- [ ] une seule action part au relâchement d'un glisser (`PlaceSeries`, `PlaceSeriesGroup`, `ReorderObject`)
- [ ] test d'`AppState` avec `new_with_preferences`, jamais la configuration réelle
- [ ] correction : versions de toutes les entrées exigées, trace annulée avec les valeurs
