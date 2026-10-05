//! Offrir **une même figure sous plusieurs formes** au presse-papiers système
//! (issue #149) : SVG de préférence, puis PNG, puis PDF.
//!
//! # Pourquoi un crate à part
//!
//! egui n'écrit que du texte (`Context::copy_text`) et `arboard`, que l'arbre
//! contient déjà, ne sait écrire que du texte, du HTML ou une image bitmap :
//! aucun des deux n'offre `image/svg+xml` ni `application/pdf`, ni plusieurs
//! types à la fois. Une offre multi-format est du code **propre à chaque
//! plateforme**, avec du FFI : il vit ici, sans dépendance au reste du
//! workspace, plutôt que dans la coquille d'interface. Ce crate ne sait rien
//! de StokesForge — il reçoit des octets étiquetés par un type MIME, dans
//! l'ordre de préférence de l'appelant.
//!
//! # Une offre, pas un choix
//!
//! [`offer`] publie toutes les [`Representation`]s ; c'est **l'application
//! destinataire** qui choisit parmi elles, et certaines ignorent l'ordre
//! proposé (un traitement de texte prend souvent le PNG avant le SVG). L'ordre
//! donné est celui de l'intention — SVG, PNG, PDF — sans promesse sur le choix
//! du destinataire.
//!
//! # Les plateformes
//!
//! | Affichage | Mécanisme | Vérifié |
//! |---|---|---|
//! | Wayland | `Clipboard::store_formats` du patch `vendor/smithay-clipboard` : le worker partagé avec egui-winit offre un `wl_data_source` par type MIME | à la main (voir `docs/application.md`) |
//! | X11 | un propriétaire de la sélection `CLIPBOARD` ([`x11`]), `TARGETS` puis un type par représentation | test réel, `--ignored` |
//! | Windows | un format enregistré par représentation (`PNG`, `image/svg+xml`, `application/pdf`) | compile seulement |
//! | macOS | `NSPasteboard`, un type UTI par représentation | compile seulement |
//!
//! # Limites assumées
//!
//! - Wayland : le compositeur n'accepte la sélection que de la fenêtre qui
//!   a le focus clavier et vient de recevoir une entrée (le « serial ») ;
//!   sinon l'offre est **perdue sans erreur** — c'est le comportement du
//!   texte d'egui-winit aussi, et c'est pourquoi l'appel part du geste Ctrl+C.
//! - X11 : pas de protocole `INCR` ; une représentation plus grosse qu'une
//!   requête X (256 Kio sans l'extension BIG-REQUESTS, de l'ordre de 16 Mio
//!   avec) n'est pas offerte. Elle est ignorée, les autres restent offertes.
//! - Le contenu disparaît avec le processus (pas de gestionnaire de
//!   presse-papiers pris en charge côté X11).

use std::sync::Arc;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

#[cfg(all(unix, not(target_os = "macos")))]
mod wayland;
#[cfg(all(unix, not(target_os = "macos")))]
pub mod x11;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// Types MIME des représentations d'une figure, dans l'ordre de préférence
/// de StokesForge.
pub mod mime {
    /// Image vectorielle.
    pub const SVG: &str = "image/svg+xml";
    /// Image matricielle.
    pub const PNG: &str = "image/png";
    /// Document PDF.
    pub const PDF: &str = "application/pdf";
}

/// Une forme du contenu offert : ses octets sous un type MIME.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Representation {
    /// Type MIME (voir [`mime`]).
    pub mime: &'static str,
    /// Le contenu ; partagé, il n'est jamais recopié par l'offre.
    pub bytes: Arc<[u8]>,
}

impl Representation {
    /// Une représentation de `bytes` sous `mime`.
    pub fn new(mime: &'static str, bytes: impl Into<Arc<[u8]>>) -> Self {
        Self {
            mime,
            bytes: bytes.into(),
        }
    }
}

/// Ce qui peut empêcher d'offrir le contenu. Le texte est en anglais brut,
/// comme celui des autres crates du dessous : l'interface le met en phrase.
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    /// Aucune représentation n'a été donnée.
    #[error("no representation to offer")]
    Empty,
    /// Ce type d'affichage ne sait pas recevoir l'offre.
    #[error("the clipboard offer is not supported on this display ({0})")]
    Unsupported(&'static str),
    /// Le système a refusé ou raté l'offre.
    #[error("could not offer the clipboard content: {0}")]
    Backend(String),
}

/// Offre `representations` comme contenu courant du presse-papiers système,
/// en remplacement du précédent.
///
/// `display` est l'affichage de la fenêtre qui a reçu le geste de copie
/// (`eframe::Frame` en est un) : il choisit le mécanisme, et sa durée de vie
/// garantit qu'un pointeur Wayland reste valide pendant l'appel.
///
/// `Ok` veut dire « offre émise », pas « offre acceptée » : sous Wayland le
/// compositeur peut l'écarter sans rien répondre (voir *Limites assumées*).
///
/// # Errors
///
/// [`ClipboardError::Empty`] sans représentation ; [`ClipboardError::Unsupported`]
/// pour un affichage sans backend ; [`ClipboardError::Backend`] si le système
/// refuse.
pub fn offer(
    display: &impl HasDisplayHandle,
    representations: &[Representation],
) -> Result<(), ClipboardError> {
    if representations.is_empty() {
        return Err(ClipboardError::Empty);
    }
    let handle = display
        .display_handle()
        .map_err(|err| ClipboardError::Backend(err.to_string()))?;
    match handle.as_raw() {
        #[cfg(all(unix, not(target_os = "macos")))]
        RawDisplayHandle::Wayland(wayland) => wayland::offer(wayland.display, representations),
        #[cfg(all(unix, not(target_os = "macos")))]
        RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_) => x11::offer(representations),
        #[cfg(windows)]
        RawDisplayHandle::Windows(_) => windows::offer(representations),
        #[cfg(target_os = "macos")]
        RawDisplayHandle::AppKit(_) => macos::offer(representations),
        other => Err(ClipboardError::Unsupported(display_name(&other))),
    }
}

/// Le nom d'un affichage, pour [`ClipboardError::Unsupported`].
fn display_name(display: &RawDisplayHandle) -> &'static str {
    match display {
        RawDisplayHandle::Wayland(_) => "Wayland",
        RawDisplayHandle::Xlib(_) => "Xlib",
        RawDisplayHandle::Xcb(_) => "XCB",
        RawDisplayHandle::Windows(_) => "Windows",
        RawDisplayHandle::AppKit(_) => "AppKit",
        _ => "other",
    }
}

/// Le nom du format du presse-papiers Windows pour un type MIME : le format
/// enregistré par les applications est `PNG` (Office, navigateurs, GIMP) ; les
/// autres gardent leur type MIME comme nom, la convention d'Inkscape pour le SVG.
#[cfg(any(windows, test))]
fn windows_format_name(mime: &str) -> &str {
    if mime == mime::PNG {
        "PNG"
    } else {
        mime
    }
}

/// L'identifiant de type uniforme (UTI) macOS d'un type MIME, `None` sans
/// équivalent connu.
#[cfg(any(target_os = "macos", test))]
fn macos_uti(mime: &str) -> Option<&'static str> {
    match mime {
        mime::PNG => Some("public.png"),
        mime::SVG => Some("public.svg-image"),
        mime::PDF => Some("com.adobe.pdf"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sans;
    impl HasDisplayHandle for Sans {
        fn display_handle(
            &self,
        ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
            Err(raw_window_handle::HandleError::Unavailable)
        }
    }

    #[test]
    fn sans_representation_l_offre_est_refusee_avant_tout_affichage() {
        // Témoin : même avec un affichage valide, rien à offrir est une
        // erreur de l'appelant, pas un succès vide.
        assert!(matches!(offer(&Sans, &[]), Err(ClipboardError::Empty)));
    }

    #[test]
    fn un_affichage_indisponible_est_une_erreur_et_pas_une_panique() {
        let un = [Representation::new(mime::PNG, vec![1, 2, 3])];
        assert!(matches!(offer(&Sans, &un), Err(ClipboardError::Backend(_))));
    }

    #[test]
    fn les_formats_windows_et_les_uti_macos_couvrent_les_trois_types() {
        assert_eq!(windows_format_name(mime::PNG), "PNG");
        assert_eq!(windows_format_name(mime::SVG), "image/svg+xml");
        assert_eq!(windows_format_name(mime::PDF), "application/pdf");
        assert_eq!(macos_uti(mime::PNG), Some("public.png"));
        assert_eq!(macos_uti(mime::SVG), Some("public.svg-image"));
        assert_eq!(macos_uti(mime::PDF), Some("com.adobe.pdf"));
        // Un type inconnu n'est pas inventé.
        assert_eq!(macos_uti("text/plain"), None);
    }

    #[test]
    fn une_representation_partage_ses_octets() {
        let bytes: Arc<[u8]> = Arc::from(vec![9u8; 4]);
        let a = Representation::new(mime::SVG, bytes.clone());
        let b = a.clone();
        assert!(Arc::ptr_eq(&a.bytes, &b.bytes));
        assert_eq!(Arc::strong_count(&bytes), 3);
    }
}
