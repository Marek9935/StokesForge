//! Capture de `sys.stdout` et `sys.stderr` pendant l'exécution d'un script.
//!
//! # Pourquoi un aiguillage par fil plutôt qu'un `redirect_stdout`
//!
//! `sys.stdout` est **global à l'interpréteur**. La recette habituelle —
//! sauver, remplacer, restaurer — n'est correcte que si un seul acteur s'en
//! sert à la fois : deux moteurs de script vivant dans le même processus (ce
//! qui arrive dans le binaire de tests, et arrivera le jour où l'application
//! ouvrira deux projets) se voleraient mutuellement leur restauration.
//!
//! On pose donc **une seule fois** un objet-relais dans `sys.stdout` et
//! `sys.stderr`, et c'est lui qui aiguille : vers le tampon du **fil courant**
//! s'il y en a un ([`begin`] l'installe, [`end`] le retire), sinon vers la
//! sortie réelle du processus (`sys.__stdout__`). L'objet est donc partagé et
//! sans état ; le seul état vit dans un `thread_local!`.
//!
//! Conséquence voulue : un `print` exécuté hors script (un futur moteur de
//! rendu, par exemple) continue d'aller sur la sortie du processus.

use std::cell::RefCell;

use pyo3::prelude::*;

/// Laquelle des deux sorties standard ce relais représente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stream {
    /// `sys.stdout`.
    Out,
    /// `sys.stderr`.
    Err,
}

impl Stream {
    /// Nom de l'attribut de `sys` que ce relais remplace.
    fn attribute(self) -> &'static str {
        match self {
            Stream::Out => "stdout",
            Stream::Err => "stderr",
        }
    }

    /// Nom de l'attribut de `sys` qui garde la sortie **réelle** du processus.
    fn original_attribute(self) -> &'static str {
        match self {
            Stream::Out => "__stdout__",
            Stream::Err => "__stderr__",
        }
    }
}

/// Tampons du fil courant, présents seulement pendant l'exécution d'un script.
#[derive(Debug, Default)]
struct Buffers {
    /// Ce qui a été écrit sur la sortie standard.
    out: String,
    /// Ce qui a été écrit sur la sortie d'erreur.
    err: String,
}

thread_local! {
    /// Tampons du fil courant. `None` hors exécution de script : les écritures
    /// repartent alors vers la sortie réelle du processus.
    static BUFFERS: RefCell<Option<Buffers>> = const { RefCell::new(None) };
}

/// Ouvre la capture sur le fil courant. Écrase un éventuel tampon résiduel.
pub(crate) fn begin() {
    BUFFERS.with(|cell| *cell.borrow_mut() = Some(Buffers::default()));
}

/// Ferme la capture sur le fil courant et rend `(stdout, stderr)`.
pub(crate) fn end() -> (String, String) {
    BUFFERS.with(|cell| {
        cell.borrow_mut()
            .take()
            .map(|buffers| (buffers.out, buffers.err))
            .unwrap_or_default()
    })
}

/// Ajoute `text` au tampon du fil courant. Rend `false` s'il n'y en a pas.
fn push(stream: Stream, text: &str) -> bool {
    BUFFERS.with(|cell| match cell.borrow_mut().as_mut() {
        Some(buffers) => {
            match stream {
                Stream::Out => buffers.out.push_str(text),
                Stream::Err => buffers.err.push_str(text),
            }
            true
        }
        None => false,
    })
}

/// Relais posé dans `sys.stdout` / `sys.stderr`.
///
/// Sans état propre (d'où `frozen`) : il ne porte que la sortie qu'il
/// représente. Tout l'état de capture vit dans un `thread_local!`, ce qui rend
/// l'objet partageable entre plusieurs moteurs de script sans qu'ils se
/// marchent dessus.
#[pyclass(frozen, module = "pyx", name = "_SortieDeScript")]
pub(crate) struct CaptureStream {
    /// Sortie représentée par ce relais.
    stream: Stream,
}

#[pymethods]
impl CaptureStream {
    /// Écrit `text` dans le tampon du fil courant, ou sur la sortie réelle du
    /// processus si aucun script ne tourne sur ce fil. Rend le nombre de
    /// caractères écrits, comme `io.TextIOBase.write`.
    fn write(&self, py: Python<'_>, text: &str) -> PyResult<usize> {
        if !push(self.stream, text) {
            let original = py
                .import("sys")?
                .getattr(self.stream.original_attribute())?;
            if !original.is_none() {
                original.call_method1("write", (text,))?;
            }
        }
        Ok(text.chars().count())
    }

    /// Vide la sortie réelle si la capture n'est pas active ; sans effet
    /// sinon (un tampon en mémoire n'a rien à vider).
    fn flush(&self, py: Python<'_>) -> PyResult<()> {
        let capturing = BUFFERS.with(|cell| cell.borrow().is_some());
        if capturing {
            return Ok(());
        }
        let original = py
            .import("sys")?
            .getattr(self.stream.original_attribute())?;
        if !original.is_none() {
            original.call_method0("flush")?;
        }
        Ok(())
    }

    /// Toujours vrai : ce relais est une sortie.
    fn writable(&self) -> bool {
        true
    }

    /// Toujours faux : on n'y lit pas.
    fn readable(&self) -> bool {
        false
    }

    /// Toujours faux : ce n'est pas un flux positionnable.
    fn seekable(&self) -> bool {
        false
    }

    /// Toujours faux : la sortie d'un script n'est pas un terminal, ce qui
    /// évite qu'une bibliothèque tente d'y écrire des codes de couleur.
    fn isatty(&self) -> bool {
        false
    }

    /// Encodage annoncé, pour les bibliothèques qui l'interrogent.
    #[getter]
    fn encoding(&self) -> &'static str {
        "utf-8"
    }

    /// Politique d'erreur d'encodage annoncée.
    #[getter]
    fn errors(&self) -> &'static str {
        "strict"
    }

    /// Représentation lisible, pour qu'un script qui inspecte `sys.stdout`
    /// comprenne ce qu'il regarde.
    fn __repr__(&self) -> String {
        format!("<PlotYx script output: {}>", self.stream.attribute())
    }
}

/// Pose le relais dans `sys.stdout` et `sys.stderr` s'il n'y est pas déjà.
///
/// Idempotent, et volontairement **définitif** : le relais n'est jamais
/// retiré. Hors capture il redirige vers la sortie réelle, si bien que le
/// laisser en place ne change le comportement de personne.
pub(crate) fn install(py: Python<'_>) -> PyResult<()> {
    let sys = py.import("sys")?;
    for stream in [Stream::Out, Stream::Err] {
        let current = sys.getattr(stream.attribute())?;
        if current.is_instance_of::<CaptureStream>() {
            continue;
        }
        sys.setattr(
            stream.attribute(),
            Bound::new(py, CaptureStream { stream })?,
        )?;
    }
    Ok(())
}
