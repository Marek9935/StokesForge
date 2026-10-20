# Application : sommaire

Décrit `stokesforge-app` (la coquille eframe) et `stokesforge-app-state` (l'état
sans egui). Découpé par famille (F2, #301) ; les titres de section sont ceux de
l'ancien `docs/application.md`, que le code et les skills citent par « fichier
§ Titre ». Conventions à suivre : `.claude/conventions.md`, section « Application ».

| Je veux… | Lire | Sections principales |
|---|---|---|
| comprendre le principe du banc, le partage des deux crates, la table module → rôle de `src/app/` | [`coquille-et-code.md`](coquille-et-code.md) | Deux crates : l'état et la coquille ; Découpage du code |
| la fenêtre d'analyse, le menu Analysis, le volet Results | [`analyse.md`](analyse.md) | Analyse : fenêtre, menu, volet Results |
| les menus de la barre, les barres d'outils, la barre d'état | [`menus-et-barres.md`](menus-et-barres.md) | Menus thématiques ; Barres d'outils ; Barre d'état |
| les fenêtres flottantes, onglets, listes, géométrie, détachement, panneaux | [`fenetres-et-panneaux.md`](fenetres-et-panneaux.md) | Fenêtres flottantes confinées ; Débordement ; Onglets, listes et sélection ; Géométrie et état ; Maximiser et détacher ; Panneaux rétractables ; Détachement générique |
| un projet neuf, le dépôt de fichiers, l'assistant d'import | [`projet-et-import.md`](projet-et-import.md) | Nouveau projet ; Glisser-déposer ; Assistant d'import |
| Project Manager, Object Manager, graphique collé entre projets | [`gestionnaires.md`](gestionnaires.md) | Project Manager et fenêtres par contenu ; Gestionnaire d'objets |
| le thème, les textes Fluent, le sélecteur de fichiers, les erreurs mises en phrase | [`theme-i18n-systeme.md`](theme-i18n-systeme.md) | Thème ; Interface multilingue (Fluent) ; Sélecteur de fichiers natif |
| la ligne de partage état/coquille, la console de script, les tests du banc, le CLI | [`etat-et-console.md`](etat-et-console.md) | Deux modules ; La console de script ; Ce que Qt donnait gratuitement ; Tests ; `stokesforge-cli` |
| la grille : saisie, Tab et flèches, gouttière, sélection, menus contextuels | [`grille.md`](grille.md) | Lignes « Metadata » et x' ; Menus contextuels de la grille ; Sélection multiple ; Tab et flèches ; Alignement |
| le tracé depuis un classeur, les propriétés des graphiques, la copie de figure | [`graphe.md`](graphe.md) | Tracer depuis un classeur ; Glisser une sélection ; Propriétés des graphiques |
