//! Aller-retour de tableaux entre Rust et numpy : **lecture sans copie**,
//! **écriture par copie**, et le contrat de sûreté qui rend la paire tenable.
//!
//! # L'asymétrie est une décision
//!
//! | Sens | Mécanisme | Copie ? |
//! |---|---|---|
//! | Rust → Python (lecture) | tableau numpy **empruntant** le `Arc<Vec<f64>>`, `writeable = False` | **non** |
//! | Python → Rust (écriture) | copie du tableau numpy dans un `Vec<f64>` neuf | **oui** |
//!
//! L'écriture copie **par choix** : le modèle doit posséder ses buffers.
//! Adopter de la mémoire allouée par l'allocateur Python ferait dépendre la
//! durée de vie d'une colonne de celle d'un objet Python. Seconde raison,
//! indépendante : un `Vec<f64>` copié est du Rust pur, il se détruit sur
//! n'importe quel fil — alors que détruire un objet Python exige le GIL, ce qui
//! interdirait de faire traverser la valeur au fil qui possède le modèle
//! (tâche 5.4).
//!
//! # Ce module ne connaît pas le modèle
//!
//! Il manipule des `Arc<Vec<f64>>` — un type de la bibliothèque standard — et
//! non des colonnes. `plotyx-python` ne dépend pas de `plotyx-core` : c'est une
//! frontière vérifiée par le compilateur, et c'est elle qui permettra à un
//! moteur de rendu Matplotlib de se poser ici sans passer par la couche de
//! script.
//!
//! # Préalable
//!
//! Comme le reste du crate, ces fonctions supposent l'interpréteur démarré
//! (cf. [`crate::interpreter::EmbeddedPython::initialize`]).

use std::sync::Arc;
use std::time::Instant;

use numpy::ndarray::aview1;
use numpy::{
    dtype, PyArray1, PyArrayDescrMethods, PyArrayMethods, PyUntypedArray, PyUntypedArrayMethods,
};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyRuntimeError, PyTypeError};
use pyo3::prelude::*;

use crate::interpreter::{attached, PythonError};

create_exception!(
    plotyx_python,
    MaskedArrayError,
    PyException,
    "Un `numpy.ma.masked_array` porte au moins une valeur masquée : la copier \
     écrirait cette valeur cachée comme une donnée réelle. Rendez le masque \
     explicite avant l'appel — `.filled(float('nan'))` pour le traduire dans \
     la valeur manquante du modèle, `.compressed()` pour l'écarter — plutôt \
     que de laisser `copy_array_to_vec` deviner."
);

/// Conteneur Python qui maintient vivant le buffer Rust sous une vue numpy.
///
/// C'est l'attribut `base` du tableau rendu par [`borrowed_readonly_array`] :
/// tant qu'il vit, le compteur de références de l'`Arc` reste **≥ 2**, donc
/// toute écriture côté modèle passe par `Arc::make_mut` et **alloue un buffer
/// neuf**. Le buffer sous la vue n'est alors ni déplacé, ni réalloué, ni
/// libéré : la clause de sûreté de `borrow_from_array` est honorée **par
/// construction, pas par vigilance**.
///
/// Le `pyclass` est `frozen` : rien, depuis Python, ne peut remplacer ou muter
/// l'`Arc` qu'il détient.
#[pyclass(frozen, module = "plotyx")]
pub struct FloatBufferOwner {
    /// Clone de l'`Arc` emprunté. Ce n'est **pas** un pointeur ni une référence
    /// empruntée : c'est ce clone, et lui seul, qui tient le compteur au-dessus
    /// de 1.
    data: Arc<Vec<f64>>,
}

impl FloatBufferOwner {
    /// Longueur du buffer maintenu vivant — utile aux tests et au diagnostic.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Vrai si le buffer maintenu vivant est vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Rend une vue numpy **sans copie** sur `data`, en lecture seule.
///
/// Le tableau rendu pointe **exactement** sur `data.as_ptr()` : aucun octet
/// n'est dupliqué, quelle que soit la taille de la colonne. Son attribut `base`
/// est un [`FloatBufferOwner`] qui détient un clone de l'`Arc`.
///
/// # `writeable = False` est un garde-fou, pas une frontière
///
/// Il attrape la faute d'inattention (`col[0] = 3` dans un script) et documente
/// l'intention. Il ne résiste pas à un script déterminé, et n'a pas à le faire :
/// le modèle de confiance donne déjà les pleins pouvoirs à l'interpréteur.
///
/// # Une vue est un instantané, pas un abonnement
///
/// Une vue prise **avant** une écriture reste sur l'**ancien** buffer. Le script
/// continue d'y lire des valeurs périmées, silencieusement :
///
/// ```python
/// v = pyx.column(col)       # v regarde directement la mémoire du modèle
/// print(v[0])               # 1.0
/// pyx.set_column(col, autres_valeurs)
/// print(v[0])               # 1.0  ← toujours l'ancienne valeur
/// print(pyx.column(col)[0]) # 2.0  ← la nouvelle
/// ```
///
/// **C'est le comportement retenu**, et c'est le seul choix sûr : muter le
/// buffer sous les pieds de numpy serait un plantage, pas une valeur
/// surprenante. Les deux options écartées, pour mémoire : *invalider la vue*
/// (numpy ne permet pas d'invalider proprement un tableau ; il faudrait rendre
/// un objet enveloppant, donc perdre le fait que ce soit un vrai `ndarray`, ce
/// que toute la bibliothèque scientifique attend) ; *vue vivante qui se met à
/// jour* (impossible sans copie ni verrou — précisément ce que le zéro-copie
/// interdit).
///
/// **Règle simple qui évite tout le problème : ne gardez pas une vue plus
/// longtemps qu'un calcul ; relisez la colonne après toute écriture.**
///
/// # Erreurs
///
/// [`Err`] si l'allocation du conteneur Python échoue, ou si le drapeau
/// d'écriture n'a pas pu être retiré.
pub fn borrowed_readonly_array<'py>(
    py: Python<'py>,
    data: &Arc<Vec<f64>>,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    // Le conteneur est créé AVANT l'emprunt : s'il ne peut pas être alloué, on
    // sort sans avoir fabriqué de tableau qui pointerait dans le vide.
    let owner = Bound::new(
        py,
        FloatBufferOwner {
            data: Arc::clone(data),
        },
    )?;

    // Vue `ndarray` empruntée sur la tranche — aucun octet copié : `aview1` ne
    // fait que retenir pointeur, longueur et pas.
    let view = aview1(&data[..]);

    // SAFETY : la clause de `PyArray::borrow_from_array` exige deux choses.
    //
    // (1) « `container` est posé comme objet de base du tableau rendu, lequel
    //     ne doit pas être détruit avant `container` ». C'est l'inverse qui est
    //     construit ici : `owner` est *consommé* par l'appel, sa référence est
    //     transférée au tableau numpy, qui la relâchera à sa propre destruction.
    //     Le conteneur survit donc mécaniquement au tableau.
    //
    // (2) « `array` ne doit pas être réalloué tant que `container` vit ».
    //     `owner` détient un CLONE de l'`Arc`, donc le compteur de références
    //     du buffer vaut ≥ 2 tant que la vue existe. Côté modèle, toute écriture
    //     passe par `Arc::make_mut`, qui, avec un compteur ≥ 2, **alloue un
    //     buffer neuf** et laisse l'ancien intact : ni mutation en place, ni
    //     réallocation (croissance du `Vec`), ni libération ne peuvent atteindre
    //     le buffer sous la vue. Le `Vec` lui-même ne peut pas être libéré : le
    //     clone de l'`Arc` le maintient vivant même si l'appelant relâche le
    //     sien.
    //
    // Conséquence assumée et documentée ci-dessus : la vue devient un
    // instantané de l'ancien buffer. C'est une valeur périmée, jamais un accès
    // invalide.
    let array = unsafe { PyArray1::borrow_from_array(&view, owner.into_any()) };

    // `writeable = False`. Le chemin passe par l'emprunt exclusif de
    // rust-numpy, seule API *sûre* qui retire le drapeau ; le garde d'emprunt
    // est relâché aussitôt, il ne survit pas à cette fonction. Le tableau ne
    // peut pas être déjà emprunté : il vient d'être créé et personne d'autre
    // n'en a la référence.
    let readonly = array
        .try_readwrite()
        .map_err(|err| PyRuntimeError::new_err(format!("exclusive borrow refused: {err}")))?
        .make_nonwriteable();
    drop(readonly);

    Ok(array)
}

/// Copie un tableau numpy de `float64` vers un `Vec<f64>` neuf.
///
/// Accepte les tableaux **non contigus** — une tranche `v[::2]` en est un : le
/// chemin de compactage est explicite, il ne suppose jamais la contiguïté.
///
/// La copie est le sens *écriture* de l'asymétrie décrite en tête de module :
/// le `Vec` rendu appartient à Rust, il ne dépend plus d'aucun objet Python et
/// se détruit sur n'importe quel fil, GIL ou pas.
///
/// # Un `numpy.ma.masked_array` est refusé, jamais deviné
///
/// `numpy.ma.MaskedArray` est une sous-classe d'`ndarray` : sans contrôle
/// dédié, elle traverserait les trois vérifications ci-dessous (c'est un
/// tableau, `float64`, une dimension) et ses cases masquées seraient copiées
/// **comme si elles étaient des données réelles** — le masque, qui dit
/// « valeur absente », serait tout simplement ignoré.
///
/// Le modèle a exactement une valeur manquante (`NaN`, documentée en tête de
/// `plotyx-core::column`), mais choisir **ici** de traduire un masque en `NaN`
/// couperait le script de la décision : `.filled()`, `.compressed()`, ou une
/// autre valeur de repli lui appartiennent. Le dépôt a déjà tranché ce genre
/// de cas pour `int64`, refusé plutôt que converti en silence « une
/// conversion implicite cacherait une perte de précision au-delà de 2⁵³ » —
/// le même raisonnement s'applique mot pour mot au masque : une conversion
/// implicite cacherait la perte du masque lui-même. D'où [`MaskedArrayError`]
/// dès qu'au moins une case est masquée ; un `MaskedArray` sans case masquée
/// (masque `nomask`, ou entièrement `False`) ne cache rien et traverse comme
/// un `ndarray` ordinaire.
///
/// # Erreurs
///
/// [`Err`] avec un message explicite si l'objet n'est pas un tableau numpy, ou
/// pas de `dtype` `float64`, ou pas à une dimension, ou [`MaskedArrayError`]
/// si l'objet est un `numpy.ma.masked_array` portant au moins une valeur
/// masquée.
pub fn copy_array_to_vec(array: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    let py = array.py();

    // 0. Masque numpy, avant tout le reste : un `MaskedArray` est un
    //    `ndarray` valide aux yeux des trois contrôles suivants, donc c'est
    //    le seul endroit où le distinguer.
    if has_hidden_masked_values(array)? {
        return Err(MaskedArrayError::new_err(format!(
            "masked numpy array (`{}`) refused: at least one value is \
             masked and would be copied as real data if accepted \
             — call `.filled(float('nan'))` (masked cell -> missing \
             value of the model) or `.compressed()` (masked cell -> dropped) \
             before `pyx.set_column`",
            type_name(array)
        )));
    }

    // 1. Est-ce seulement un tableau numpy ? Une liste Python échoue ici.
    let untyped = array.cast::<PyUntypedArray>().map_err(|_| {
        PyTypeError::new_err(format!(
            "expected a one-dimensional numpy float64 array, got an object of type `{}`",
            type_name(array)
        ))
    })?;

    // 2. Dimension. Un tableau 2-D échoue ici, avec sa forme dans le message.
    if untyped.ndim() != 1 {
        return Err(PyTypeError::new_err(format!(
            "expected a one-dimensional numpy array, got {} dimensions (shape {:?})",
            untyped.ndim(),
            untyped.shape()
        )));
    }

    // 3. `dtype`. Un tableau d'`int64` échoue ici : on refuse explicitement
    //    plutôt que de convertir en silence — une conversion implicite cacherait
    //    une perte de précision au-delà de 2^53.
    let expected = dtype::<f64>(py);
    let actual = untyped.dtype();
    if !actual.is_equiv_to(&expected) {
        return Err(PyTypeError::new_err(format!(
            "expected a numpy float64 array, got an array of `{actual}`"
        )));
    }

    let typed = untyped.cast::<PyArray1<f64>>().map_err(|_| {
        PyTypeError::new_err("one-dimensional numpy float64 array expected".to_owned())
    })?;
    let readonly = typed
        .try_readonly()
        .map_err(|err| PyRuntimeError::new_err(format!("shared borrow refused: {err}")))?;
    let view = readonly.as_array();

    // Compactage explicite : `as_slice` ne rend `Some` que pour un tableau
    // contigu en mémoire. Sinon on parcourt en suivant les pas, ce qui couvre
    // `v[::2]`, `v[::-1]` et toute vue exotique.
    match view.as_slice() {
        Some(contiguous) => Ok(contiguous.to_vec()),
        None => Ok(view.iter().copied().collect()),
    }
}

/// Vrai si `object` est un `numpy.ma.masked_array` avec **au moins une**
/// valeur masquée.
///
/// Délègue à `numpy.ma.is_masked`, plutôt que de relire l'attribut `mask` et
/// d'en décider ici : c'est numpy qui connaît le cas `nomask` (le sentinelle
/// que porte un `MaskedArray` sans case masquée, pour lequel un examen manuel
/// devrait aussi couvrir un masque explicite mais entièrement `False`).
/// `is_masked` rend `False` pour tout objet qui n'est pas un `MaskedArray` —
/// une liste, un entier, un `ndarray` ordinaire — donc cet appel est sûr avant
/// même de savoir si `object` est un tableau numpy.
///
/// `numpy.ma` est déjà chargé dès que `numpy` l'est (son `__init__.py`
/// importe son sous-module `ma`) : `py.import` retrouve donc un module déjà en
/// mémoire, jamais un accès disque.
fn has_hidden_masked_values(object: &Bound<'_, PyAny>) -> PyResult<bool> {
    let py = object.py();
    let ma = py.import("numpy.ma")?;
    ma.call_method1("is_masked", (object,))?.extract()
}

/// Nom du type Python d'un objet, pour les messages d'erreur.
fn type_name(object: &Bound<'_, PyAny>) -> String {
    object
        .get_type()
        .name()
        .map(|name| name.to_string())
        .unwrap_or_else(|_| "?".to_owned())
}

/// Mesures du verrou : `(durée d'un emprunt, durée d'une copie)` pour `len`
/// valeurs, en **nanosecondes**.
///
/// Les deux gestes sont mesurés dans une seule prise du GIL, après un
/// préchauffage **à la même taille** : il absorbe l'initialisation paresseuse
/// de la C-API numpy et du type [`FloatBufferOwner`], mais surtout il fait
/// payer d'avance les défauts de page du premier tableau de 8 Mo. Sans lui, la
/// copie mesurée est celle d'une allocation *froide* (≈ 2,7 ms pour 1 M `f64`)
/// au lieu du régime établi (≈ 0,8 ms) — l'écart est du système, pas de numpy,
/// et il fausserait la comparaison avec la tâche 5.1a.
///
/// L'emprunt est en temps **constant** (il n'alloue que deux objets Python) là
/// où la copie est **linéaire** : l'écart croît avec la taille de la colonne.
///
/// # Erreurs
///
/// [`Err`] si un appel Python échoue.
pub fn measure_borrow_vs_copy(len: usize) -> Result<(u128, u128), PythonError> {
    let data: Arc<Vec<f64>> = Arc::new((0..len).map(|index| index as f64).collect());

    attached(|py| {
        // Préchauffage à la taille réelle : type de `FloatBufferOwner`,
        // `PY_ARRAY_API`, drapeaux d'emprunt, et défauts de page de l'allocation
        // de 8 Mo. Le tableau de chauffe est détruit avant la mesure.
        std::hint::black_box(borrowed_readonly_array(py, &data)?);
        std::hint::black_box(PyArray1::from_slice(py, &data));

        let before_borrow = Instant::now();
        let borrowed = borrowed_readonly_array(py, &data)?;
        let borrow_ns = before_borrow.elapsed().as_nanos();
        std::hint::black_box(&borrowed);

        let before_copy = Instant::now();
        let copied = PyArray1::from_slice(py, &data);
        let copy_ns = before_copy.elapsed().as_nanos();
        std::hint::black_box(&copied);

        Ok((borrow_ns, copy_ns))
    })
}
