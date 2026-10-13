# Reprise des issues #210, #211 et #212

Travail interrompu à la demande de l’utilisateur le 2026-10-13 pour préserver
les crédits. Branche `codex/issues-210-212`, worktree `/tmp/stokesforge-210-212`.
Aucune fusion ni aucun push sur `dev` n’a été effectué.

## État

Les trois implémentations sont présentes, avec leurs tests et leur documentation.
#210 : analyse depuis courbe/groupe, X explicite, style et cadrage de la source,
ajout des résultats au même calque/groupe.
#211 : constante par point ou moyenne de zone, réglages dédiés conservés
lors des changements de forme, persistance et Python.
#212 : remplacement atomique, versions attendues obligatoires, trace persistée
et annulable, Results immuable, confirmation des sources partagées,
colonnes calculées/formules protégées, API Python.
Choix utilisateur : ajouter un repère discret cohérent avec l’interface.
Implémenté par crayon Phosphor neutre dans la marge d’en-tête et info-bulle.

## Vérifications réellement obtenues

- Analyse pure et persistance constante : tests ciblés verts.
- Formulaire : 24 tests verts et test peint de conservation des réglages vert.
- #210 : 4 unitaires, 1 intégration X/groupe/undo, 1 test rendu styles verts.
- b14 et b22 : verts, couverture du remplacement >= 20 passes dans b22.
- Commandes et libellés #212 : ciblés verts avant les derniers renforcements.
- Clippy workspace all-targets avec -D warnings : vert lors de sa dernière passe,
  avant quelques derniers ajouts de tests et renforcements.
- Suite workspace arrêtée sur typographie française de la nouvelle clé
  analysis-correction-trace ; espace insécable corrigé. Pas de suite finale verte.
- Suite Python arrêtée sur nouveau test constant qui attrapait ValueError
  au lieu de pyst.InvalidValueError ; corrigé. Test Python remplacement vert.
- Dernière commande fmt/check/test/build interrompue en chaîne dès fmt check
  (sources modifiées entre les passes). Formatage appliqué avant sauvegarde.
- Icône isolée regardée ; application réelle et pseudo-langue non vérifiées.

## À faire avant fusion

1. Relire le diff, notamment variantes constante et remplacement, puis lancer
   fmt check, clippy workspace all-targets -D warnings, cargo test --workspace.
2. Rejouer la suite Python complète et Clippy Python avec le runtime embarqué.
   Le runtime et target sont des liens locaux vers le dépôt principal ;
   CARGO_TARGET_DIR peut pointer vers /home/aliocha/dev_project/StokesForge/target.
   /tmp/stokesforge-verify-python.sh reprend scripts/python-test.sh avec chemins
   runtime/config originaux pour réutiliser le cache sans changer RUSTFLAGS.
3. Construire app/CLI et vérifier interactions dans app réelle, capture en en-XA,
   crayon après remplacement, Results, Ctrl+Z, graphe/OM et constante.
4. Fusionner et pousser dev seulement après validation ; préserver modifications
   utilisateur du dépôt principal (.codex/README.md, AGENTS.md et lien
   .agents/skills/run-stokesforge).
5. Après push réussi nettoyer worktree et branche conformément au skill.

Le script icons-build.sh --sheet échoue actuellement sur des SVG sources
text-align-* absents. Ses suppressions ont été restaurées ; seule la nouvelle
variante pencil-simple a été cuite avec les couleurs de palette.tsv existantes.
La nouvelle icône source et cuite est incluse dans le commit.
