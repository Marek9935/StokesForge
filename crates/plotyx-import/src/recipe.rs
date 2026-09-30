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
    /// Conserver l'en-tête du fichier comme métadonnées de chaque colonne
    /// (vrai par défaut : tout est conservé). Voir [`crate::metadata`].
    #[serde(default = "default_true")]
    pub keep_header_metadata: bool,
    /// Conserver aussi le pied de page (les lignes après la fin des données,
    /// [`DataEnd`]) comme métadonnées. Faux par défaut.
    #[serde(default)]
    pub keep_footer_metadata: bool,
    /// Clés ou sections à ne **pas** conserver (nom de clé, ou titre de
    /// section entre crochets : toute la section est alors écartée),
    /// comparés sans casse ni bordures blanches.
    #[serde(default)]
    pub excluded_metadata: Vec<String>,
    /// Rôle choisi pour chaque colonne du fichier, par position (étape
    /// « Colonnes » de l'assistant). Plus court que le nombre de colonnes :
    /// les colonnes suivantes sont en [`ColumnRoleChoice::Auto`].
    #[serde(default)]
    pub column_roles: Vec<ColumnRoleChoice>,
    /// Séparateur de milliers à ignorer dans les nombres (`' '`, `','`, `'.'`,
    /// `'\''`…), `None` s'il n'y en a pas.
    #[serde(default)]
    pub thousands: Option<char>,
    /// Jetons qui valent « valeur manquante » (`--`, `n/a`, `*`, `NaN`…) : la
    /// cellule reste vide au lieu de devenir du texte (et une colonne mixte).
    #[serde(default)]
    pub missing_tokens: Vec<String>,
    /// Préfixe des lignes de commentaire à ignorer dans la zone de données
    /// (`#`, `;`, `//`…), vide s'il n'y en a pas. Une ligne qui commence (après
    /// ses espaces) par ce préfixe n'est jamais une donnée.
    #[serde(default)]
    pub comment_prefix: String,
    /// Nombre de lignes de données à ignorer au début (après l'en-tête et la
    /// ligne de noms de colonnes) : avec [`DataEnd`], la plage de lignes à
    /// importer.
    #[serde(default)]
    pub skip_data_rows: usize,
    /// Encodage du fichier ; `Auto` le détecte ([`crate::decode`]).
    #[serde(default)]
    pub encoding: Encoding,
    /// Critères de choix automatique de ce modèle d'après un fichier — voir
    /// [`crate::choose_recipe`]. Vide : jamais choisi automatiquement.
    #[serde(default)]
    pub auto_match: AutoMatch,
    /// Nature forcée de chaque colonne, par position (étape « Colonnes »).
    /// Plus court que le nombre de colonnes : les suivantes sont `Auto`.
    #[serde(default)]
    pub column_kinds: Vec<ColumnKind>,
    /// Où les colonnes importées atterrissent dans le classeur actif — voir
    /// [`ImportMode`]. Sans effet pour un nouveau classeur.
    #[serde(default)]
    pub import_mode: ImportMode,
    /// Comment plusieurs fichiers se répartissent en feuilles — voir
    /// [`FilesLayout`].
    #[serde(default)]
    pub files_layout: FilesLayout,
    /// Comment un fichier qui contient **plusieurs blocs de données** (un
    /// spectre après l'autre) se découpe — voir [`BlockSplit`]. Chaque bloc est
    /// analysé avec la même recette, son propre en-tête compris.
    #[serde(default)]
    pub block_split: BlockSplit,
}

/// Découpage d'un fichier en blocs de données indépendants.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum BlockSplit {
    /// Un seul bloc : tout le fichier.
    #[default]
    None,
    /// Un nouveau bloc commence à chaque ligne qui débute par ce texte (sans
    /// casse, bordures blanches retirées) — par exemple `TITLE` pour un fichier
    /// Jasco qui enchaîne plusieurs spectres, chacun avec son en-tête.
    BeforeLine(String),
    /// Les blocs sont séparés par une ou plusieurs lignes vides.
    BlankLines,
}

/// Mode d'import dans le classeur actif, comme les options d'Origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ImportMode {
    /// Une nouvelle feuille (le comportement d'origine).
    #[default]
    NewSheet,
    /// Remplace le contenu de la feuille active.
    ReplaceSheet,
    /// Ajoute des colonnes à la suite de la feuille active.
    AddColumns,
    /// Ajoute des lignes à la suite de la feuille active : la colonne `j` du
    /// fichier prolonge la colonne `j` de la feuille.
    AddRows,
}

/// Répartition de plusieurs fichiers en feuilles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FilesLayout {
    /// Une feuille par fichier.
    #[default]
    SheetPerFile,
    /// Tous les fichiers dans une même feuille, côte à côte (X Y X Y…).
    SideBySide,
    /// Tous les fichiers dans une même feuille avec le **X du premier fichier
    /// partagé** : une colonne X, puis les autres colonnes de chaque fichier.
    SharedX,
}

/// Nature d'une colonne importée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ColumnKind {
    /// Nombres ; un jeton illisible est conservé en texte (colonne mixte).
    #[default]
    Auto,
    /// Texte : chaque jeton non vide est gardé tel quel, même s'il ressemble
    /// à un nombre (codes, identifiants d'échantillon…).
    Text,
    /// Dates et heures, lues en timestamps POSIX ([`crate::datetime`]) ; un
    /// jeton qui n'est pas une date reste en texte.
    DateTime,
}

/// Critères auxquels un fichier doit **tous** répondre (les non vides) pour
/// qu'un modèle soit proposé d'office : extension, morceau du nom, signature
/// dans le début du contenu (comme `ORIGIN;JASCO`) — le principe des
/// « Import Filters » d'Origin.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct AutoMatch {
    /// Extensions acceptées, sans point, sans casse (`csv`, `txt`…).
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Texte que le nom du fichier doit contenir (sans casse).
    #[serde(default)]
    pub name_contains: String,
    /// Texte que le début du fichier doit contenir (sans casse).
    #[serde(default)]
    pub content_contains: String,
}

impl AutoMatch {
    /// Aucun critère : le modèle n'est jamais choisi automatiquement.
    pub fn is_empty(&self) -> bool {
        self.extensions.is_empty()
            && self.name_contains.trim().is_empty()
            && self.content_contains.trim().is_empty()
    }
}

impl ImportRecipe {
    /// La recette en JSON lisible — le fichier qu'on passe à un collègue.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Relit une recette exportée par [`ImportRecipe::to_json`]. `Err` avec un
    /// message lisible si le texte n'est pas une recette.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|err| format!("recette illisible : {err}"))
    }
}

/// Encodage forcé d'un fichier, quand la détection automatique se trompe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Encoding {
    /// UTF-16 avec marque, sinon UTF-8, sinon Windows-1252.
    #[default]
    Auto,
    /// UTF-8 (une marque d'ordre est retirée ; un octet invalide devient
    /// `U+FFFD` au lieu de faire échouer l'import).
    Utf8,
    /// Windows-1252, la page de code des appareils sous Windows.
    Windows1252,
    /// ISO 8859-1.
    Latin1,
    /// UTF-16 petit-boutiste, avec ou sans marque.
    Utf16Le,
    /// UTF-16 gros-boutiste, avec ou sans marque.
    Utf16Be,
}

/// Rôle d'une colonne importée, tel que choisi dans l'assistant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ColumnRoleChoice {
    /// La première colonne est l'abscisse (X), les autres sont des Y.
    #[default]
    Auto,
    /// Abscisse.
    X,
    /// Ordonnée.
    Y,
    /// Barre d'erreur en X.
    XError,
    /// Barre d'erreur en Y.
    YError,
    /// Colonne non importée.
    Ignore,
}

fn default_true() -> bool {
    true
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Separator {
    Comma,
    Semicolon,
    Tab,
    Whitespace,
    /// Un caractère au choix (`|`, `:`…).
    Custom(char),
    /// Colonnes à largeur fixe : la largeur, en caractères, de chacune sauf
    /// la dernière, qui prend le reste de la ligne.
    FixedWidth(Vec<usize>),
    /// Détecté automatiquement à partir de la première ligne de données.
    Auto,
}

/// Convention de séparateur décimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DecimalStyle {
    Dot,
    Comma,
    /// Détecté d'après les premières lignes de données : virgule si elle
    /// s'y trouve entre deux chiffres avec un autre séparateur de colonnes
    /// (`;`, tabulation), point sinon.
    Auto,
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
    /// Nature choisie pour la colonne `index` (`Auto` par défaut).
    pub fn column_kind(&self, index: usize) -> ColumnKind {
        self.column_kinds.get(index).copied().unwrap_or_default()
    }

    /// Rôle effectif de la colonne `index` : le choix explicite, sinon X pour
    /// la première colonne et Y pour les suivantes.
    pub fn column_role(&self, index: usize) -> ColumnRoleChoice {
        match self.column_roles.get(index).copied().unwrap_or_default() {
            ColumnRoleChoice::Auto if index == 0 => ColumnRoleChoice::X,
            ColumnRoleChoice::Auto => ColumnRoleChoice::Y,
            explicit => explicit,
        }
    }

    /// Ramène les deux réglages d'en-tête concurrents (`header` et
    /// `header_layout`) à un seul : `HeaderSkip::Lines(n)` devient `n` lignes
    /// d'en-tête principal. `UntilMarker` reste tel quel. Sans effet si le
    /// découpage structuré est déjà utilisé.
    pub fn unify_header(&mut self) {
        if self.header_layout.main_header_lines == 0 && self.header_layout.subheader_lines == 0 {
            if let HeaderSkip::Lines(n) = self.header {
                self.header_layout.main_header_lines = n;
                self.header = HeaderSkip::None;
            }
        }
    }

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
            keep_header_metadata: true,
            keep_footer_metadata: false,
            excluded_metadata: Vec::new(),
            column_roles: Vec::new(),
            thousands: None,
            missing_tokens: Vec::new(),
            comment_prefix: String::new(),
            skip_data_rows: 0,
            encoding: Encoding::Auto,
            auto_match: AutoMatch::default(),
            column_kinds: Vec::new(),
            import_mode: ImportMode::NewSheet,
            files_layout: FilesLayout::SheetPerFile,
            block_split: BlockSplit::None,
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
        assert!(recipe.keep_header_metadata);
        assert!(!recipe.keep_footer_metadata);
        assert!(recipe.excluded_metadata.is_empty());
    }

    #[test]
    fn unify_header_turns_lines_into_a_main_header() {
        let mut recipe = ImportRecipe::new("t");
        recipe.header = HeaderSkip::Lines(3);
        recipe.unify_header();
        assert_eq!(recipe.header, HeaderSkip::None);
        assert_eq!(recipe.header_layout.main_header_lines, 3);
        // Le drapeau `auto_detect` seul (recettes anciennes) ne compte pas.
        let mut old = ImportRecipe::new("t");
        old.header = HeaderSkip::Lines(2);
        old.header_layout.auto_detect = true;
        old.unify_header();
        assert_eq!(old.header_layout.main_header_lines, 2);
        assert_eq!(old.header, HeaderSkip::None);
        // Déjà structuré, ou marqueur : rien ne bouge.
        recipe.header = HeaderSkip::UntilMarker("X".into());
        recipe.unify_header();
        assert_eq!(recipe.header, HeaderSkip::UntilMarker("X".into()));
    }

    #[test]
    fn column_roles_default_to_x_then_y() {
        let mut recipe = ImportRecipe::new("t");
        assert_eq!(recipe.column_role(0), ColumnRoleChoice::X);
        assert_eq!(recipe.column_role(5), ColumnRoleChoice::Y);
        recipe.column_roles = vec![ColumnRoleChoice::Ignore];
        assert_eq!(recipe.column_role(0), ColumnRoleChoice::Ignore);
        assert_eq!(recipe.column_role(1), ColumnRoleChoice::Y);
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
            keep_header_metadata: false,
            keep_footer_metadata: true,
            excluded_metadata: vec!["DATE".into()],
            column_roles: vec![ColumnRoleChoice::X, ColumnRoleChoice::Ignore],
            thousands: Some(' '),
            missing_tokens: vec!["--".into(), "n/a".into()],
            comment_prefix: "#".into(),
            skip_data_rows: 2,
            encoding: Encoding::Windows1252,
            auto_match: AutoMatch {
                extensions: vec!["csv".into()],
                name_contains: "jasco".into(),
                content_contains: "ORIGIN;JASCO".into(),
            },
            column_kinds: vec![ColumnKind::DateTime, ColumnKind::Text],
            import_mode: ImportMode::AddRows,
            files_layout: FilesLayout::SharedX,
            block_split: BlockSplit::BeforeLine("TITLE".into()),
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
        assert!(recipe.keep_header_metadata, "tout est conservé par défaut");
    }
}
