# Évaluation de Matplotlib — spike #69

Le prototype fonctionne sur Linux x86_64 : Matplotlib s'importe dans CPython
isolé, lit les colonnes `pyx` via numpy et produit du SVG et du PNG hors écran.
Le SVG passe aussi par les convertisseurs PNG/PDF déjà utilisés par les exports
PlotYx. Ce résultat est un fichier exporté, pas un `PlotSpec` éditable.

## Reproduction

```sh
scripts/python-vendor.sh
CARGO_BUILD_JOBS=2 scripts/python-matplotlib-spike.sh
```

Le script copie le runtime normal dans
`vendor/python/extracted/matplotlib-spike/python/` (ignoré par Git), puis
installe uniquement les roues binaires épinglées dans
`vendor/python/matplotlib-requirements.txt`. numpy vient du verrou normal.
La copie est renouvelée quand `vendor/python/lock.env` change ; `pip check`
contrôle la cohérence. Le runtime habituel et les paquets de distribution
restent ceux de base, sans Matplotlib.

Le test exécutable est `plotyx-script/examples/matplotlib-spike.rs`, lancé
explicitement par le script. Il vérifie :

- version 3.11.2, import depuis la copie et `sys.flags.isolated == 1` ;
- création de colonnes, calcul numpy et tracé depuis leurs vues en lecture seule ;
- `FigureCanvasSVG` puis `FigureCanvasAgg`, sans fenêtre ni `pyplot` ;
- deux exports SVG identiques (`svg.hashsalt` fixé et date de métadonnée retirée) ;
- signatures PNG direct/Agg et PNG/PDF convertis par les exports Rust ;
- données préservées et annulation du groupe de créations.

Les fichiers exportés restent sur disque après undo : l'historique annule le
modèle, pas les écritures de fichiers arbitraires d'un script. Les artefacts de
contrôle sont `/tmp/plotyx69-spike.svg`, `/tmp/plotyx69-spike.png` et
`/tmp/plotyx69-measurements.json`. Aucun artefact généré n'est versionné.

## Mesures du 2 octobre 2026

CPython 3.12.14 et numpy 2.5.3 du verrou, Matplotlib 3.11.2 et ses neuf
dépendances épinglées, Linux x86_64, poste de développement partagé.
Les tailles sont apparentes (`du -sb`), préfixe complet incluant pip ; elles
ne représentent pas une archive compressée ni la taille d'un installateur.

| Mesure | Résultat |
|---|---:|
| Runtime de base | 174 605 105 octets (166,5 Mio) |
| Copie avec Matplotlib | 269 414 669 octets (256,9 Mio) |
| Surcoût | 94 809 564 octets (90,4 Mio) |
| Initialisation du moteur embarqué, sans Matplotlib | 65,6 ms |
| Import numpy + Matplotlib + Figure/SVG/Agg dans le moteur, cache de polices neuf | 1 236 ms |
| Import Matplotlib + backend SVG en processus frais, premier échantillon | 769 ms |
| Même import, médiane des cinq processus suivants | 151 ms |

La dernière série commence par importer numpy séparément (49–53 ms), puis
chronomètre Matplotlib et le backend SVG. Chaque processus est neuf ; seul
`MPLCONFIGDIR` est partagé entre les six lancers. Le premier construit le
cache de polices ; les cinq suivants le réutilisent. Les caches du système de
fichiers ne sont pas purgés. L'import embarqué charge aussi Figure et Agg :
les deux mesures ne sont donc pas directement comparables. Ces valeurs
situent le coût sur ce poste, sans promettre un temps universel.

## Ce que le spike permet de décider

Matplotlib convient à un export libre issu d'un script : l'utilisateur
scientifique peut réutiliser son code de tracé et les backends standards. Le
coût en taille est sensible, et l'import doit rester paresseux. Le prototype
n'ajoute pas de fenêtre Matplotlib ni de nouvelle surface native `pyx`.

Les graphes créés par `pyx.plot` gardent leurs séries et références de colonnes
dans un `PlotSpec`, modifiable dans les propriétés de PlotYx. Un SVG Matplotlib
est un résultat figé ; il n'offre pas cet historique d'édition. Les deux voies
ont donc des usages distincts. Le titre du prototype utilise le mathtext de
Matplotlib ; le rendu Typst des graphes natifs reste celui de PlotYx.

L'essai démontre la voie « export » demandée dans #69. Un affichage dans une
note, la gestion de figures persistantes ou une fenêtre dédiée ne sont pas
implémentés par ce prototype. L'intégration éventuelle au runtime distribué
reste un choix produit ; les mesures et le test permettent de le prendre.

Documentation officielle utilisée :
[dépendances](https://matplotlib.org/stable/install/dependencies.html),
[backend SVG](https://matplotlib.org/stable/api/backend_svg_api.html),
[Figure.savefig](https://matplotlib.org/stable/api/_as_gen/matplotlib.figure.Figure.savefig.html).
