# Voir l'interface avant de conclure

Un symptôme visuel ne se diagnostique pas en lisant le code. La leçon vient
d'un cas réel : sur « le contour de surbrillance est mal positionné », une
lecture du code avait conclu que le contour excluait l'en-tête de la fenêtre.
Les captures de l'utilisateur montraient l'inverse — l'en-tête était bien
entouré, c'était le corps de la fenêtre qui débordait, et une fenêtre repliée
gardait un cadre à sa taille dépliée. **Un raisonnement juste sur un code mal
compris donne une cause fausse, et elle est convaincante.**

Ça vaut pour tout ce qui se décrit avec un mot visuel : position, contour,
alignement, troncature, chevauchement, couleur, surbrillance, taille, ombre,
clignotement, rafraîchissement.

## Le mécanisme intégré

`stokesforge-app` sait se photographier lui-même. Deux variables d'environnement,
déclarées à `crates/stokesforge-app/src/app.rs:341-344` :

```sh
STOKESFORGE_APP_SCREENSHOT=/tmp/stokesforge.ppm \
STOKESFORGE_APP_AUTOQUIT_MS=2000 \
cargo run -p stokesforge-app -- /chemin/vers/projet.stokesforge
```

L'application s'ouvre, se photographie, et se ferme toute seule au bout du
délai. Sans `AUTOQUIT`, elle reste ouverte et bloque la session.

**Le format est du PPM binaire (`P6`)**, choisi pour ne coûter aucune
dépendance — sept lignes de code. Il n'est pas lisible directement :

```sh
magick /tmp/stokesforge.ppm /tmp/stokesforge.png
```

Puis lire le PNG. C'est cette étape qui permet de *regarder* au lieu de
supposer.

La doc de tête de `StokesForgeApp::screenshot` dit pourquoi ce mécanisme existe, et
c'est la phrase à retenir : « les tests couvrent la géométrie
(`stokesforge-render-egui`), pas l'apparence ». Un `cargo test --workspace` vert ne
dit **rien** sur ce à quoi la fenêtre ressemble. C'est aussi le seul moyen
d'accéder à une fenêtre sous Wayland, où aucun outil externe ne peut
photographier une fenêtre donnée.

## En pseudo-langue, pour les textes

Pour un symptôme de **texte** (libellé tronqué, texte qui n'a pas été traduit,
bouton trop étroit), lancer la même capture avec `STOKESFORGE_LANG=en-XA` :

```sh
STOKESFORGE_LANG=en-XA STOKESFORGE_APP_SCREENSHOT=/tmp/stokesforge.ppm \
STOKESFORGE_APP_AUTOQUIT_MS=2000 cargo run -p stokesforge-app
```

Chaque texte qui vient du catalogue s'affiche accentué, allongé de 30 % et
encadré de `[[ ]]`. Un texte **sans** `[[ ]]` est écrit en dur dans le code (il a
échappé à `tr!`) ; un `]]` **absent** est un texte coupé par la mise en page —
le français, plus long que l'anglais, le couperait aussi.

## Ce que la capture ne donne pas

Elle photographie **un** état, à **un** instant, dans la fenêtre principale.
Elle ne montre pas :

- un état atteint après une interaction (glisser, clic droit, repli) — il
  faudrait piloter l'application, ce qu'aucun outil du dépôt ne fait ;
- une fenêtre OS détachée (`show_viewport_deferred`) ;
- un comportement qui dépend du compositeur (position refusée sous Wayland,
  sélecteur de fichiers natif).

Dans ces cas-là, **la capture de l'utilisateur est la source la plus fiable** —
la demander explicitement plutôt que de deviner :

> « Peux-tu m'envoyer une capture de l'état où ça se produit ? Je préfère
> regarder plutôt que déduire du code. »

Et si le symptôme n'est reproductible que devant un écran, l'écrire dans le
rapport : aucun outil non interactif de ce dépôt ne peut le valider à la place
d'un humain.

## Avant / après

Pour un correctif visuel, deux captures du **même** état valent tous les
paragraphes d'explication — et prouvent le correctif au lieu de l'affirmer.
