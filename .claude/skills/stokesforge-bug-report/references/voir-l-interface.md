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
déclarées dans `crates/stokesforge-app/src/app.rs` (`SCREENSHOT_ENV`, `AUTOQUIT_ENV`) :

```sh
STOKESFORGE_APP_SCREENSHOT=/tmp/stokesforge.ppm \
STOKESFORGE_APP_AUTOQUIT_MS=2000 \
cargo run -p stokesforge-app -- /chemin/vers/projet.stf
```

L'application s'ouvre, se photographie, et se ferme toute seule au bout du
délai. Sans `AUTOQUIT`, elle reste ouverte et bloque la session.

Dans une session d'agent sans affichage réellement disponible, la fenêtre
peut ne jamais se peindre : aucun fichier n'est écrit et le processus ne se
ferme pas. Toujours borner la commande (`timeout 60 …`), essayer **une**
fois (au plus une seconde avec `STOKESFORGE_X11=1`), ne pas boucler, et dire
dans le compte rendu que l'aspect n'a pas été regardé. Les tests d'aspect du
module de la surface (formes peintes : fonds, filets, gras via
`test_support::textes`) restent alors le seul filet ; ils prouvent les
formes, pas le rendu final.

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
de photographier la fenêtre en Wayland natif, où aucun outil externe ne peut
photographier une fenêtre donnée (sous X11/Xwayland, `run-stokesforge` le fait).

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

- un état atteint après une interaction (clic droit, repli, saisie) — la
  capture seule ne le montre pas ; [`run-stokesforge`](../../run-stokesforge/SKILL.md)
  pilote l'application (clics, clavier, captures, sous X11/Xwayland) et couvre
  ces cas, **pas** le glisser ;
- une fenêtre OS détachée (`show_viewport_deferred`) ;
- un comportement qui dépend du compositeur (position refusée sous Wayland,
  sélecteur de fichiers natif).

Dans ces cas-là, **la capture de l'utilisateur est la source la plus fiable** —
la demander explicitement plutôt que de deviner :

> « Peux-tu m'envoyer une capture de l'état où ça se produit ? Je préfère
> regarder plutôt que déduire du code. »

Et si le symptôme n'est reproductible que devant un écran réel (Wayland natif,
sélecteur de fichiers, glisser), l'écrire dans le rapport : `run-stokesforge`
tourne en X11 et ne peut pas le valider à la place d'un humain.

## Avant / après

Pour un correctif visuel, deux captures du **même** état valent tous les
paragraphes d'explication — et prouvent le correctif au lieu de l'affirmer.
