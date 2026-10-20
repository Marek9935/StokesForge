# Fiche : réglage de couleur

Quand la lire : un champ de couleur du modèle ou de l'interface, le sélecteur, une règle de couleur globale.

## Où lire la règle

- Sélecteur commun, usages, couleurs enregistrées, sélecteur replié, `egui::Area` plutôt que `egui::Popup` : `docs/application/graphe.md` § Propriétés des graphiques (paragraphe « Custom color ») ; `.claude/conventions.md` § Application (`stokesforge-app` et `stokesforge-app-state`) : briques et gardes, point `color_picker::color_field`.
- Couleur vide = automatique, encre automatique : `docs/rendu.md` § Couleurs du texte et des échelles : l'encre automatique.
- Couleur globale / thème (`apply_theme`, `set_text_color`) : `docs/application/graphe.md` § Propriétés des graphiques.
- Persistance d'un champ neuf (`#[serde(default)]`) : `docs/persistance.md` § Le format ne s'écrit pas dans le modèle.
- Échantillons : `docs/icones.md` § Échantillons de couleur (#89).

## À vérifier

- [ ] `color_picker::color_field` avec le bon `ColorUsage` et le `PickerEnv`, jamais un champ `#RRGGBB`
- [ ] vide = automatique, résolu dans `stokesforge-render`, pas dans un backend
- [ ] un élément lourd du sélecteur va dans la partie dépliée
- [ ] nouveau champ de couleur du modèle : `#[serde(default)]` et ajout à `stokesforge-plot::theme`
- [ ] menu déroulant dans une fenêtre surgissante : `egui::Area` propre, pas `egui::Popup`
