//! Le modèle de recette : la description figée (sérialisable) de comment un
//! appareil donné écrit ses fichiers, sans rien connaître du moteur de
//! parsing lui-même (voir [`crate::parse`]).

/// Une recette de parsing réutilisable pour un format de fichier d'appareil.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImportRecipe {
    pub name: String,
    pub separator: Separator,
    pub decimal: DecimalStyle,
    pub header: HeaderSkip,
    pub footer_skip_lines: usize,
    pub has_column_header_row: bool,
    pub name_source: NameSource,
    /// Découpage de l'en-tête en en-tête « principal » et « sous-en-tête »,
    /// façon assistant d'import ASCII d'OriginLab (chantier « prévisualisation
    /// d'import ») — voir [`HeaderLayout`]. `Default` (zéro partout) laisse
    /// `header` (ci-dessus) entièrement aux commandes, comme avant ce
    /// chantier : les deux modèles ne se recouvrent que si l'appelant met
    /// `header_layout` à autre chose que zéro. `#[serde(default)]` pour
    /// qu'une recette JSON enregistrée avant ce chantier se recharge à
    /// l'identique (`RecipeStore`, voir sa doc de tête).
    #[serde(default)]
    pub header_layout: HeaderLayout,
    /// Assignation des rôles `long_name`/`unit`/`comment` à des plages de
    /// lignes de sous-en-tête — voir [`SubheaderRoles`]. Sans effet tant que
    /// `header_layout` ne définit aucune ligne de sous-en-tête.
    #[serde(default)]
    pub subheader_roles: SubheaderRoles,
    /// Nombre de caractères à retirer en tête de chaque ligne de données
    /// avant analyse (préfixes fixes de certains formats d'instrument).
    #[serde(default)]
    pub skip_chars: usize,
    /// Nombre de lignes de données à annoter dans l'aperçu en direct
    /// ([`crate::preview::preview`]) — n'affecte jamais [`crate::parse`], qui
    /// analyse toujours le fichier entier. Plafonne le travail de l'aperçu
    /// indépendamment de la taille réelle du fichier.
    #[serde(default = "default_preview_lines")]
    pub preview_lines: usize,
    /// Où s'arrêtent les données (le reste est le pied de page) — voir
    /// [`DataEnd`]. S'applique **après** `footer_skip_lines`, qui continue de
    /// retirer un nombre fixe de lignes en fin de fichier.
    #[serde(default)]
    pub data_end: DataEnd,
}

/// Fin de la zone de données d'un fichier. Les lignes qui suivent la coupure
/// forment le pied de page : jamais importées, mais montrées par l'aperçu
/// ([`crate::preview::LineRole::Footer`]) et gardées par
/// [`crate::parse::ParsedTable::footer_lines`].
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum DataEnd {
    /// Coupure automatique. Elle s'arme à la première ligne dont la première
    /// colonne est un nombre ; une fois armée, elle coupe à la première ligne
    /// dont la première colonne (le X) n'est pas un nombre — une ligne vide
    /// coupe aussi. Une case texte dans une autre colonne ne coupe pas.
    #[default]
    Auto,
    /// Aucune coupure : les données vont jusqu'à la fin du fichier.
    None,
    /// Coupe à la première ligne vide (ou blanche).
    FirstBlankLine,
    /// Coupe à la première ligne égale à ce marqueur (comparaison insensible
    /// à la casse, bordures blanches retirées) ; cette ligne est exclue.
    Marker(String),
    /// Garde seulement les `n` premières lignes de données.
    LinesAfterHeader(usize),
    /// Retire les `n` dernières lignes (comme `footer_skip_lines`).
    LinesBeforeEnd(usize),
}

fn default_preview_lines() -> usize {
    20
}

/// Plage de lignes de sous-en-tête, 1-based et incluse aux deux bornes,
/// **relative au bloc de sous-en-tête** (donc indépendante du nombre de
/// lignes d'en-tête principal qui le précèdent) — c'est le « from … to … »
/// de l'assistant OriginLab pour un rôle donné.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SubheaderLineRange {
    pub from: usize,
    pub to: usize,
}

impl SubheaderLineRange {
    /// Une plage d'une seule ligne — le cas de très loin le plus courant
    /// (l'exemple OriginLab de référence est `18` à `18`).
    pub fn single(line: usize) -> Self {
        Self {
            from: line,
            to: line,
        }
    }
}

/// Combien de lignes d'en-tête « principal » et de « sous-en-tête » compte le
/// fichier, façon assistant d'import ASCII d'OriginLab. Distinct de
/// `ImportRecipe::header` (qui ne connaît qu'un bloc unique à sauter) :
/// seul le sous-en-tête peut porter des rôles assignés par ligne
/// ([`SubheaderRoles`]) — c'est la seule raison de la distinction, PlotYx ne
/// traite pas le contenu de l'en-tête principal autrement que pour le
/// sauter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct HeaderLayout {
    pub main_header_lines: usize,
    pub subheader_lines: usize,
    /// Coché, l'outil recalcule ces deux comptes depuis le contenu du
    /// fichier plutôt que de les garder tels que saisis — voir
    /// `crate::preview::detect_header_layout`. Ce champ ne fait rien tout
    /// seul : c'est la couche appelante (`plotyx-app`) qui, en le voyant
    /// coché, appelle la détection et réécrit `main_header_lines`/
    /// `subheader_lines` — comme le bouton de réévaluation d'OriginLab, la
    /// détection ne s'applique pas en continu à chaque frappe.
    pub auto_detect: bool,
}

/// Séparateur de colonnes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Separator {
    Comma,
    Semicolon,
    Tab,
    Whitespace,
    /// Détecté automatiquement à partir de la première ligne de données.
    Auto,
}

/// Convention de séparateur décimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DecimalStyle {
    Dot,
    Comma,
}

/// Combien de lignes ignorer avant les données.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum HeaderSkip {
    None,
    /// Ignore les `n` premières lignes, quel que soit leur contenu.
    Lines(usize),
    /// Ignore tout jusqu'à la première ligne dont le texte, une fois
    /// bordures blanches retirées, est égal (comparaison insensible à la
    /// casse) à ce marqueur — cette ligne elle-même est ignorée aussi.
    UntilMarker(String),
}

/// D'où vient le nom de la série importée.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NameSource {
    /// Le nom de fichier (sans extension), fourni par l'appelant.
    Filename,
    /// Une valeur lue dans les lignes d'en-tête ignorées : cherche une ligne
    /// « CLÉ<séparateur>VALEUR » où CLÉ correspond à cette chaîne
    /// (insensible à la casse, bordures blanches ignorées des deux côtés),
    /// et prend VALEUR (`trim`ée) comme nom.
    HeaderKey(String),
}

/// Assignation des rôles `long_name`/`unit`/`comment` de
/// [`plotyx_core::ColumnMeta`] (le crate ne dépend pas de `plotyx-core`, voir
/// la doc de tête de [`crate`] — les noms de rôle restent ceux de
/// `ColumnMeta`, pas ceux d'OriginLab, précisément pour que la correspondance
/// soit directe côté `plotyx-app`) à des plages de lignes de sous-en-tête.
///
/// N'a pas d'équivalent pour « System Parameters »/« User Parameters »
/// d'OriginLab, délibérément : voir `docs/chantier-import-previsualisation.md`
/// (décision à ne pas rouvrir sans raison nouvelle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct SubheaderRoles {
    pub long_name: Option<SubheaderLineRange>,
    pub unit: Option<SubheaderLineRange>,
    pub comment: Option<SubheaderLineRange>,
    /// Coché, `unit` est ignoré : chaque jeton de la ligne `long_name` est
    /// lu comme « Nom (unité) », l'unité étant ce qui suit la première
    /// parenthèse ouvrante jusqu'à la première fermante — un jeton sans
    /// parenthèses reconnues garde tout son texte comme nom long et rend une
    /// unité vide, plutôt que de couper arbitrairement (voir
    /// `crate::parse::parse`).
    pub same_line_name_unit: bool,
}

impl ImportRecipe {
    /// Nouvelle recette aux réglages les plus permissifs.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            separator: Separator::Auto,
            decimal: DecimalStyle::Dot,
            header: HeaderSkip::None,
            footer_skip_lines: 0,
            has_column_header_row: false,
            name_source: NameSource::Filename,
            header_layout: HeaderLayout::default(),
            subheader_roles: SubheaderRoles::default(),
            skip_chars: 0,
            preview_lines: default_preview_lines(),
            data_end: DataEnd::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_est_permissive_par_defaut() {
        let recipe = ImportRecipe::new("test");
        assert_eq!(recipe.name, "test");
        assert_eq!(recipe.separator, Separator::Auto);
        assert_eq!(recipe.decimal, DecimalStyle::Dot);
        assert_eq!(recipe.header, HeaderSkip::None);
        assert_eq!(recipe.footer_skip_lines, 0);
        assert!(!recipe.has_column_header_row);
        assert_eq!(recipe.name_source, NameSource::Filename);
        assert_eq!(recipe.header_layout, HeaderLayout::default());
        assert_eq!(recipe.subheader_roles, SubheaderRoles::default());
        assert_eq!(recipe.skip_chars, 0);
        assert_eq!(recipe.preview_lines, 20);
        assert_eq!(recipe.data_end, DataEnd::Auto);
    }

    #[test]
    fn round_trip_json() {
        let recipe = ImportRecipe {
            name: "Jasco".into(),
            separator: Separator::Semicolon,
            decimal: DecimalStyle::Comma,
            header: HeaderSkip::UntilMarker("XYDATA".into()),
            footer_skip_lines: 2,
            has_column_header_row: false,
            name_source: NameSource::HeaderKey("TITLE".into()),
            header_layout: HeaderLayout {
                main_header_lines: 3,
                subheader_lines: 4,
                auto_detect: true,
            },
            subheader_roles: SubheaderRoles {
                long_name: Some(SubheaderLineRange::single(1)),
                unit: Some(SubheaderLineRange { from: 2, to: 2 }),
                comment: None,
                same_line_name_unit: false,
            },
            skip_chars: 5,
            preview_lines: 50,
            data_end: DataEnd::Marker("##### Extended".into()),
        };
        let json = serde_json::to_string(&recipe).unwrap();
        let back: ImportRecipe = serde_json::from_str(&json).unwrap();
        assert_eq!(recipe, back);
    }

    /// Une recette enregistrée par `RecipeStore` **avant** ce chantier (donc
    /// sans `header_layout`/`subheader_roles`/`skip_chars`/`preview_lines`
    /// dans son JSON) doit continuer à se charger, avec les nouveaux champs
    /// à leurs valeurs par défaut — `#[serde(default)]` est ce qui tient
    /// cette promesse, ce test la vérifie plutôt que de la supposer.
    #[test]
    fn recette_json_anterieure_au_chantier_se_charge_avec_les_defauts() {
        let json_avant_chantier = r#"{
            "name": "Jasco UV-Vis",
            "separator": "Semicolon",
            "decimal": "Comma",
            "header": {"Lines": 3},
            "footer_skip_lines": 2,
            "has_column_header_row": true,
            "name_source": "Filename"
        }"#;
        let recipe: ImportRecipe = serde_json::from_str(json_avant_chantier)
            .expect("un JSON sans les nouveaux champs doit rester lisible");
        assert_eq!(recipe.header_layout, HeaderLayout::default());
        assert_eq!(recipe.subheader_roles, SubheaderRoles::default());
        assert_eq!(recipe.skip_chars, 0);
        assert_eq!(recipe.preview_lines, 20);
        assert_eq!(recipe.data_end, DataEnd::Auto);
    }
}
