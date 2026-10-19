# Fiche : icônes

Quand la lire : on affiche une icône, on ajoute une action (elle a un logo prévu), on ajoute une icône.

## Où lire la règle

- Quand mettre une icône : `docs/icones.md` § Quand mettre une icône — et quand non.
- Chaque action a son logo (#110) : `docs/icones.md` § Chaque action a son logo (règle posée avec #110).
- Couleur : `docs/icones.md` § Couleur : neutre par défaut, couleur rare.
- Ajouter une icône (planche-contact à faire valider) : `docs/icones.md` § Ajouter une icône.
- Composant et garde : `docs/icones.md` § Garde-fous ; `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, point `toute icône passe par icons::icon_image`.

## À vérifier

- [ ] `icons::icon_image` / `icon_button`, info-bulle `tr!`, jamais un glyphe Unicode ni un emoji
- [ ] l'icône est affichée seulement si elle apporte quelque chose
- [ ] le logo prévu de l'action est nommé dans le compte rendu
- [ ] rôle de couleur `IconRole::Neutral` sauf type d'objet, action destructive ou état
