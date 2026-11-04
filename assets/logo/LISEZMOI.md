# Logo StokesForge : récapitulatif

Date : 3 novembre 2026. Ce document résume ce qui a été discuté, ce qui est voulu, ce qui a été rejeté, et où en est le logo. Ce dossier est rangé dans le dépôt sous `assets/logo/` (#443) ; l'icône d'application n'en est pas encore tirée (#388).

## 1. L'idée

Une **forge** (enclume + marteau) rencontre un **graphe de spectroscopie**. Le marteau **excite la bande d'absorption** (verte) ; l'énergie ressort en **bande d'émission** (orange), à plus basse énergie. L'écart entre les deux maxima est le **déplacement de Stokes**.

## 2. Physique (non négociable)

- L'axe x se lit en longueur d'onde croissante vers la droite.
- **Absorption = vert**, à gauche : une bande principale et **un seul épaulement vibronique, à gauche** (côté haute énergie). Pas de second pic, pas de creux.
- **Émission = orange**, à droite : **image miroir exacte** de l'absorption (règle du miroir), épaulement **à droite**.
- Les deux maxima se font face ; leur écart est le déplacement de Stokes (34 sur une largeur de 256 dans la version actuelle). Léger recouvrement au pied (transition 0-0).
- Le marteau agit sur **l'absorption** (on excite), jamais sur l'émission : l'émission est la conséquence (relaxation, puis émission décalée vers le rouge).

Formes calculées (scripts) : gaussienne principale σ = 7, épaulement σ = 8, amplitude 0,6, décalé de 17 ; l'émission est le miroir exact.

## 3. Ce qui est voulu (état actuel)

| Élément | Voulu |
| --- | --- |
| Enclume | Silhouette d'enclume, **basse** (hauteur réduite à ~72 %), corne **conique courte et droite** à pointe arrondie (pas une coque de bateau). Une seule marche : corne puis table. **Aucune deuxième marche** sous les courbes. |
| Matière de l'enclume | **Verre sombre « liquide »** (style Liquid Glass d'Apple, mais qui **assombrit** pour faire ressortir le contenu). La lumière des spectres coule **dans** le verre (vert sous l'absorption, orange sous l'émission) et se concentre dans l'arrondi des bords. Les spectres se reflètent dans le verre. **Liquide, pas glossy** : pas de reflets blancs vernis. |
| Jonction courbes / enclume | Les courbes sont **posées directement sur la table**. La ligne de jonction tire vers le **sombre**, pas vers le blanc. Rien ne déborde sous ou à côté des courbes. |
| Courbes | **Formes pleines sans trait coloré** (le mélange trait + remplissage est rejeté). Vert dense ; orange incandescent (orange chaud au sommet, orange profond au pied). Les sommets restent bien lisibles (pas trop clairs). Une fine lueur suit le haut des courbes, sans halo étalé au pied. |
| Axe Y | Flèche d'axe planté sur la table, à gauche des courbes. Reste fixe (sinon l'appli « a l'air cassée »). |
| Marteau | Marteau de forgeron (table de frappe évasée, collet, panne en coin émoussé), tombé depuis la corne, **posé** sur le sommet vert sans s'y enfoncer, **devant** les courbes. Petits éclats orange au contact. Pas de marteaux fantômes de la chute. |
| Lien absorption → émission | **La comète** : un triangle fin qui part de l'impact et finit en **point** sur le sommet orange, dégradé vert → orange. |
| Thème | Travail principal en **clair**. Une version sombre existe. |

## 4. Rejeté ou mis de côté

- Graduations x dans la table, flèche Δ gravée, enclume gravée pleine.
- Spectres à deux bosses (faux physiquement) ; marteau frappant l'émission.
- Marteaux intermédiaires (chronophotographie) : trop chargé.
- Arc de points, flèche courbe en pointillé : lourds, pas propres.
- Petits trous carré / rond dans la table : pas convaincants.
- Étoiles à quatre branches (à revoir plus tard, éventuellement).
- Trait coloré + remplissage ; halos qui « bavent » au pied ; bande blanche à la jonction ; reflet diagonal sur l'enclume ; effet glossy.

## 5. Gardé en réserve (idées appréciées)

- Ciel étoilé discret en version sombre.
- Enclume au voile grisâtre (version nuit).
- Explorations des sous-agents : « Enclume de Jablonski » (diagramme d'énergie dans l'enclume), « L'enclume est le déplacement de Stokes », « Press » (l'axe devient le marteau), médaillon « foyer ».

## 6. Version actuelle

- `00-final/stokesforge-logo-clair.svg` : version de travail (thème clair).
- `00-final/stokesforge-logo-sombre.svg` : même logo sur tuile sombre.
- PNG en 1024, 512, 256, 64 et 32 px à côté.

Ce SVG utilise des dégradés, des masques et des flous (filtres SVG). Il s'affiche correctement dans les navigateurs et Inkscape ; certains moteurs simples ignorent les filtres (le logo reste lisible mais perd la lueur).

## 7. Limites de l'outil et suite (Blender)

**Ce que le SVG ne sait pas faire** : une vraie réfraction (la lumière déviée par un bord arrondi, qui déforme ce qu'il y a derrière). Le rendu actuel l'imite avec des dégradés et des flous ; c'est près du maximum en SVG pur.

**Blender peut-il sortir un SVG ?** Pas pour cet effet. Blender exporte en SVG seulement du **dessin au trait** (Freestyle SVG, Grease Pencil) : contours et aplats, sans matériaux, sans verre, sans lumière. Un rendu « verre liquide » sort de Blender en **image bitmap** (PNG, éventuellement très grande : 2048 ou 4096 px).

Figma : exporte du SVG, mais ses effets de verre / flou d'arrière-plan sont pixellisés ou perdus à l'export SVG. Icon Composer (Apple) : produit des icônes Apple, pas du SVG.

**Stratégie proposée**

1. **SVG = logo de référence** (vectoriel) : la version actuelle, nettoyée, pour la doc, le site, les petites tailles et tous les usages plats.
2. **Blender = rendu « hero »** en PNG haute définition pour l'icône d'application, l'écran « À propos », le site.
3. Les deux partagent exactement la même géométrie (mêmes courbes, même enclume) : les scripts Python génèrent les deux.

**Plan Blender (à faire par Claude, sans compétence Blender requise)**

- Un script Python `bpy` qui construit toute la scène : enclume extrudée depuis le même contour, matériau verre sombre (Principled BSDF, transmission, rugosité faible), courbes en volumes émissifs vert et orange, marteau, axe, comète ; caméra orthographique de face ; éclairage.
- Rendu Cycles, fond transparent, sortie PNG 2048 px.
- Utilisation : installer Blender (gratuit), puis une seule commande : `blender -b -P stokesforge_logo.py`. Le script écrit le PNG.

## 8. Contenu de l'archive

| Dossier | Contenu |
| --- | --- |
| `00-final/` | Logo actuel (SVG clair et sombre + PNG à plusieurs tailles) |
| `01-planches/` | Toutes les planches de comparaison (PNG) |
| `02-iterations/` | Toutes les itérations, par série (v, p, q, r, s, t, u, w, candidats 1 à 6) |
| `03-explorations-anterieures/` | Premiers logos et explorations (enclume au trait, chutes, etc.) |
| `04-sous-agents/` | Les explorations des trois sous-agents (géométrique, organique, conceptuel), avec leurs scripts |
| `05-design-system/` | Textes et jetons du design system en ligne |
| `06-scripts/` | Les générateurs Python et les scripts de planches |

**Régénérer le logo actuel** (Python 3, depuis `06-scripts/`) :

```bash
SCENE=none LH=112 MODE=plain python3 excite9.py sortie
```

Les scripts s'enchaînent (`make.py` → `make2.py` → `drop3.py` → `drop4.py` → `excite2.py` → … → `excite9.py`) ; chacun réutilise le précédent. Les planches se font avec Node + Playwright (`shot*.js`).

Couleurs : encre `#22262a`, vert `#3a9155`, orange `#e0651a`, fond clair `#f3f1ec`, fond sombre `#15181a`, encre claire `#e6e9e7`.
