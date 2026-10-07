//! Sélecteurs de fichiers natifs hors du fil de l'interface (issue #192).
//!
//! `rfd` attend la réponse du portail (`xdg-desktop-portal`) dans une boucle
//! sans délai d'expiration : appelé sur le fil d'egui, un sélecteur fermé sans
//! réponse (clic sur l'application, sous GNOME/Wayland) gelait toute
//! l'application. Le sélecteur s'ouvre donc sur un fil à part et rend son choix
//! par un canal ; l'interface sonde ce canal à chaque image
//! ([`FilePicker::poll`]) et reste vivante quoi qu'il arrive — au pire une
//! demande reste en attente, jamais une application gelée.
//!
//! Les sept sélecteurs de l'application passent par [`PickRequest`], poussé
//! comme [`crate::state::Action::PickFile`] ; `StokesForgeApp` le démarre et
//! applique la réponse. Un seul sélecteur est *suivi* à la fois : une nouvelle
//! demande **remplace** la précédente. Sans cela, un sélecteur fermé sans que le
//! portail ne réponde resterait « en attente » à jamais et empêcherait d'en
//! rouvrir un (suite de #192) ; son fil, bloqué dans `rfd`, est abandonné et sa
//! réponse éventuelle est jetée.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use stokesforge_plot::PlotSpec;

use crate::tr;

/// Ce que l'on demande à l'utilisateur de choisir, avec de quoi appliquer la
/// réponse.
#[derive(Debug, Clone, PartialEq)]
pub enum PickRequest {
    /// File › Open : un projet à charger.
    OpenProject,
    /// File › Save As : la destination du projet (nom suggéré).
    SaveProjectAs { suggested: String },
    /// Export du graphe : spec déjà cadré par la vue, nom suggéré.
    ExportGraph {
        spec: Box<PlotSpec>,
        file_name: String,
    },
    /// Assistant d'import : enregistrer la recette dans un fichier JSON.
    ExportImportRecipe { file_name: String },
    /// Assistant d'import : lire une recette JSON.
    ImportImportRecipe,
    /// Assistant d'import : fichiers à importer (dossier de départ facultatif).
    AddImportFiles { directory: Option<PathBuf> },
}

impl PickRequest {
    fn dialog(&self) -> rfd::FileDialog {
        let dialog = rfd::FileDialog::new();
        match self {
            Self::OpenProject => dialog
                .set_title(tr!("project-open-title"))
                .add_filter(tr!("project-file-filter"), &["stf", "stokesforge"]),
            Self::SaveProjectAs { suggested } => dialog
                .set_title(tr!("project-save-title"))
                .add_filter(tr!("project-file-filter"), &["stf", "stokesforge"])
                .set_file_name(suggested),
            Self::ExportGraph { file_name, .. } => dialog
                .set_title(tr!("export-dialog-title"))
                .add_filter("SVG", &["svg"])
                .add_filter("PDF", &["pdf"])
                .add_filter("PNG", &["png"])
                .set_file_name(file_name),
            Self::ExportImportRecipe { file_name } => dialog
                .set_title(tr!("import-files-export-title"))
                .add_filter(tr!("import-files-template-filter"), &["json"])
                .set_file_name(file_name),
            Self::ImportImportRecipe => dialog
                .set_title(tr!("import-files-import-title"))
                .add_filter(tr!("import-files-template-filter"), &["json"]),
            Self::AddImportFiles { directory } => {
                let dialog = dialog.set_title(tr!("import-files-add-title"));
                match directory {
                    Some(dir) => dialog.set_directory(dir),
                    None => dialog,
                }
            }
        }
    }

    /// Lance le sélecteur (bloquant) ; à appeler sur le fil dédié.
    fn run(&self, dialog: rfd::FileDialog) -> Option<Vec<PathBuf>> {
        match self {
            Self::SaveProjectAs { .. }
            | Self::ExportGraph { .. }
            | Self::ExportImportRecipe { .. } => dialog.save_file().map(|path| vec![path]),
            Self::OpenProject | Self::ImportImportRecipe => {
                dialog.pick_file().map(|path| vec![path])
            }
            Self::AddImportFiles { .. } => dialog.pick_files(),
        }
    }
}

/// La réponse d'un sélecteur : `None` si annulé ou fermé sans choix.
pub type Picked = Option<Vec<PathBuf>>;

struct Pending {
    request: PickRequest,
    receiver: Receiver<Picked>,
}

/// Le sélecteur en cours, s'il y en a un.
#[derive(Default)]
pub struct FilePicker {
    pending: Option<Pending>,
}

impl FilePicker {
    /// Un sélecteur est-il ouvert ?
    pub fn is_open(&self) -> bool {
        self.pending.is_some()
    }

    /// Ouvre le sélecteur sur un fil à part, en abandonnant celui qui était
    /// suivi (peut-être mort côté portail). `wake` réveille l'interface quand
    /// la réponse est prête.
    pub fn start(&mut self, request: PickRequest, wake: impl Fn() + Send + 'static) {
        let dialog = request.dialog();
        let (sender, receiver) = channel();
        let worker = request.clone();
        std::thread::spawn(move || {
            let _ = sender.send(worker.run(dialog));
            wake();
        });
        self.pending = Some(Pending { request, receiver });
    }

    /// La réponse du sélecteur terminé, une seule fois. Ne bloque jamais ; un
    /// fil mort sans réponse vaut « annulé ».
    pub fn poll(&mut self) -> Option<(PickRequest, Picked)> {
        let pending = self.pending.as_ref()?;
        let picked = match pending.receiver.try_recv() {
            Ok(picked) => picked,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => None,
        };
        let pending = self.pending.take()?;
        Some((pending.request, picked))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Garde (#192) : un `rfd::FileDialog` appelé hors de ce module serait
    /// synchrone sur le fil de l'interface — le gel du portail.
    #[test]
    fn aucun_selecteur_rfd_hors_du_module() {
        let fautifs: Vec<String> = crate::sources::sources()
            .iter()
            .filter(|fichier| fichier.chemin != "file_picker.rs")
            .flat_map(|fichier| {
                fichier
                    .code()
                    .lines()
                    .enumerate()
                    .filter(|(_, ligne)| ligne.contains("rfd::"))
                    .map(|(index, _)| format!("src/{}:{}", fichier.chemin, index + 1))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(
            fautifs.is_empty(),
            "sélecteur de fichiers hors de file_picker.rs — passer par \
             Action::PickFile(PickRequest) : {fautifs:?}"
        );
    }

    /// Suite de #192 : un sélecteur fermé sans réponse du portail ne doit pas
    /// empêcher d'en rouvrir un.
    #[test]
    fn une_nouvelle_demande_remplace_un_selecteur_reste_sans_reponse() {
        let (_sender, receiver) = channel::<Picked>();
        let mut picker = FilePicker {
            pending: Some(Pending {
                request: PickRequest::OpenProject,
                receiver,
            }),
        };
        picker.start(PickRequest::ImportImportRecipe, || {});
        let pending = picker.pending.as_ref().expect("suivi");
        assert_eq!(pending.request, PickRequest::ImportImportRecipe);
    }

    #[test]
    fn sans_selecteur_ouvert_le_sondage_ne_bloque_pas() {
        let mut picker = FilePicker::default();
        assert!(!picker.is_open());
        assert!(picker.poll().is_none());
    }

    #[test]
    fn un_fil_mort_sans_reponse_vaut_annule() {
        let (sender, receiver) = channel::<Picked>();
        drop(sender);
        let mut picker = FilePicker {
            pending: Some(Pending {
                request: PickRequest::OpenProject,
                receiver,
            }),
        };
        let (request, picked) = picker.poll().expect("réponse");
        assert_eq!(request, PickRequest::OpenProject);
        assert_eq!(picked, None);
        assert!(!picker.is_open());
    }

    #[test]
    fn une_reponse_attendue_laisse_l_appelant_libre() {
        // Le sélecteur n'a pas répondu : le sondage rend la main aussitôt.
        let (_sender, receiver) = channel::<Picked>();
        let mut picker = FilePicker {
            pending: Some(Pending {
                request: PickRequest::ImportImportRecipe,
                receiver,
            }),
        };
        assert!(picker.poll().is_none());
        assert!(picker.is_open());
    }
}
