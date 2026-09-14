//! Le levier d'anticrénelage — tâche R5.5 bis.
//!
//! # Ce que la mesure R0 0.3 a trouvé, et que le plan n'avait pas prévu
//!
//! Le *feathering* — la frange d'un pixel qu'`epaint` ajoute à chaque trait
//! pour l'adoucir — pèse **78 % du coût de tessellation** : 22,1 ms par
//! image avec, **4,88 ms sans**, sur le même graphe de 300 000 points. Le
//! couper fait donc repasser le cas de charge du document *sous* le seuil de
//! 8 ms, là où le LTTB n'y retirerait rien ([`plotyx_render::decimate`]).
//! C'est le levier le plus rentable du chantier, et il n'était pas dans la
//! liste du §4.2 d'`architecture-choisie.md` avant la mesure.
//!
//! **Décidé le 2026-09-10** : il se coupe à l'entrée en interaction souris,
//! se rétablit au relâchement. Le crénelage ne se voit pas sur une image qui
//! bouge, et c'est exactement pendant le déplacement que les 22,1 ms font
//! mal. Ce n'est donc pas un réglage exposé à l'utilisateur, mais un **état
//! du rendu**.
//!
//! # Ce que R5 construit, et ce que R7.3 a câblé par-dessus
//!
//! R5 pose **le levier**, pas l'événement qui l'actionne : détecter un
//! glisser était le travail de la phase R7 (Interaction), câblé depuis.
//! La frontière est restée nette :
//!
//! - **R5 fournit** [`Feathering`], [`apply`] et [`current`] : de quoi dire
//!   « dessine sans frange » / « dessine avec », et savoir où on en est ;
//! - **R7.3 fournit l'appel**, dans `lib.rs` : à l'entrée du glisser
//!   (`Response::dragged()` — pan ou rectangle de zoom, les deux comptent),
//!   `apply(ctx, Feathering::Off)` ; au relâchement,
//!   `apply(ctx, Feathering::Full)`. Rien d'autre n'était à écrire ici, et ce
//!   module n'a pas changé depuis.
//!
//! # Pourquoi ce n'est pas dans [`crate::cache`], ni dans `plotyx-render`
//!
//! Deux frontières, chacune pour une raison différente, et toutes deux
//! faciles à franchir par inadvertance :
//!
//! - **pas dans la clé de cache.** Couper la frange change la
//!   *tessellation*, pas les `Shape` — et la tessellation a lieu à chaque
//!   image de toute façon. Ajouter un terme à la clé
//!   `(revision, rect, pixels_per_point)` ferait reconstruire toute la
//!   figure à chaque pression de bouton, c'est-à-dire payer cher
//!   exactement au moment qu'on voulait alléger. `architecture-choisie.md`
//!   §4.2 le dit explicitement ; [`crate::cache`] le redit ; et
//!   `tests/feathering.rs` le vérifie, parce qu'un troisième rappel écrit
//!   ne vaut pas une vérification ;
//! - **pas dans `plotyx-render`.** [`egui::epaint::TessellationOptions`] est
//!   un réglage de la pile egui, en **aval** de la collecte des `Shape`.
//!   `plotyx-render` ne connaît ni `egui` ni le `Context`, et
//!   `plotyx-render-svg` n'a pas de tessellation du tout : un SVG décrit des
//!   courbes, il ne les triangule pas. C'est le seul endroit de R5 où la
//!   règle « aucune fonctionnalité dans un seul backend » ne s'applique pas,
//!   et ce n'est pas une entorse : il n'y a rien à mettre de l'autre côté.
//!
//! # Pourquoi c'est un réglage de `Context` et non de [`crate::GraphView`]
//!
//! `TessellationOptions` vit dans les options du `Context`, donc pour toute
//! la fenêtre. Une `GraphView` qui le poserait au moment de peindre le
//! poserait pour tous les autres widgets de l'image, et deux vues en
//! poseraient deux valeurs contradictoires. L'appelant est le seul à savoir
//! quand une interaction commence ; la fonction est donc libre, et prend le
//! `Context` en paramètre plutôt que d'aller le chercher.

/// L'état de la frange d'anticrénelage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Feathering {
    /// Frange active : la qualité pleine, le régime de repos.
    #[default]
    Full,
    /// Frange coupée : le régime d'interaction, ~4,5× moins cher à
    /// tesseler.
    Off,
}

impl Feathering {
    /// Vrai si la frange est active.
    pub fn is_on(self) -> bool {
        matches!(self, Feathering::Full)
    }

    /// Le régime correspondant à « une interaction est en cours ».
    ///
    /// Sert au câblage R7.3 (`lib.rs`) en une ligne, sans avoir à se
    /// rappeler dans quel sens va le booléen.
    pub fn during_drag(dragging: bool) -> Self {
        if dragging {
            Feathering::Off
        } else {
            Feathering::Full
        }
    }
}

/// Pose le régime d'anticrénelage sur un contexte egui.
///
/// Rend le régime **précédent**, pour que l'appelant puisse le rétablir sans
/// avoir à le mémoriser lui-même.
pub fn apply(context: &egui::Context, feathering: Feathering) -> Feathering {
    context.options_mut(|options| {
        let previous = from_flag(options.tessellation_options.feathering);
        options.tessellation_options.feathering = feathering.is_on();
        previous
    })
}

/// Le régime en vigueur sur ce contexte.
pub fn current(context: &egui::Context) -> Feathering {
    context.options(|options| from_flag(options.tessellation_options.feathering))
}

fn from_flag(on: bool) -> Feathering {
    if on {
        Feathering::Full
    } else {
        Feathering::Off
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_regime_par_defaut_est_la_qualite_pleine() {
        assert_eq!(Feathering::default(), Feathering::Full);
        assert!(Feathering::default().is_on());
    }

    #[test]
    fn le_sens_du_declencheur_de_r7_est_ecrit_une_fois() {
        assert_eq!(Feathering::during_drag(true), Feathering::Off);
        assert_eq!(Feathering::during_drag(false), Feathering::Full);
    }
}
