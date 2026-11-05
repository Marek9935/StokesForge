# Logo StokesForge : récapitulatif

Date : 4 novembre 2026. Ce document résume ce qui a été discuté, ce qui est voulu, ce qui a été rejeté, et où en est le logo. Ce dossier est rangé dans le dépôt sous `assets/logo/` (#443) ; l'icône d'application et l'icône des fichiers `.stf` en sont tirées (#388). La matière en verre vient de #445. Sous GPL-3.0-or-later comme le code : voir `LICENSE` (#450).

## 1. L'idée

Une **forge** (enclume + marteau) rencontre un **graphe de spectroscopie**. Le marteau **excite la bande d'absorption** (verte) ; l'énergie ressort en **bande d'émission** (orange), à plus basse énergie. L'écart entre les deux maxima est le **déplacement de Stokes**.

## 2. Physique (non négociable)

- L'axe x se lit en longueur d'onde croissante vers la droite.
- **Absorption = vert**, à gauche : une bande principale et **un seul épaulement vibronique, à gauche** (côté haute énergie). Pas de second pic, pas de creux.
- **Émission = orange**, à droite : **image miroir exacte** de l'absorption (règle du miroir), épaulement **à droite**.
- Les deux maxima se font face ; leur écart est le déplacement de Stokes (30 sur une largeur de 256 dans la version actuelle, 36 avant #445). Léger recouvrement au pied (transition 0-0).
- Le marteau agit sur **l'absorption** (on excite), jamais sur l'émission : l'émission est la conséquence (relaxation, puis émission décalée vers le rouge).

Formes calculées (scripts) : gaussienne principale σ = 7, épaulement σ = 8, amplitude 0,6, décalé de 17 ; l'émission est le miroir exact.

## 3. Ce qui est voulu (état actuel)

| Élément | Voulu |
| --- | --- |
| Enclume | Silhouette d'enclume, **basse** (hauteur réduite à ~72 %), corne **conique courte et droite** à pointe arrondie (pas une coque de bateau). Une seule marche : corne puis table, aux deux coins hauts légèrement arrondis (#446). **Aucune deuxième marche** sous les courbes. |
| Matière de l'enclume | **Verre sombre « liquide »** (style Liquid Glass d'Apple, mais qui **assombrit** pour faire ressortir le contenu). La lumière des spectres coule **dans** le verre (vert sous l'absorption, orange sous l'émission) et se concentre dans l'arrondi des bords. Les spectres se reflètent dans le verre. **Liquide, pas verni** : un filet blanc adouci (65 % de sa force, choix de #445) suit le bord, et le reflet net des deux pics se courbe en arrivant dans l'arrondi. |
| Jonction courbes / enclume | Les courbes sont **posées directement sur la table**. La ligne de jonction tire vers le **sombre**, pas vers le blanc : un trait fin qui s'efface aux deux bouts, là où les courbes s'arrêtent (#446). Rien ne déborde sous ou à côté des courbes. |
| Courbes | **Formes pleines sans trait coloré** (le mélange trait + remplissage est rejeté). Vert dense ; orange incandescent (orange chaud au sommet, orange profond au pied). Les sommets restent bien lisibles (pas trop clairs). Une fine lueur suit le haut des courbes, sans halo étalé au pied. Volume net : formes bombées, sans changer leur contour. |
| Axe Y | Flèche d'axe planté sur la table, à gauche des courbes. Reste fixe (sinon l'appli « a l'air cassée »). |
| Marteau | Marteau de forgeron dessiné en courbes (table de frappe évasée, collet, panne en coin émoussé), le bout du manche **posé sur la corne**, la face **posée** sur le sommet vert sans s'y enfoncer, **devant** les courbes. Petits éclats orange au contact, sans toucher la tête. Pas de marteaux fantômes de la chute. |
| Lien absorption → émission | **La comète** : un triangle fin qui part de l'impact et finit en **point** sur le sommet orange, dégradé vert → orange. |
| Ombre et fond | Ombre grise douce sous l'enclume, sans couleur. Fond de tuile en aplat uni (`#f3f1ec` en clair, `#15181a` en sombre). |
| Thème | Travail principal en **clair**. Une version sombre existe. |

## 4. Rejeté ou mis de côté

- Graduations x dans la table, flèche Δ gravée, enclume gravée pleine.
- Spectres à deux bosses (faux physiquement) ; marteau frappant l'émission.
- Marteaux intermédiaires (chronophotographie) : trop chargé.
- Arc de points, flèche courbe en pointillé : lourds, pas propres.
- Petits trous carré / rond dans la table : pas convaincants.
- Étoiles à quatre branches (à revoir plus tard, éventuellement).
- Trait coloré + remplissage ; halos qui « bavent » au pied ; bande blanche à la jonction ; reflet diagonal sur l'enclume ; effet glossy.
- Essayés pour #445 et écartés : verre clair ou moyen (l'enclume se perd sur fond clair), filet blanc à pleine force, ombre colorée sous l'enclume, tuile en dégradé.

## 5. Gardé en réserve (idées appréciées)

- Ciel étoilé discret en version sombre.
- Enclume au voile grisâtre (version nuit).
- Mouvement (#445, sans décision) : `02-iterations/mouvement/` garde une animation de la frappe (`-anime.svg`, boucle de 4,2 s) et une version fixe avec des ondes dans le verre (`-ondes.svg`).
- Explorations des sous-agents : « Enclume de Jablonski » (diagramme d'énergie dans l'enclume), « L'enclume est le déplacement de Stokes », « Press » (l'axe devient le marteau), médaillon « foyer ».

## 6. Version actuelle

- `00-final/stokesforge-logo-clair.svg` et `-sombre.svg` : **versions de référence**, en contours vectoriels.
- `00-final/stokesforge-logo-clair-filtre.svg` et `-sombre-filtre.svg` : même logo, réfraction calculée à l'affichage par un filtre SVG.
- PNG en 1024, 512, 256, 64 et 32 px, tirés des versions de référence avec resvg.
- `00-final/stokesforge-icone-clair.svg` : **icône d'application** (#388), le même dessin de référence sans tuile, fond transparent, centré sur son poids visuel, le même à toutes les tailles. `00-final/stokesforge-fichier-stf-clair.svg` : **icône des fichiers `.stf`**, une feuille à l'horizontale, coin plié en haut à droite, le logo dedans. Aucun raster n'en est rangé dans le dépôt : la crate `stokesforge-logo-gen` produit les PNG (16 à 512 px) et les `.ico` à chaque version (`scripts/logo-dist.sh`), après avoir refusé une source hors cadre (fond opaque, dessin décentré, filtre `feDisplacementMap` ou `feImage`).
- L'ancien logo plat est dans `02-iterations/avant-liquid-glass/`. C'est aussi la source que les scripts relisent.

| Version | Poids | Compressé | Contenu | Moteurs de rendu |
| --- | --- | --- | --- | --- |
| Contours | 269 Ko | 59 Ko | formes, dégradés, découpes, flou gaussien ; aucune image | Mesurés concordants à moins de 1/255 d'écart moyen : Chromium, Firefox, librsvg, resvg 0.45.1, Inkscape 1.4 |
| Filtre | 185 Ko | 129 Ko | `feDisplacementMap`, `feImage` et masques en images de niveaux de gris | Corrects : Chromium, librsvg. **Cassé dans resvg** (déviation en marches d'escalier), lumières perdues dans Inkscape |

Pas testé : Safari, Illustrator, Figma, KDE (QtSvg).

## 7. Méthode du verre

Chaque forme est gonflée numériquement (équation de Poisson, puis un dôme qui sature). Les normales de cette surface donnent, pixel par pixel : la réfraction (loi de Snell), l'absorption selon l'épaisseur (Beer-Lambert, bords plus clairs), la réflexion aux bords (Fresnel) et la lumière directionnelle.

**Le SVG sait faire une vraie réfraction**, contrairement à ce que disait une version précédente de ce document : `feDisplacementMap` la fait, et la limite « Chromium seulement » ne vise que son emploi en `backdrop-filter`. Elle ne se voit que si ce qui passe derrière le verre a des contours : un flou n'a rien à déformer. D'où le reflet net.

La version de référence calcule cette réfraction une fois pour toutes, puis trace chaque couche en une pile de contours imbriqués, avec un flou très léger qui dissout les paliers. Ne pas remplacer ce flou par des centaines de couches très transparentes : les arrondis sur 8 bits s'accumulent et font des plaques colorées. À 2048 px et plus, de légers paliers se devinent encore dans le reflet.

Limites : le fichier ne se retouche pas à la main, on change les réglages et on relance le script. L'effet natif d'Apple ne s'obtient que dans Icon Composer, sur Mac ; il attend des calques sans flou ni reflet. Ici il est recréé.

**Blender** reste possible pour un rendu « hero » en PNG, mais il ne sort pas de SVG pour cet effet (Freestyle et Grease Pencil ne font que du trait et des aplats), et il n'a pas été utilisé.

## 8. Contenu de l'archive

| Dossier | Contenu |
| --- | --- |
| `00-final/` | Logo actuel (SVG clair et sombre, en contours et en filtre, + PNG à plusieurs tailles), icône d'application et icône des fichiers `.stf` (SVG clairs) |
| `01-planches/` | Toutes les planches de comparaison (PNG) |
| `02-iterations/` | Toutes les itérations, par série (v, p, q, r, s, t, u, w, candidats 1 à 6), l'ancien logo (`avant-liquid-glass/`) et les essais de mouvement (`mouvement/`) |
| `03-explorations-anterieures/` | Premiers logos et explorations (enclume au trait, chutes, etc.) |
| `04-sous-agents/` | Les explorations des trois sous-agents (géométrique, organique, conceptuel), avec leurs scripts |
| `05-design-system/` | Textes et jetons du design system en ligne |
| `06-scripts/` | Les générateurs Python et les scripts de planches |

**Régénérer le logo actuel** (depuis `06-scripts/` ; Python 3 avec numpy, scipy, Pillow et contourpy, plus `rsvg-convert`). Le résultat est identique à l'octet près :

```bash
SRC=../02-iterations/avant-liquid-glass/stokesforge-logo-clair.svg
OPTS="rimw=1,spec=.65,rim=.65,cres=.55,cast=0,ftile=1"
python3 vector.py $SRC stokesforge-logo-clair.svg --levels 18 --opts $OPTS                # référence, clair
python3 vector.py $SRC stokesforge-logo-sombre.svg --theme dark --levels 18 --opts $OPTS  # référence, sombre
python3 glass.py $SRC stokesforge-logo-clair-filtre.svg --opts $OPTS                      # filtre (--theme dark pour le sombre)
python3 vector.py $SRC stokesforge-icone-clair.svg --levels 18 --opts $OPTS,icon=1,tx=-31.72,ty=-16.72,scale=1.03      # icône d'application
python3 vector.py $SRC stokesforge-fichier-stf-clair.svg --levels 18 --opts $OPTS,sheet=1,tx=13.25,ty=24,scale=.74      # icône des fichiers .stf
```

Les deux dernières (#388) : `icon=1` retire la tuile et son liseré, `sheet=1` pose le dessin sur une feuille à l'horizontale (coin plié en haut à droite, 232 sur 168), `tx`, `ty` et `scale` remplacent la translation et l'échelle du dessin (par défaut `14 6` et `.9`, ceux de la tuile). **Centrage selon le poids visuel** (retour de l'utilisateur) : en largeur, c'est le centre de gravité du dessin, pondéré par l'opacité, qui tombe au milieu, à x = 155,07 en coordonnées du dessin ; la longue corne, fine, ne pèse presque rien et ne doit pas repousser la masse du logo. En hauteur, on centre la zone dessinée (y de 53,1 à 227,9), car l'enclume est lourde par nature. Donc `tx = 128 - scale × 155,07` et `ty = 128 - scale × 140,5`. Pour l'icône, `scale` vaut 1,03 : la pointe de la corne s'arrête à 8 sur 256 du bord gauche, ce qui fixe la taille et laisse plus de vide à droite. Le générateur refuse une icône dont le poids s'écarte de plus de 4 px du milieu.

Essais de mouvement : ajouter `,anim=1` ou `,ripple=2.4` à `OPTS` de `vector.py`. PNG : `resvg -w 1024 entree.svg sortie.png` (resvg 0.45.1).

**Régénérer l'ancien logo plat** (Python 3, depuis `06-scripts/`) :

```bash
SCENE=none LH=112 MODE=plain python3 excite9.py sortie
```

Les scripts s'enchaînent (`make.py` → `make2.py` → `drop3.py` → `drop4.py` → `excite2.py` → … → `excite9.py`) ; chacun réutilise le précédent. Les planches se font avec Node + Playwright (`shot*.js`).

Couleurs : encre `#22262a`, vert `#3a9155`, orange `#e0651a`, fond clair `#f3f1ec`, fond sombre `#15181a`, encre claire `#e6e9e7`.
