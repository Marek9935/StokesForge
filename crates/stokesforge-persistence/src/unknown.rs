//! Mécanique partagée de dégradation gracieuse d'une variante d'énumération
//! inconnue (C5, niveau 2, `guide-de-correction-robustesse.md` §F3).
//!
//! # Le principe : capturer par valeur d'abord, choisir ensuite
//!
//! `serde` dérive normalement un décodage d'enum externally-tagged qui **ne
//! sait pas se rattraper** : dès que le nom de variante lu ne correspond à
//! aucune des variantes connues, l'échec remonte immédiatement — c'était le
//! bug (C5, niveau 1 l'avait déjà nommé sans le corriger : l'erreur devenait
//! `UnknownVariant` plutôt que `Corrupt`, mais le fichier restait perdu en
//! entier).
//!
//! Le correctif : décoder d'abord la valeur MessagePack qui suit le nom de
//! variante en un arbre générique (`rmpv::Value`, qui sait représenter
//! n'importe quelle valeur du format sans connaître le schéma), la
//! réencoder, puis *essayer* de la redécoder comme l'une des variantes
//! connues — via `rmp_serde`, pas `rmpv::ext::from_value` : ce dernier
//! représente les enums Rust par indice de variante (`[indice, contenu]`),
//! une convention différente de celle qu'écrit ce format
//! (« externally tagged » par nom, `schema.rs` §tête) ; seul `rmp_serde` —
//! le même décodeur que pour le reste du fichier — sait relire ce que ce
//! format a réellement écrit. Si cet essai échoue — variante inconnue, ou
//! un type de tracé inconnu enfoui trois niveaux plus bas dans le spec
//! d'un graphe — le nom et le contenu bruts sont conservés tels quels dans
//! un [`stokesforge_plot::RawVariant`] plutôt que de faire échouer l'appelant.
//! C'est ce qui borne la perte à **ce seul nœud** (ou cette seule série) au
//! lieu du fichier entier.
//!
//! # Le compromis assumé
//!
//! Passer par `rmpv::Value` puis réencoder avant de retenter `rmp_serde`
//! copie les octets d'une colonne `Float` (un `Vec<u8>` déjà présent dans le
//! fichier) une ou deux fois de plus qu'un décodage direct : un `load()`
//! alloue et copie les mesures d'une colonne plusieurs fois plutôt qu'une
//! seule. Rien dans ce dépôt ne mesure aujourd'hui le coût de `load()`/
//! `save()` (B17, `b4_allocateur.rs` comptent d'autres opérations) ; ce
//! n'est donc pas un correctif qui casse un garde-fou existant, seulement un
//! coût à surveiller si le format finit par porter des colonnes bien plus
//! grandes.
//!
//! # Comment le nœud redevient les mêmes octets à la sauvegarde suivante
//!
//! [`RawVariant::payload`] porte le contenu associé, déjà encodé en
//! MessagePack — vide pour une variante sans contenu (un futur variant
//! unitaire, sérialisé comme une simple chaîne). `serialize_raw_variant`
//! réinjecte ces octets tels quels (redécodés en `rmpv::Value` puis
//! sérialisés par ce même `Value`, qui produit le même encodage canonique
//! que celui qui les a écrits — c'est la même bibliothèque des deux côtés) :
//! rien n'est renommé, réordonné ou normalisé, ce qui est la condition de
//! B20 (deux sauvegardes successives byte-identiques).

use serde::de::{DeserializeOwned, Error as _};
use serde::ser::{Error as _, SerializeMap};
use serde::{Deserialize, Deserializer, Serializer};
use stokesforge_plot::RawVariant;

/// Résultat de la lecture du tag externally-tagged d'un enum : soit une
/// simple chaîne (variante unitaire, sans contenu), soit une map à une
/// entrée `{nom: contenu}`.
enum Tag {
    Bare(String),
    Tagged(String, rmpv::Value),
}

fn split_tag(value: &rmpv::Value) -> Result<Tag, String> {
    match value {
        rmpv::Value::String(name) => {
            let name = name
                .as_str()
                .ok_or_else(|| "variant name is not UTF-8".to_string())?;
            Ok(Tag::Bare(name.to_string()))
        }
        rmpv::Value::Map(entries) if entries.len() == 1 => {
            let (key, content) = &entries[0];
            let name = key
                .as_str()
                .ok_or_else(|| "variant key is not text".to_string())?;
            Ok(Tag::Tagged(name.to_string(), content.clone()))
        }
        other => Err(format!(
            "unexpected externally-tagged variant shape: {other:?}"
        )),
    }
}

/// Décode un enum externally-tagged en essayant d'abord `Known` (le schéma
/// d'aujourd'hui) ; à l'échec, capture le nom et le contenu bruts dans un
/// [`RawVariant`] plutôt que de propager l'erreur.
pub(crate) fn deserialize_with_fallback<'de, D, Known, Public>(
    deserializer: D,
) -> Result<Public, D::Error>
where
    D: Deserializer<'de>,
    Known: DeserializeOwned + Into<Public>,
    Public: From<RawVariant>,
{
    let value = rmpv::Value::deserialize(deserializer)?;
    // Réencodage canonique de la valeur capturée, puis nouvelle tentative
    // par le vrai décodeur `rmp_serde` (voir la doc de tête du module) :
    // c'est lui, pas `rmpv`, qui sait interpréter le tag externally-tagged
    // par nom que ce format écrit partout ailleurs.
    if let Ok(reencoded) = rmp_serde::to_vec_named(&value) {
        if let Ok(known) = rmp_serde::from_slice::<Known>(&reencoded) {
            return Ok(known.into());
        }
    }
    let (name, payload) = match split_tag(&value).map_err(D::Error::custom)? {
        Tag::Bare(name) => (name, Vec::new()),
        Tag::Tagged(name, content) => {
            let bytes = rmp_serde::to_vec_named(&content).map_err(D::Error::custom)?;
            (name, bytes)
        }
    };
    Ok(Public::from(RawVariant { name, payload }))
}

/// Réécrit une variante capturée à l'identique : `payload` vide → la simple
/// chaîne du nom (variante unitaire) ; sinon → `{nom: contenu}`, le contenu
/// étant redécodé depuis les octets bruts puis sérialisé par le `Serializer`
/// donné (celui de l'appelant, pas nécessairement `rmp_serde` — `rmpv::Value`
/// se sérialise vers n'importe quel format `serde`).
pub(crate) fn serialize_raw_variant<S: Serializer>(
    raw: &RawVariant,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if raw.payload.is_empty() {
        return serializer.serialize_str(&raw.name);
    }
    let content: rmpv::Value = rmp_serde::from_slice(&raw.payload).map_err(S::Error::custom)?;
    let mut map = serializer.serialize_map(Some(1))?;
    map.serialize_entry(&raw.name, &content)?;
    map.end()
}
