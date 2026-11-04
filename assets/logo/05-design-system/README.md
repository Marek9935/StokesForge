# StokesForge

Analyse et tracé de données scientifiques, dans l'esprit d'Origin, réécrit en Rust. Le logo raconte le nom : sur une **enclume** (la forge), un **marteau** frappe la bande d'**absorption** (on excite la molécule) ; les étincelles de la frappe retombent en **émission**, à plus basse énergie. L'écart entre le maximum d'absorption et le maximum d'émission est le **déplacement de Stokes** : après l'excitation, la molécule relaxe (vibrations, solvant) avant d'émettre, d'où une émission décalée vers le rouge.

## Le logo

**Version retenue : `stokesforge-icon.svg`** et ses déclinaisons sans tuile `stokesforge-mark.svg` / `stokesforge-mark-dark.svg`. Le dessin est au trait épais, sans graduations ni flèche : il se lit d'un coup d'œil, de 16 px à l'affiche.

- **Enclume** (`ink`) : contour seul, intérieur transparent. Silhouette reprise de la capture d'origine : corne pointue à gauche, table plate, taille resserrée, pieds évasés. Le contour a l'épaisseur de la ligne d'axe.
- **Axe des ordonnées** (`ink`) : planté dans la table de l'enclume, flèche en haut. La table de l'enclume sert d'axe des abscisses.
- **Deux bandes, absorption et émission, en miroir** (règle du miroir de Levshin) : l'axe x se lit en longueur d'onde croissante.
  - absorption en `forge-green` (le vert `#3a9155` du dessin) : bande principale et **un** épaulement vibronique **à gauche**, côté haute énergie ;
  - émission en `ember` : image miroir exacte de l'absorption, épaulement **à droite**, côté basse énergie. Orange comme le métal chaud et comme le décalage vers le rouge ;
  - les deux maxima se font face ; l'écart entre eux est le **déplacement de Stokes**. Les bandes se recouvrent légèrement au pied, vers la transition 0-0.
  - forme : gaussienne principale (σ = 8) plus épaulement (σ = 9, amplitude 0,6, décalé de 20), sans creux entre les deux ; déplacement de Stokes de 32 sur 256.
- **Marteau** (`ink`) : petit, accessoire ; il touche le sommet du pic d'absorption, quatre étincelles `ember` marquent l'impact. Le décalage entre les deux courbes est le déplacement de Stokes que la frappe a « forgé ».

## Fichiers (groupe Logos)

| Fichier | Quand |
| --- | --- |
| `stokesforge-icon.svg` | **Logo retenu.** Icône d'application : tuile `surface` à coins `radius-icon`. |
| `stokesforge-mark.svg` | Le même dessin sans tuile, fond transparent clair (site, documentation, en-têtes). |
| `stokesforge-mark-dark.svg` | Le même sans tuile, pour fond sombre : encre `#e6e9e7`, courbes éclaircies. |
| `stokesforge-lockup.svg`, `-dark` | Logo horizontal : le dessin suivi de **Stokes**Forge (« Stokes » normal, « Forge » gras). Le texte utilise la police système. |

Explorations autour du logo retenu (à discuter) :

| Fichier | L'idée |
| --- | --- |
| `explore-axis-hammer.svg` | L'axe des ordonnées *est* le manche d'un marteau posé debout : la tête remplace la flèche. Un seul objet en moins, le graphe et l'outil se confondent. |
Base commune des essais « excitation » : le marteau, tombé depuis la corne de l'enclume, se pose sur le **sommet de l'absorption** (on excite) ; plans : émission, absorption, marteau devant ; bandes fines, **déplacement de Stokes 34**.

| `explore-excite-bande-lueur.svg` | **Bande dégradée + lueur.** Le flanc montant de l'émission naît vert et vire à l'orange à son maximum : l'émission sort de l'absorption. Une lueur orange s'allume sous la bande d'émission. Aucun lien entre les bandes. |
| `explore-excite-etoile.svg` | **Bande dégradée + étincelle.** Même flanc dégradé, et une étincelle à quatre branches (plus une petite) au-dessus du maximum d'émission : la lumière émise. |
| `explore-excite-bande.svg` | Le flanc dégradé seul. |
| `explore-excite-lueur.svg` | Trait droit en dégradé vert → orange de l'impact au maximum d'émission, plus la lueur. |
| `explore-excite-trait.svg` | Le trait droit en dégradé seul. |
| `explore-excite-comete.svg` | Le trait en comète : s'épaissit vers l'émission et finit en point. |
| `explore-excite-fall.svg` | Version précédente : arc de points orange de l'absorption vers l'émission, déplacement de Stokes 44. |
| `explore-excite-strike.svg` | Version précédente : frappe d'en haut, même arc. |
| `explore-behind-axis.svg` | **Derrière le vert.** Plans : émission au fond, marteau au milieu, absorption devant (liseré couleur de fond). Le marteau, tombé depuis le pied de l'axe, se pose sur la pointe de l'émission (38 à 42° selon la longueur). Nouvelle tête de forgeron : table de frappe évasée et chanfreinée du côté qui tombe, collet, renflement de l'œil, panne en coin émoussé ; manche évasé vers le bout. Pour laisser la place à la tête entre les deux maxima, les bandes sont plus fines (σ = 7, épaulement σ = 8 décalé de 17) et le **déplacement de Stokes passe de 32 à 44**. |
| `explore-behind-axis-dent.svg` | La même, marteau un peu plus court posé sur le flanc gauche de l'émission puis tourné de 4° de plus : déformation légère. |
| `explore-behind-horn.svg`, `-dent` | Les deux mêmes, mais le pied du marteau repose sur la **corne** de l'enclume : la chute est plus plate (50°), le manche se voit à gauche et ne croise le vert que bas. Manche plus long (128–136). |
| `explore-drop-still.svg` | **Posé, devant.** Axe Y fixe ; le marteau est tombé et sa tête est **posée sur la pointe** de la bande d'émission, sans s'y enfoncer (contact calculé sur la forme exacte du marteau : 38,3°). Le marteau est entièrement **devant** le graphe, comme la courbe orange ; un fin liseré couleur de fond détache son manche de la bande verte. Des éclats marquent le contact. |
| `explore-drop-dent-still.svg` | Le marteau, un peu plus court, est posé sur le flanc gauche de l'émission puis a continué de 6° : il **déforme** légèrement la bande, repoussée hors du marteau avec un creux lissé, comme une matière molle. |
| `explore-fall.svg` | **Chute.** Le marteau-axe bascule depuis l'origine et tombe en travers du graphe ; son manche se pose sur le maximum d'émission (angle de chute calculé : 44,5°). Deux fantômes et un arc pointillé décomposent la chute, façon chronophotographie. Plus d'axe Y : c'était lui, le marteau. L'histoire : le marteau tombé a poussé la courbe, d'où le déplacement de Stokes. |
| `explore-fall-axis.svg` | La même chute, avec un axe Y qui reste en place : le graphe reste un graphe, l'application n'a pas l'air cassée. |
| `explore-fall-cartoon.svg` | Axe fixe, sans fantômes : étoile d'impact jaune sur le maximum d'émission et traits de vibration sur son flanc droit, comme si la courbe tremblait encore du choc. |
| `explore-fall-night.svg` | La chute en « forge de nuit », sur tuile sombre. |
| `explore-fallen.svg`, `explore-fallen-mark.svg`, `-dark` | Suite de l'axe-marteau : le marteau est tombé de l'axe (qui retrouve sa flèche) et reste appuyé contre le flanc de la bande d'émission, le pied du manche au coin de la table. Angle de contact calculé : 11,8°, le marteau touche la courbe sans la traverser. Le marteau ne peut pas pivoter depuis l'origine : il masquerait la bande d'absorption. |
| `explore-hot-emission.svg` | L'émission est incandescente : dégradé jaune → orange → rouge sombre et halo, comme une barre sortie du feu. |
| `explore-dark-tile.svg` | La même, en « forge de nuit » sur tuile sombre : le halo y prend tout son sens. |
| `explore-favicon.svg` | Favicon 16–32 px : enclume et deux bandes seulement, traits très épais, ni axe ni marteau. |

## Règles d'usage

- Marge de protection : `space-4` autour du logo à 256 px, `space-2` à 64 px.
- Taille minimale : 16 px.
- Ne pas recolorer l'enclume dans une couleur de courbe ; ne pas inverser l'ordre vert → orange (l'émission est toujours à droite).
- Ne pas ajouter d'ombre ni de dégradé, ne pas remplir l'enclume : le dessin est au trait.
- Nom à côté du logo : style `display`, couleur `ink`, écrit **StokesForge** (un mot, S et F capitales).

## Couleurs

Neutres d'abord (`ground`, `surface`, `ink`), deux teintes qui portent l'identité (`forge-green`, `ember`), et `accent` pour l'interface (même vert que `ACCENT` dans `theme.rs`). `forge-green` et `ember` sont des couleurs de graphique : pour du texte vert, prendre `accent`.

## Typographie

Pas de police propre pour l'instant : pile système (`sans`). Un nom écrit en `display` 64 px, gras, interlettrage −0,02 em.
