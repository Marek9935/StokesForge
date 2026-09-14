//! Choix de police — texte et Math — embarquée ou système.
//!
//! Tâche ponctuelle du 2026-09-13, hors séquencement R0-R8 : embarquer trois
//! paires police-de-texte-sans-serif + police-Math (Libertinus, Fira, Noto)
//! et donner à [`text::font_definitions`] un vrai choix plutôt que le
//! `FontDefinitions::default()` d'epaint qu'elle rendait jusqu'ici sans
//! discussion. Voir `docs/archive/newarchitecture/architecture-choisie.md` §7 pour
//! la décision documentée et sa date : elle remplace la phrase « Fonts
//! embarquées uniquement, scan système désactivé », qui datait d'avant cette
//! tâche.
//!
//! # Ce que ce module garantit, et pourquoi c'est le seul endroit qui compte
//!
//! [`text::font_definitions`] reste la **source unique de mesure de texte**
//! (`text.rs`, tâche R4.1) : l'écran et l'export continuent d'appeler
//! exactement la même fonction, donc de partager le même jeu de polices,
//! quel que soit le choix demandé. Ce module ne casse pas cet invariant —
//! il lui donne un paramètre ([`FontChoice`]) au lieu de le figer sur le
//! défaut d'epaint. Un appelant qui change de police sans passer le même
//! [`FontChoice`] des deux côtés reproduirait exactement le défaut que
//! `text.rs` existe pour empêcher ; ce module ne peut rien y faire seul,
//! d'où l'unique point d'entrée [`resolve`] plutôt que deux fonctions
//! séparées « pour l'écran » et « pour l'export ».
//!
//! # Les trois paires embarquées
//!
//! Sous licence SIL Open Font License 1.1 (vérifiée sur chaque source,
//! détail et provenance dans `vendor/fonts/README.md`) :
//!
//! | [`FontSet`] | Texte | Math |
//! |---|---|---|
//! | [`FontSet::Libertinus`] | Libertinus Sans | Libertinus Math |
//! | [`FontSet::Fira`] | Fira Sans | Fira Math |
//! | [`FontSet::Noto`] | Noto Sans | Noto Sans Math |
//!
//! [`FontSet::Noto`] est le [`Default`] du type : c'est la police utilisée
//! quand rien n'est demandé, et celle sur laquelle on replie silencieusement
//! quand une police système demandée est introuvable — les deux rôles que la
//! demande utilisateur assigne à Noto.
//!
//! # Police système, et repli
//!
//! [`FontChoice::System`] désigne une police par son nom (« Calibri », par
//! exemple). La résolution ([`resolve`]) la cherche sur la machine courante
//! via `fontdb` — le même mécanisme, en substance, que celui que
//! `plotyx-render-svg::pdf` utilise déjà côté export pour ne pas perdre le
//! texte silencieusement (`architecture-choisie.md` §5, R6). Introuvable ⇒
//! repli sur [`FontSet::Noto`], sans erreur : un nom de police qui ne
//! correspond à rien sur la machine où le projet est rouvert ne doit ni
//! planter, ni faire disparaître le texte, ni remonter une erreur qu'aucun
//! utilisateur de graphe ne doit avoir à traiter.
//!
//! # Ce qui n'est pas ici
//!
//! Rien ne câble ce mécanisme dans `plotyx_plot::PlotSpec` : le spec n'a
//! aujourd'hui aucun champ de police (vérifié dans
//! `crates/plotyx-plot/src/{spec,series,axis,panel,legend,style}.rs`), et
//! lui en ajouter un toucherait au format `.plx` persisté — une décision
//! hors du périmètre de cette tâche (`CLAUDE.md`, piège « format `.plx` »).
//! [`resolve`] est écrit pour qu'un futur champ n'ait qu'à lui passer un
//! [`FontChoice`], sans reprise.
//!
//! Le texte ordinaire (graduations, titres d'axe) continue de passer par les
//! fonts egui et jamais par Typst : seule une formule (`$...$`, R8, feature
//! `typst`, `architecture-choisie.md` §4.3) consomme les polices Math, via
//! [`crate::formula::resolve_fonts_with_math`]. [`FontSet::math_bytes`] les
//! expose donc en octets bruts — pas en `epaint::FontData`, puisque rien ne
//! consomme ce type pour du Math — pour ce seul appelant.

use std::sync::Arc;

use epaint::text::{FontData, FontDefinitions, FontFamily};

/// Une des trois paires police-de-texte-sans-serif + police-Math embarquées
/// dans le binaire (`vendor/fonts/`, voir le manifeste à côté).
///
/// [`Default`] vaut [`FontSet::Noto`] : c'est la police utilisée quand rien
/// n'est demandé, et celle sur laquelle [`resolve`] replie silencieusement
/// une police système introuvable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontSet {
    /// Libertinus Sans (texte) + Libertinus Math.
    Libertinus,
    /// Fira Sans (texte) + Fira Math.
    Fira,
    /// Noto Sans (texte) + Noto Sans Math — le défaut et le repli.
    Noto,
}

impl FontSet {
    /// Les trois paires, dans l'ordre où la demande utilisateur les cite.
    pub const ALL: [FontSet; 3] = [FontSet::Libertinus, FontSet::Fira, FontSet::Noto];

    /// Le nom de famille de la police de texte, tel qu'embarqué dans le
    /// fichier lui-même (vérifié à la récupération, voir le manifeste).
    pub fn sans_family_name(self) -> &'static str {
        match self {
            FontSet::Libertinus => "Libertinus Sans",
            FontSet::Fira => "Fira Sans",
            FontSet::Noto => "Noto Sans",
        }
    }

    /// Le nom de famille de la police Math associée.
    pub fn math_family_name(self) -> &'static str {
        match self {
            FontSet::Libertinus => "Libertinus Math",
            FontSet::Fira => "Fira Math",
            FontSet::Noto => "Noto Sans Math",
        }
    }

    /// Les octets bruts du fichier de police de texte, embarqués dans le
    /// binaire à la compilation.
    pub fn sans_bytes(self) -> &'static [u8] {
        match self {
            FontSet::Libertinus => {
                include_bytes!("../../../vendor/fonts/libertinus-sans/LibertinusSans-Regular.otf")
            }
            FontSet::Fira => {
                include_bytes!("../../../vendor/fonts/fira-sans/FiraSans-Regular.ttf")
            }
            FontSet::Noto => {
                include_bytes!("../../../vendor/fonts/noto-sans/NotoSans-Variable.ttf")
            }
        }
    }

    /// Les octets bruts du fichier de police Math, embarqués dans le binaire
    /// à la compilation.
    ///
    /// Seul appelant : [`crate::formula::resolve_fonts_with_math`] (R8,
    /// feature `typst`) — voir la documentation du module. Octets bruts et
    /// non `epaint::FontData` : c'est ce consommateur Typst qui décide de la
    /// forme qu'il lui faut, pas ce module.
    pub fn math_bytes(self) -> &'static [u8] {
        match self {
            FontSet::Libertinus => {
                include_bytes!("../../../vendor/fonts/libertinus-math/LibertinusMath-Regular.otf")
            }
            FontSet::Fira => {
                include_bytes!("../../../vendor/fonts/fira-math/FiraMath-Regular.otf")
            }
            FontSet::Noto => {
                include_bytes!("../../../vendor/fonts/noto-sans-math/NotoSansMath-Regular.ttf")
            }
        }
    }
}

impl Default for FontSet {
    /// Noto Sans / Noto Sans Math — le point 1 de la demande utilisateur.
    fn default() -> Self {
        FontSet::Noto
    }
}

/// La police de texte demandée à [`resolve`].
///
/// [`Default`] vaut [`FontChoice::Embedded`]`(`[`FontSet::Noto`]`)` : sans
/// rien spécifier, PlotYx utilise Noto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontChoice {
    /// Une des trois paires embarquées, désignée explicitement.
    Embedded(FontSet),
    /// Une police du système, par son nom exact (par exemple `"Calibri"`).
    ///
    /// « Il doit toujours être possible de récupérer/utiliser les polices
    /// système à la place » : ce n'est donc pas une police à défaut de
    /// mieux, mais un choix de premier ordre, au même titre qu'une des
    /// trois paires embarquées. Introuvable sur la machine courante ⇒
    /// [`resolve`] replie silencieusement sur [`FontSet::Noto`].
    System(String),
}

impl Default for FontChoice {
    fn default() -> Self {
        FontChoice::Embedded(FontSet::default())
    }
}

/// D'où vient réellement la police de texte après résolution — pour un
/// appelant qui veut savoir si un repli a eu lieu (diagnostics, tests),
/// sans que cela change la [`FontDefinitions`] rendue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontSource {
    /// Une des trois paires embarquées, telle que demandée.
    Embedded(FontSet),
    /// La police système demandée a été trouvée et est utilisée telle
    /// quelle.
    System(String),
    /// La police système demandée était introuvable sur cette machine : on a
    /// replié sur Noto, silencieusement. Le nom conservé est celui qui a été
    /// demandé, pas celui utilisé — utile pour un diagnostic, jamais montré
    /// comme une erreur à l'utilisateur.
    FallenBackToNoto {
        /// Le nom qui avait été demandé et n'a pas été trouvé.
        requested: String,
    },
}

/// Le résultat de [`resolve`] : les [`FontDefinitions`] réellement à poser
/// sur un `epaint::Fonts` (écran ou export, indifféremment — c'est tout
/// l'invariant de ce module), la provenance de la police de texte, et la
/// police Math à associer, pour le consommateur Typst (R8).
#[derive(Debug, Clone)]
pub struct ResolvedFonts {
    /// À poser tel quel sur un `epaint::Fonts` ou un `egui::Context`.
    pub definitions: FontDefinitions,
    /// D'où vient la police de texte utilisée.
    pub source: FontSource,
    /// La police Math à associer à cette résolution.
    ///
    /// Pour un choix embarqué, c'est la Math de la même paire. Pour une
    /// police système — trouvée ou non — c'est [`FontSet::Noto`] : une
    /// police système n'a pas de compagne Math, et Noto est déjà le repli
    /// du reste du mécanisme. L'appelant Typst (R8,
    /// `crate::formula::resolve_fonts_with_math`) a ainsi toujours une
    /// police Math cohérente à utiliser, sans avoir à deviner.
    pub math: FontSet,
}

/// Résout un [`FontChoice`] en polices réellement utilisables.
///
/// **Le** point d'entrée de ce module : un appelant qui a besoin de
/// [`FontDefinitions`] (pour semer un `egui::Context`, ou pour construire un
/// [`text::TextMeasurer`] côté export) passe toujours par ici, avec le même
/// [`FontChoice`] des deux côtés — c'est ce qui tient l'invariant de
/// `text.rs`.
///
/// La recherche d'une police système (`FontChoice::System`) scanne le
/// système de fichiers (`fontdb`, chargement des polices système) : ne pas
/// appeler en boucle serrée, comme pour [`text::TextMeasurer`] déjà.
pub fn resolve(choice: &FontChoice) -> ResolvedFonts {
    match choice {
        FontChoice::Embedded(set) => ResolvedFonts {
            definitions: definitions_for(set.sans_family_name(), set.sans_bytes(), None),
            source: FontSource::Embedded(*set),
            math: *set,
        },
        FontChoice::System(name) => match load_system_font(name) {
            Some(data) => ResolvedFonts {
                definitions: definitions_for(name, &[], Some(data)),
                source: FontSource::System(name.clone()),
                math: FontSet::Noto,
            },
            None => ResolvedFonts {
                definitions: definitions_for(
                    FontSet::Noto.sans_family_name(),
                    FontSet::Noto.sans_bytes(),
                    None,
                ),
                source: FontSource::FallenBackToNoto {
                    requested: name.clone(),
                },
                math: FontSet::Noto,
            },
        },
    }
}

/// Construit des [`FontDefinitions`] dont la famille [`FontFamily::Proportional`]
/// commence par `name`, suivie des secours d'epaint déjà présents (emoji,
/// `Ubuntu-Light`) — inchangés, pour ne pas perdre la couverture de
/// caractères qu'ils offrent sur ce que `name` ne couvre pas. La famille
/// [`FontFamily::Monospace`] n'est pas touchée : elle est hors du périmètre
/// de cette tâche (embed « sans-serif + Math », pas monospace).
///
/// `bytes` est ignoré (et peut être vide) quand `owned` porte déjà les
/// données — c'est le cas d'une police système, dont les octets viennent de
/// `fontdb` et non d'un `include_bytes!`.
fn definitions_for(name: &str, bytes: &'static [u8], owned: Option<FontData>) -> FontDefinitions {
    let mut definitions = FontDefinitions::default();
    let data = owned.unwrap_or_else(|| FontData::from_static(bytes));
    definitions
        .font_data
        .insert(name.to_owned(), Arc::new(data));

    let mut proportional = vec![name.to_owned()];
    proportional.extend(
        definitions
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
    definitions
        .families
        .insert(FontFamily::Proportional, proportional);

    definitions
}

/// Cherche une police système par son nom exact, et rend ses octets bruts
/// (avec l'index de face à l'intérieur du fichier) si elle existe.
///
/// `None` couvre aussi bien « aucune police de ce nom » que « trouvée, mais
/// ses octets n'ont pas pu être lus » : dans les deux cas, [`resolve`]
/// replie sur Noto sans faire de différence, exactement comme la demande
/// utilisateur le prescrit (« sans erreur bloquante, sans texte perdu »).
///
/// `pub(crate)` (et non privée) depuis la tâche du 2026-09-13 sur
/// `formula.rs` : Typst a besoin des mêmes octets bruts pour construire ses
/// propres `typst::text::Font`, texte et Math — même mécanisme de
/// résolution système, pas un second à écrire pour ce seul consommateur.
pub(crate) fn load_system_font(name: &str) -> Option<FontData> {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();

    let query = fontdb::Query {
        families: &[fontdb::Family::Name(name)],
        ..fontdb::Query::default()
    };
    let id = database.query(&query)?;
    database.with_face_data(id, |bytes, index| {
        let mut data = FontData::from_owned(bytes.to_vec());
        data.index = index;
        data
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Point 1 de la demande utilisateur : sans rien spécifier, c'est Noto.
    #[test]
    fn sans_rien_demander_le_choix_par_defaut_est_noto() {
        assert_eq!(FontChoice::default(), FontChoice::Embedded(FontSet::Noto));
        let resolved = resolve(&FontChoice::default());
        assert_eq!(resolved.source, FontSource::Embedded(FontSet::Noto));
        assert_eq!(resolved.math, FontSet::Noto);
        assert!(resolved
            .definitions
            .font_data
            .contains_key(FontSet::Noto.sans_family_name()));
    }

    /// Une police système qui ne peut exister sur aucune machine de test
    /// (nom délibérément absurde) doit replier sur Noto sans paniquer ni
    /// remonter d'erreur.
    #[test]
    fn une_police_systeme_introuvable_replie_silencieusement_sur_noto() {
        let nom = "Police PlotYx Qui Ne Peut Pas Exister — Test R.13";
        let resolved = resolve(&FontChoice::System(nom.to_owned()));
        assert_eq!(
            resolved.source,
            FontSource::FallenBackToNoto {
                requested: nom.to_owned()
            }
        );
        assert_eq!(resolved.math, FontSet::Noto);
        assert!(resolved
            .definitions
            .font_data
            .contains_key(FontSet::Noto.sans_family_name()));
        // Le premier choix de la famille proportionnelle doit être Noto, pas
        // seulement présent quelque part dans la table des polices.
        assert_eq!(
            resolved
                .definitions
                .families
                .get(&FontFamily::Proportional)
                .and_then(|fonts| fonts.first())
                .map(String::as_str),
            Some(FontSet::Noto.sans_family_name())
        );
    }

    /// Les trois paires embarquées se résolvent chacune sur leur propre
    /// police, texte et Math cohérentes entre elles.
    #[test]
    fn chaque_paire_embarquee_se_resout_sur_elle_meme() {
        for set in FontSet::ALL {
            let resolved = resolve(&FontChoice::Embedded(set));
            assert_eq!(resolved.source, FontSource::Embedded(set));
            assert_eq!(resolved.math, set);
            assert!(resolved
                .definitions
                .font_data
                .contains_key(set.sans_family_name()));
            assert_eq!(
                resolved
                    .definitions
                    .families
                    .get(&FontFamily::Proportional)
                    .and_then(|fonts| fonts.first())
                    .map(String::as_str),
                Some(set.sans_family_name())
            );
        }
    }

    /// Les six fichiers de police embarqués sont bien des polices — pas des
    /// octets vides ni un placeholder : chacun doit parser avec `skrifa`,
    /// la même bibliothèque qu'utilise `epaint` pour les mettre en page.
    #[test]
    fn les_six_fichiers_embarques_sont_des_polices_valides() {
        for set in FontSet::ALL {
            for bytes in [set.sans_bytes(), set.math_bytes()] {
                assert!(!bytes.is_empty(), "{set:?} : fichier vide");
                skrifa::FontRef::new(bytes)
                    .unwrap_or_else(|err| panic!("{set:?} : police invalide ({err})"));
            }
        }
    }

    /// Les noms de famille annoncés par [`FontSet`] correspondent à ce que
    /// le fichier de police lui-même déclare — sinon `fontdb` (côté export,
    /// `plotyx-render-svg::pdf`) et `epaint` (côté écran) ne s'accorderaient
    /// pas sur le nom à chercher.
    #[test]
    fn les_noms_de_famille_correspondent_au_contenu_du_fichier() {
        use skrifa::raw::TableProvider;
        use skrifa::string::StringId;

        for set in FontSet::ALL {
            for (bytes, attendu) in [
                (set.sans_bytes(), set.sans_family_name()),
                (set.math_bytes(), set.math_family_name()),
            ] {
                let font = skrifa::FontRef::new(bytes)
                    .unwrap_or_else(|err| panic!("{set:?} : police invalide ({err})"));
                let name = font.name().unwrap_or_else(|err| {
                    panic!("{set:?} : table `name` absente ({err})");
                });
                let trouve = name
                    .name_record()
                    .iter()
                    .filter(|record| {
                        record.name_id() == StringId::TYPOGRAPHIC_FAMILY_NAME
                            || record.name_id() == StringId::FAMILY_NAME
                    })
                    .any(|record| {
                        record
                            .string(name.string_data())
                            .map(|s| s.to_string() == attendu)
                            .unwrap_or(false)
                    });
                assert!(
                    trouve,
                    "{set:?} : aucun nom de famille du fichier n'égale « {attendu} »"
                );
            }
        }
    }
}
