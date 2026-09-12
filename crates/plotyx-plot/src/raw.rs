//! Capture d'une variante d'énumération inconnue à la lecture (C5, niveau 2,
//! `guide-de-correction-robustesse.md` §F3).
//!
//! Un fichier `.plx` écrit par une version plus récente de PlotYx peut porter
//! une variante qu'un lecteur plus ancien ne connaît pas (un nouveau type de
//! tracé, une nouvelle nature de nœud). Plutôt que de faire échouer toute la
//! désérialisation — ce que `serde` fait par défaut pour un enum externally
//! tagged — `plotyx-persistence` capture le nom de la variante et son
//! contenu encodé, et les porte jusqu'ici pour que le reste du modèle (et
//! l'affichage) puisse dégrader gracieusement au lieu de refuser le fichier
//! entier.
//!
//! Ce type ne connaît ni `serde` ni le format MessagePack : `plotyx-plot` et
//! `plotyx-core` n'ont pas le droit d'en dépendre
//! (`tests/no_ui_dependency.rs`). Il ne porte que des octets opaques, déjà
//! encodés tels que lus du fichier ; seul `plotyx-persistence` sait les
//! réinterpréter, à la lecture comme à la ré-écriture — c'est ce qui garantit
//! qu'une variante inconnue survit à deux sauvegardes successives à
//! l'identique (B20).

/// Nom et contenu bruts d'une variante d'énumération que ce lecteur ne
/// connaît pas.
#[derive(Debug, Clone, PartialEq)]
pub struct RawVariant {
    /// Nom de la variante tel qu'écrit dans le fichier.
    pub name: String,
    /// Contenu associé, encodé en MessagePack tel que lu — vide pour une
    /// variante sans contenu (un futur variant unitaire).
    pub payload: Vec<u8>,
}
