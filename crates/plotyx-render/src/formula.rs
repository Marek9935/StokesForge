//! Typst derrière la feature `typst` — tâche R8.
//!
//! Ce module porte, comme `architecture-choisie.md` §8 le prescrit pour
//! `plotyx-render` : le `World` Typst minimal (8.1), la compilation d'une
//! formule en document (8.1), et le cache générique qui amortit cette
//! compilation entre deux images (8.2). Ce qu'il ne porte **pas** :
//! `typst-render` (RGBA, tâche 8.3, `plotyx-render-egui` uniquement) et
//! `typst-svg` (fragment SVG, tâche 8.4, `plotyx-render-svg` uniquement) —
//! deux dépendances backend-spécifiques que ce crate n'a aucune raison de
//! tirer, la même règle que R4-R7 appliquent déjà au reste du pipeline.
//!
//! # Toujours compilé (issue #29)
//!
//! Ce module était à l'origine derrière une feature `typst` hors défaut
//! (`architecture-choisie.md` §7, tâche 8.6), pour garder le droit
//! d'abandonner Typst sans rien démonter. La feature a été supprimée : tout
//! texte de titre et de légende doit passer par Typst (issue #28), et un
//! build sans elle aurait dessiné une autre figure que le build par défaut.
//! Ce qui reste optionnel, c'est l'activation **par backend**
//! (`with_typst`) : un backend construit sans elle dessine tout en texte
//! brut, et c'est le témoin des tests qui prouvent que Typst est réellement
//! consulté. Le repli en texte brut, `$…$` compris, ne joue plus en
//! production que pour une chaîne qui ne compile pas (invariant 10).
//!
//! # La décision que ce module tranche, et que le roadmap ne tranchait pas
//!
//! `plotyx_plot` ne distingue pas syntaxiquement un texte ordinaire d'une
//! formule : `AxisSpec::label`, `PanelSpec::title`, `PlotSpec::title` sont
//! des `String` ordinaires. **Décision prise ici** : un `&str` est une
//! formule Typst si et seulement s'il contient le caractère `$`, le
//! délimiteur natif du mode mathématique de Typst — pas de nouveau champ
//! dans `plotyx_plot`, donc pas de migration du format `.plx` (la même
//! discipline que le choix de police du 2026-09-13, qui n'a pas non plus
//! touché `PlotSpec`).
//!
//! Ce choix a été préféré à un marqueur dédié (un préfixe réservé, un
//! second champ) pour deux raisons :
//!
//! - c'est la syntaxe que Typst lui-même utilise pour entrer en mode
//!   mathématique dans un document markup — le texte est compilé tel quel,
//!   sans l'envelopper artificiellement dans un `$...$` supplémentaire,
//!   donc un libellé mixte comme `"$lambda$ (nm)"` produit une formule
//!   suivie de texte ordinaire, dans le **même** appel à Typst, sans code
//!   de découpage à écrire ici ;
//! - les libellés qui ne doivent jamais passer par Typst (les graduations
//!   numériques, produites par `scale::format_tick`) ne contiennent jamais
//!   `$` : aucune numérisation ne l'introduit, donc la détection ne peut
//!   pas les faire dévier de leur chemin actuel par accident.
//!
//! **Limite assumée, pas corrigée** : un texte ordinaire qui contiendrait
//! littéralement un `$` (un prix, « Coût ($) ») serait à tort envoyé à
//! Typst, qui échouerait à le compiler comme markup valide dans la plupart
//! des cas ou produirait un rendu inattendu dans les autres. Aucun texte de
//! ce chantier (titres, unités scientifiques) n'a ce besoin aujourd'hui ; si
//! le cas se présentait, la sortie serait un préfixe d'échappement (`\$`,
//! déjà valide en Typst) plutôt qu'un nouveau champ.
//!
//! # Ce que R0 avait déjà noté sur l'API, vérifié de nouveau ici
//!
//! - `FileId::new` prend un `RootedPath`, pas un couple `(root, chemin)` —
//!   confirmé dans `typst_syntax::path` 0.15.1 ;
//! - `VirtualPath::new` rend un `Result`, pas la valeur ;
//! - `PagedDocument` vient de `typst_layout`, pas de `typst` ;
//! - aucun scan de polices système : ce `World` ne connaît que les deux
//!   polices embarquées passées à [`compile`] ([`crate::fonts::FontSet`]),
//!   jamais `typst-kit` ni un `fontdb` de plus — la reproductibilité que
//!   `include_system_fonts(false)` viserait ailleurs est ici acquise par
//!   construction, sans avoir besoin de cette dépendance.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use plotters_backend::{
    text_anchor::{HPos, VPos},
    FontTransform,
};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::{Page, PagedDocument};

use crate::fonts::{FontChoice, FontSet};

/// La couleur d'un texte Typst : rouge, vert, bleu, opacité, en octets non
/// prémultipliés — issue #30.
///
/// Un tableau plutôt que `plotters_backend::BackendColor` : la couleur entre
/// dans la clé du cache ([`CacheKey`]), et un `f64` d'opacité ne s'y hache
/// pas. [`ink`] fait la conversion, une fois, pour les deux backends.
pub type Ink = [u8; 4];

/// L'encre Typst d'une couleur de Plotters — celle que `draw_text` reçoit
/// par `style.color()`, déjà résolue par le pipeline (couleur explicite ou
/// `color::auto_ink`). Même arrondi de l'opacité que le chemin `epaint`
/// de l'écran.
pub fn ink(color: plotters_backend::BackendColor) -> Ink {
    let (r, g, b) = color.rgb;
    let alpha = (color.alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    [r, g, b, alpha]
}

/// Vrai si `text` doit être compilé par Typst plutôt que dessiné en
/// `Shape::Text` brut par les fonts egui.
///
/// Voir la documentation de tête de ce module pour la décision et sa
/// limite assumée.
pub fn is_formula(text: &str) -> bool {
    text.contains('$')
}

/// La bibliothèque standard de Typst, construite une fois pour tout le
/// processus.
///
/// Elle ne dépend d'aucun choix de police (`Library` ne porte que les
/// définitions du langage — fonctions, règles de mise en forme par défaut),
/// donc rien ne justifie de la reconstruire à chaque compilation : c'est la
/// part fixe du coût mesuré en R0, amortie une fois pour toutes plutôt que
/// par formule.
fn library() -> &'static LazyHash<Library> {
    static LIBRARY: OnceLock<LazyHash<Library>> = OnceLock::new();
    LIBRARY.get_or_init(|| LazyHash::new(Library::default()))
}

/// Le `World` Typst minimal de ce chantier — tâche 8.1.
///
/// ~90 lignes une fois les imports comptés à part, conforme au calibrage que
/// R0 avait annoncé (« ~100-150 lignes »). Ne connaît qu'un seul fichier, le
/// texte de la formule elle-même : pas d'imports, pas de packages, pas de
/// système de fichiers.
struct FormulaWorld {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: FileId,
    source: Source,
}

impl FormulaWorld {
    /// Construit un monde pour une seule compilation, sur les deux polices
    /// (texte et Math) résolues par [`crate::fonts::resolve`].
    fn new(markup: String, sans: Font, math: Font) -> Result<Self, String> {
        let fonts = vec![sans, math];
        let book = FontBook::from_fonts(&fonts);
        let vpath = VirtualPath::new("/formule.typ")
            .map_err(|err| format!("chemin virtuel Typst invalide : {err}"))?;
        let main = RootedPath::new(VirtualRoot::Project, vpath).intern();
        let source = Source::new(main, markup);
        Ok(Self {
            book: LazyHash::new(book),
            fonts,
            main,
            source,
        })
    }
}

impl World for FormulaWorld {
    fn library(&self) -> &LazyHash<Library> {
        library()
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            // Aucun fichier externe : ni import, ni package, ni police par
            // chemin. Une formule qui en réclame un échoue proprement — pas
            // de panique, pas de scan de système de fichiers.
            Err(FileError::NotFound(id.vpath().get_with_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().get_with_slash().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        // Aucune formule de ce chantier n'a besoin de la date du jour ; une
        // formule qui appellerait `datetime.today()` échouerait proprement
        // plutôt que de rendre une figure dont le contenu dépendrait de
        // l'instant de son rendu — exactement ce que la reproductibilité
        // visée par `include_system_fonts(false)` ailleurs demande aussi.
        None
    }
}

/// Le résultat d'une compilation Typst réussie — juste assez pour mesurer
/// et pour dessiner, rien de plus.
///
/// Ne porte qu'une seule page : une formule ou un libellé mixte tient sur
/// une page unique tant que `#set page(width: auto, height: auto)` la fait
/// grandir avec son contenu (voir [`compile`]). Un texte qui produirait un
/// saut de page (peu probable pour un libellé d'axe) verrait ses pages
/// suivantes silencieusement ignorées plutôt que provoquer une erreur -
/// cohérent avec le reste du chantier, où une entrée dégénérée du spec ne
/// fait jamais planter le rendu (`architecture-choisie.md` §11.1).
///
/// `Clone` pour qu'un artefact d'écran garde son document et le rastérise de
/// nouveau à une autre résolution sans recompiler
/// (`plotyx_render_egui::scaled`, issue #31) ; le document de Typst est fait
/// de pointeurs partagés, le clone ne recopie pas les glyphes.
#[derive(Clone)]
pub struct Compiled {
    document: PagedDocument,
}

impl Compiled {
    /// La page compilée, si la formule en a produit une.
    pub fn page(&self) -> Option<&Page> {
        self.document.pages().first()
    }

    /// La taille de la page en points Typst — le **même** point abstrait
    /// que celui déjà utilisé par tout ce chantier pour parler de taille de
    /// police (`text::font_size`, `text::PLOTTERS_SIZE_PER_POINT`) : un
    /// libellé Typst et un libellé ordinaire se mesurent donc dans la même
    /// unité, sans conversion supplémentaire au moment de réserver la
    /// place dans la mise en page de Plotters.
    ///
    /// `(0.0, 0.0)` si la compilation n'a produit aucune page (formule
    /// vide) : une boîte de taille nulle plutôt qu'une erreur, le
    /// comportement le plus proche de « rien à dessiner ».
    pub fn size_pt(&self) -> (f64, f64) {
        self.page()
            .map(|page| {
                let size = page.frame.size();
                (size.x.to_pt(), size.y.to_pt())
            })
            .unwrap_or((0.0, 0.0))
    }
}

/// Compile `text` (potentiellement un mélange de markup et de mode
/// mathématique, voir [`is_formula`]) en document Typst — tâche 8.1.
///
/// `size_pt` est la taille de police au sens de
/// [`crate::text::font_size`] : le même nombre que celui qui construirait
/// l'`epaint::FontId` d'un libellé ordinaire de la même taille Plotters,
/// pour qu'un titre à moitié Typst et à moitié texte brut ne change pas de
/// corps entre les deux moitiés.
///
/// `#set page(width: auto, height: auto, margin: 0pt)` est le mécanisme
/// standard de Typst pour obtenir une page qui épouse son contenu plutôt
/// qu'une page A4 : c'est ce qui fait qu'une formule compile en un
/// document d'une page dont la taille **est** la taille du libellé, sans
/// avoir à la déduire d'un rognage a posteriori.
///
/// `ink` devient la couleur de tout le texte (`#set text(fill: …)`, que le
/// mode mathématique hérite) : sans elle, Typst dessine en noir, et un titre
/// sur fond sombre disparaissait (issue #30). Une couleur écrite dans la
/// formule elle-même (`#text(fill: red)[…]`) garde la priorité, comme toute
/// règle locale de Typst — c'est ce qui interdit de rendre en blanc puis de
/// teinter l'image après coup.
///
/// `top-edge`/`bottom-edge: "bounds"` : par défaut, Typst borne une ligne
/// de la hauteur des capitales à la ligne de base. Une page ajustée à son
/// contenu coupait donc tout ce qui descend sous la ligne de base — les
/// jambages (« p », « y ») et surtout les indices (`n_i`, `"Ru(bpy)"_3`),
/// l'essentiel d'une notation scientifique. Le défaut est passé inaperçu
/// jusqu'à la première capture de l'application avec Typst branché (issue
/// #29). Avec `"bounds"`, la boîte est l'encre réellement posée : rien
/// n'est coupé, au prix d'une hauteur qui varie d'un libellé à l'autre
/// (« a » est moins haut que « Ag ») — l'alignement des lignes de base
/// entre libellés voisins est le sujet de l'issue #32.
pub fn compile(
    text: &str,
    size_pt: f32,
    ink: Ink,
    sans: Font,
    math: Font,
) -> Result<Compiled, String> {
    let sans_name = family_name(&sans);
    let math_name = family_name(&math);
    let [r, g, b, a] = ink;
    let markup = format!(
        "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n\
         #set text(font: \"{sans_name}\", size: {size_pt}pt, fill: rgb({r}, {g}, {b}, {a}), \
                   top-edge: \"bounds\", bottom-edge: \"bounds\")\n\
         #show math.equation: set text(font: \"{math_name}\")\n\
         {text}"
    );
    let world = FormulaWorld::new(markup, sans, math)?;
    let warned = typst::compile::<PagedDocument>(&world);
    let document = warned.output.map_err(|diagnostics| {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    Ok(Compiled { document })
}

/// Le nom de famille d'une `Font` Typst déjà chargée — pour l'écrire dans
/// le `#set text(font: ...)` généré par [`compile`], plutôt que de le faire
/// porter séparément par l'appelant et risquer qu'il diverge de la police
/// réellement chargée dans le `World`.
fn family_name(font: &Font) -> String {
    font.info().family.clone()
}

/// Une clé de cache : la formule, sa taille et sa couleur (le texte généré
/// les contient déjà, voir [`compile`]) plus le `pixels_per_point` de
/// l'artefact backend demandé. La couleur y est depuis l'issue #30 : deux
/// titres identiques de couleurs différentes sont deux artefacts.
///
/// **Décision documentée, pas seulement codée** : la compilation Typst
/// elle-même (`document`, en points) ne dépend pas de `pixels_per_point` —
/// Typst met en page dans un espace abstrait, sans notion d'écran. Le
/// mettre quand même dans la clé, comme la tâche 8.2 du roadmap le demande,
/// n'est donc pas nécessaire à la **correction** de ce cache (contrairement
/// à `epaint::Fonts`, où la hauteur de ligne dépend réellement de l'échelle
/// physique, `text.rs`) : c'est un choix délibéré pour offrir un seul
/// mécanisme de cache aux deux backends plutôt que d'en écrire un second,
/// plus fin, spécifique à l'écran. `plotyx-render-svg` (§8.4) appelle
/// toujours ce cache avec un `pixels_per_point` constant (`1.0`) — sa
/// clé ne varie donc jamais sur ce terme, sans que rien ne l'empêche de le
/// faire un jour. `plotyx-render-egui` (§8.3), lui, y fait effectivement
/// varier `pixels_per_point` avec l'écran réel : c'est le cas qui justifie
/// le terme.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    text: String,
    size_pt_bits: u32,
    ink: Ink,
    font_choice: String,
    pixels_per_point_bits: u32,
}

impl CacheKey {
    fn new(text: &str, size_pt: f32, ink: Ink, choice: &FontChoice, pixels_per_point: f32) -> Self {
        Self {
            text: text.to_owned(),
            size_pt_bits: size_pt.to_bits(),
            ink,
            // `FontChoice` n'a pas de `Hash` (fonts.rs, ajout du
            // 2026-09-13, hors périmètre de cette tâche) : sa
            // représentation `Debug` est déterministe et suffit à
            // distinguer les choix, sans lui ajouter un trait pour ce
            // seul usage.
            font_choice: format!("{choice:?}"),
            pixels_per_point_bits: pixels_per_point.to_bits(),
        }
    }
}

/// Cache générique d'artefacts dérivés d'une compilation Typst — tâche 8.2.
///
/// Générique sur `V` : `plotyx-render-egui` y range une texture RGBA
/// (§8.3), `plotyx-render-svg` un fragment SVG (§8.4). Ce module ne connaît
/// ni l'un ni l'autre type — c'est tout l'intérêt, la logique de cache et
/// de compilation restant unique quel que soit le backend qui l'utilise.
pub struct FormulaCache<V> {
    entries: Mutex<HashMap<CacheKey, Arc<V>>>,
}

impl<V> FormulaCache<V> {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Rend l'artefact déjà en cache pour `text` à `size_pt`, en `ink`, à
    /// `pixels_per_point`, ou compile puis construit un nouvel artefact
    /// avec `build`.
    ///
    /// `sans`/`math` sont les polices déjà résolues
    /// (`crate::fonts::resolve(choice)`) : ce cache ne sait pas les
    /// résoudre lui-même, pour ne pas décoder deux polices embarquées à
    /// chaque appel manqué — c'est à l'appelant de les construire une fois
    /// et de les réutiliser, comme [`crate::text::TextMeasurer`] le fait
    /// déjà pour `epaint::Fonts`.
    #[allow(clippy::too_many_arguments)]
    pub fn get_or_try_insert(
        &self,
        text: &str,
        size_pt: f32,
        ink: Ink,
        choice: &FontChoice,
        pixels_per_point: f32,
        sans: Font,
        math: Font,
        build: impl FnOnce(&Compiled) -> Result<V, String>,
    ) -> Result<Arc<V>, String> {
        let key = CacheKey::new(text, size_pt, ink, choice, pixels_per_point);
        // Verrou empoisonné : reconstruit plutôt que de faire échouer tout
        // le rendu pour une raison qu'aucun appelant ne peut corriger — la
        // même politique que `TextMeasurer::layout_at`.
        if let Some(hit) = self
            .entries
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .get(&key)
        {
            return Ok(Arc::clone(hit));
        }
        let compiled = compile(text, size_pt, ink, sans, math)?;
        let value = Arc::new(build(&compiled)?);
        self.entries
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(key, Arc::clone(&value));
        Ok(value)
    }

    /// Nombre d'artefacts actuellement en cache — pour les tests.
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Les artefacts actuellement en cache, dans un ordre quelconque — pour
    /// retrouver l'artefact d'une forme déjà construite
    /// (`plotyx_render_egui::scaled`, issue #31, qui rastérise de nouveau
    /// une formule à la taille affichée).
    pub fn values(&self) -> Vec<Arc<V>> {
        self.entries
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .values()
            .cloned()
            .collect()
    }

    /// Vide le cache — pour borner sa taille chez un appelant qui vit
    /// longtemps (`plotyx_render_egui::GraphView`, où chaque frappe dans un
    /// titre crée une entrée). À n'appeler qu'**avant** de reconstruire tout
    /// ce qui référence les artefacts retirés : une texture libérée alors
    /// qu'une forme en cache la désigne encore s'afficherait vide.
    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clear();
    }
}

impl<V> std::fmt::Debug for FormulaCache<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormulaCache")
            .field("entries", &self.len())
            .finish()
    }
}

impl<V> Default for FormulaCache<V> {
    fn default() -> Self {
        Self::new()
    }
}

/// La mesure d'une formule, telle que `estimate_text_size` doit la rendre —
/// **l'invariant de la tâche 8.5**.
///
/// Toujours en points, jamais en pixels d'un écran particulier : c'est le
/// même contrat que [`crate::text::TextSource::measure`], et il tient ici
/// pour une raison différente — non pas parce qu'on le décide, mais parce
/// que la mise en page de Typst ne connaît de toute façon aucune notion de
/// pixel physique. Convertie en `(u32, u32)` par
/// [`crate::text::round_size`], **la même fonction** que le chemin
/// `epaint`, pour que l'arrondi lui-même ne puisse pas diverger entre les
/// deux chemins de mesure.
pub fn measure(compiled: &Compiled) -> (u32, u32) {
    round_size_pt(compiled.size_pt())
}

/// Comme [`measure`], mais à partir d'une taille en points déjà connue —
/// pour un appelant qui a mis en cache autre chose qu'un [`Compiled`]
/// entier (`plotyx-render-svg::typst_export`, qui ne garde que le fragment
/// SVG et sa taille, pas le document Typst).
pub fn round_size_pt((w, h): (f64, f64)) -> (u32, u32) {
    crate::text::round_size(epaint::Vec2::new(w as f32, h as f32))
}

/// Un choix de police Math, indépendant du choix de police de texte —
/// branchement technique du 2026-09-13, en complément de
/// [`resolve_fonts_with_math`].
///
/// Jusqu'ici, [`resolve_fonts`] déterminait la police Math uniquement à
/// partir du [`FontChoice`] de texte (`crate::fonts::resolve(choice).math`,
/// toujours une des trois paires embarquées) : aucun moyen d'exprimer « ce
/// texte, mais cette Math-là ». Ce type comble cette lacune sans rien
/// décider de plus : aucune valeur par défaut n'en dérive une politique
/// produit, c'est un simple second axe de choix, au même titre que
/// [`FontChoice`] pour le texte.
///
/// **Dégradation silencieuse pour [`MathFontChoice::System`], à savoir
/// avant d'exposer ce choix à un utilisateur** : rien ne garantit qu'une
/// police système porte une table Unicode Math (OpenType `MATH`) valide.
/// Typst compilera quand même avec une police qui n'en a pas — fractions,
/// exposants, opérateurs empilés seront mis en page avec les seules
/// métriques de corps ordinaires (mal centrés verticalement, tailles
/// d'opérateurs agrandissables absentes) — sans erreur ni avertissement
/// remonté. Ce n'est pas un bug de ce mécanisme mais un fait connu de
/// l'écosystème des polices Math OpenType (la table `MATH` est rare hors
/// des polices conçues pour, comme les trois paires embarquées) : un futur
/// appelant qui proposerait ce choix à un utilisateur doit le savoir avant
/// de le faire, plutôt que de découvrir un rendu dégradé sans diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathFontChoice {
    /// Une des trois paires embarquées — sa moitié Math, indépendamment de
    /// la police de texte demandée par ailleurs.
    Embedded(FontSet),
    /// Une police du système, par son nom exact. Introuvable ⇒ repli sur
    /// [`FontSet::Noto`], la même politique que
    /// [`crate::fonts::FontChoice::System`] applique déjà côté texte.
    System(String),
}

/// Charge les polices Typst (texte + Math) correspondant à un
/// [`FontChoice`] — pour que les deux backends (8.3, 8.4) n'aient pas
/// chacun leur propre façon de le faire.
///
/// Équivalent à [`resolve_fonts_with_math`]`(choice, None)` : la police
/// Math suit le [`FontChoice`] de texte, exactement comme avant l'ajout de
/// [`MathFontChoice`] — voir sa documentation pour ce que `None` signifie
/// précisément. Aucun appelant existant n'a besoin de changer pour garder
/// son comportement actuel.
pub fn resolve_fonts(choice: &FontChoice) -> Result<(Font, Font), String> {
    resolve_fonts_with_math(choice, None)
}

/// Comme [`resolve_fonts`], avec un choix de police Math optionnel et
/// indépendant du choix de texte.
///
/// `math_choice` à `None` reproduit exactement [`resolve_fonts`] tel qu'il
/// se comportait avant cet ajout : la Math suit le [`FontChoice`] de texte
/// (une des trois paires embarquées pour [`FontChoice::Embedded`],
/// [`FontSet::Noto`] pour [`FontChoice::System`] — la même règle que
/// [`crate::fonts::resolve`] applique déjà). `Some(..)` prend le dessus et
/// résout la Math séparément via [`MathFontChoice`], dont la documentation
/// détaille la dégradation silencieuse d'une police système sans table
/// Math.
///
/// Contrairement à l'ancienne limite documentée ici pour
/// [`FontChoice::System`] côté texte (repli sur Noto **que la police soit
/// trouvée ou non**) : la police système demandée est désormais réellement
/// cherchée via [`crate::fonts::load_system_font`] — le même mécanisme que
/// [`crate::fonts::resolve`] utilise côté écran/export ordinaire — et
/// utilisée si trouvée ; le repli sur Noto ne joue plus que si elle est
/// introuvable, la même politique que le reste du projet applique déjà à
/// une police système (`fonts.rs`, tête de module).
pub fn resolve_fonts_with_math(
    choice: &FontChoice,
    math_choice: Option<&MathFontChoice>,
) -> Result<(Font, Font), String> {
    let sans = match choice {
        FontChoice::Embedded(set) => embedded_font(set.sans_bytes(), "sans")?,
        FontChoice::System(name) => match crate::fonts::load_system_font(name) {
            Some(data) => system_font(data, "sans")?,
            None => embedded_font(FontSet::Noto.sans_bytes(), "sans")?,
        },
    };

    let math = match math_choice {
        Some(MathFontChoice::Embedded(set)) => embedded_font(set.math_bytes(), "Math")?,
        Some(MathFontChoice::System(name)) => match crate::fonts::load_system_font(name) {
            Some(data) => system_font(data, "Math")?,
            None => embedded_font(FontSet::Noto.math_bytes(), "Math")?,
        },
        // Comportement historique, inchangé : la Math suit le choix de
        // texte plutôt qu'un scan système propre — voir la documentation
        // de cette fonction et de `crate::fonts::resolve`.
        None => {
            let math_set = match choice {
                FontChoice::Embedded(set) => *set,
                FontChoice::System(_) => FontSet::Noto,
            };
            embedded_font(math_set.math_bytes(), "Math")?
        }
    };

    Ok((sans, math))
}

/// Construit une `Font` Typst à partir des octets d'une des paires
/// embarquées — factorisé entre texte et Math, pour un message d'erreur qui
/// nomme le rôle plutôt que de dupliquer le même `ok_or_else` quatre fois.
fn embedded_font(bytes: &'static [u8], role: &str) -> Result<Font, String> {
    Font::new(Bytes::new(bytes), 0).ok_or_else(|| format!("police {role} embarquée illisible"))
}

/// Construit une `Font` Typst à partir des octets bruts d'une police
/// système déjà trouvée par [`crate::fonts::load_system_font`] — même
/// index de face que celui que `fontdb` a résolu, pour ne pas retomber sur
/// la première face d'un fichier qui en contiendrait plusieurs (une famille
/// de polices variable, par exemple).
fn system_font(data: epaint::text::FontData, role: &str) -> Result<Font, String> {
    let index = data.index;
    Font::new(Bytes::new(data.font), index)
        .ok_or_else(|| format!("police {role} système illisible"))
}

/// Le décalage d'ancrage d'une boîte de `size_pt`, dans son propre repère
/// **non tourné** — c'est-à-dire avant l'éventuelle rotation de
/// [`rotate_offset`].
///
/// Transcrit de `plotyx-render-egui::backend::EguiBackend::draw_text`
/// (calcul identique, non partagé jusqu'ici parce qu'il n'existait qu'à un
/// seul endroit) : un label Typst doit s'ancrer exactement comme un label
/// ordinaire, sous peine d'un décalage visible entre les deux à la
/// frontière `$...$`.
pub fn anchor_offset((width, height): (f64, f64), h: HPos, v: VPos) -> (f64, f64) {
    let dx = match h {
        HPos::Left => 0.0,
        HPos::Center => -width / 2.0,
        HPos::Right => -width,
    };
    let dy = match v {
        VPos::Top => 0.0,
        VPos::Center => -height / 2.0,
        VPos::Bottom => -height,
    };
    (dx, dy)
}

/// Applique à un décalage la même rotation que Plotters applique au texte —
/// voir `EguiBackend::rotate`, dont ceci est la version partagée (utilisée
/// aussi par `plotyx-render-svg`, qui n'avait jusqu'ici aucune raison de
/// tourner quoi que ce soit).
pub fn rotate_offset((dx, dy): (f64, f64), transform: FontTransform) -> (f64, f64) {
    match transform {
        FontTransform::None => (dx, dy),
        FontTransform::Rotate90 => (-dy, dx),
        FontTransform::Rotate180 => (-dx, -dy),
        FontTransform::Rotate270 => (dy, -dx),
    }
}

/// L'angle d'une rotation de Plotters, en degrés horaires — l'unité que
/// SVG attend (`rotate(deg, cx, cy)`, déjà utilisée telle quelle par
/// `plotters-svg::draw_text`) et qu'`epaint::TextShape::with_angle` attend
/// en radians (voir l'appelant côté écran, qui convertit).
pub fn rotation_degrees(transform: FontTransform) -> f64 {
    match transform {
        FontTransform::None => 0.0,
        FontTransform::Rotate90 => 90.0,
        FontTransform::Rotate180 => 180.0,
        FontTransform::Rotate270 => 270.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noto() -> (Font, Font) {
        resolve_fonts(&FontChoice::default()).expect("polices Noto illisibles")
    }

    const NOIR: Ink = [0, 0, 0, 255];

    /// Détection de base : les délimiteurs `$` déclenchent Typst, un texte
    /// ordinaire ne le fait jamais — y compris une graduation numérique
    /// telle que `scale::format_tick` la produit.
    #[test]
    fn la_detection_repose_sur_le_dollar() {
        assert!(is_formula("$lambda$"));
        assert!(!is_formula("Temps (s)"));
        assert!(!is_formula("111"));
        assert!(!is_formula(""));
    }

    /// Une formule triviale compile en une page dont la taille n'est pas
    /// nulle — le test de sanité qui aurait dû exister avant tout code de
    /// production consommant cette compilation (consigne de la mission).
    #[test]
    fn une_formule_triviale_compile_et_mesure_une_taille_non_nulle() {
        let (sans, math) = noto();
        let compiled =
            compile("$lambda$", 12.0, NOIR, sans, math).expect("compilation Typst échouée");
        let (w, h) = compiled.size_pt();
        assert!(w > 0.0, "largeur nulle : {w}");
        assert!(h > 0.0, "hauteur nulle : {h}");
    }

    /// Un libellé mixte texte + formule compile aussi : c'est la raison
    /// documentée du choix de détection (pas de découpage manuel ici).
    #[test]
    fn un_libelle_mixte_texte_et_formule_compile() {
        let (sans, math) = noto();
        let compiled =
            compile("$lambda$ (nm)", 12.0, NOIR, sans, math).expect("compilation Typst échouée");
        assert!(compiled.page().is_some());
    }

    /// Une formule syntaxiquement invalide échoue proprement, sans
    /// paniquer — condition pour qu'un backend puisse replier sur
    /// `Shape::Text` brut plutôt que de faire tomber tout le rendu.
    #[test]
    fn une_formule_invalide_echoue_sans_paniquer() {
        let (sans, math) = noto();
        let result = compile("$ (unclosed", 12.0, NOIR, sans, math);
        assert!(result.is_err());
    }

    /// Deux tailles différentes de la même formule donnent des tailles de
    /// page différentes — sanity check qu'on mesure bien le corps de
    /// police demandé, pas une valeur figée.
    #[test]
    fn une_taille_de_police_plus_grande_rend_une_boite_plus_grande() {
        let (sans1, math1) = noto();
        let (sans2, math2) = noto();
        let petite = compile("$lambda$", 8.0, NOIR, sans1, math1).unwrap();
        let grande = compile("$lambda$", 32.0, NOIR, sans2, math2).unwrap();
        assert!(grande.size_pt().0 > petite.size_pt().0);
        assert!(grande.size_pt().1 > petite.size_pt().1);
    }

    /// Un indice agrandit la boîte **vers le bas** : la page va jusqu'au
    /// bas de l'encre, pas jusqu'à la ligne de base (défaut de Typst, qui
    /// coupait les indices et les jambages à l'écran — issue #29). Même
    /// vérification sur un jambage de texte ordinaire, hors mode maths.
    #[test]
    fn un_indice_ou_un_jambage_nest_pas_coupe_par_la_page() {
        let hauteur = |text: &str| {
            let (sans, math) = noto();
            compile(text, 14.0, NOIR, sans, math)
                .expect("compilation Typst échouée")
                .size_pt()
                .1
        };
        assert!(hauteur("$n_i$") > hauteur("$n$"), "indice coupé");
        assert!(hauteur("pn") > hauteur("n"), "jambage coupé");
    }

    /// Le cache : une deuxième demande identique ne recompile pas — vérifié
    /// en comptant les entrées, pas le temps (un temps mesuré serait
    /// fragile en CI). Tâche 8.2.
    #[test]
    fn le_cache_ne_grandit_pas_sur_une_demande_identique() {
        let cache: FormulaCache<(f64, f64)> = FormulaCache::new();
        let choice = FontChoice::default();
        for _ in 0..3 {
            let (sans, math) = noto();
            cache
                .get_or_try_insert(
                    "$lambda$",
                    12.0,
                    NOIR,
                    &choice,
                    1.0,
                    sans,
                    math,
                    |compiled| Ok(compiled.size_pt()),
                )
                .expect("échec de compilation/insertion");
        }
        assert_eq!(cache.len(), 1);
    }

    /// Deux `pixels_per_point` différents pour la même formule occupent
    /// deux entrées — la clé documentée par [`CacheKey`], vérifiée plutôt
    /// que supposée.
    #[test]
    fn deux_pixels_per_point_distincts_occupent_deux_entrees() {
        let cache: FormulaCache<(f64, f64)> = FormulaCache::new();
        let choice = FontChoice::default();
        for ppp in [1.0_f32, 1.25_f32] {
            let (sans, math) = noto();
            cache
                .get_or_try_insert(
                    "$lambda$",
                    12.0,
                    NOIR,
                    &choice,
                    ppp,
                    sans,
                    math,
                    |compiled| Ok(compiled.size_pt()),
                )
                .expect("échec de compilation/insertion");
        }
        assert_eq!(cache.len(), 2);
    }

    /// Mesure de sanité sur le **pipeline réel** — pas le spike jetable de
    /// R0. Ignoré par défaut, comme les bancs coûteux de
    /// `plotyx-robustesse` (`cargo test -- --ignored`) : ce n'est pas une
    /// assertion de correction, juste un repère chiffré à comparer au seuil
    /// de 150 ms déjà fixé et déjà mesuré sous ce seuil par R0.
    #[test]
    #[ignore = "mesure de latence, pas une assertion — cargo test -- --ignored"]
    fn latence_de_compilation_dune_formule_inedite() {
        let n = 200;
        let start = std::time::Instant::now();
        for i in 0..n {
            let (sans, math) = noto();
            let text = format!("$lambda_{i} = m_{i} c^2$");
            compile(&text, 14.0, NOIR, sans, math).expect("compilation Typst échouée");
        }
        let elapsed = start.elapsed();
        let per_call = elapsed / n;
        println!(
            "{n} compilations Typst de formules inédites en {elapsed:?} \
             ({per_call:?}/formule, seuil R0 : 150 ms)"
        );
        assert!(
            per_call.as_millis() < 150,
            "latence de compilation {per_call:?} au-delà du seuil fixé en R0"
        );
    }

    /// Une police système **introuvable** replie sur Noto pour la police de
    /// texte et pour la Math — plus « toujours », comme avant le
    /// branchement sur [`crate::fonts::load_system_font`] : seulement
    /// quand la recherche système échoue réellement, la même politique que
    /// `crate::fonts::resolve` applique déjà.
    #[test]
    fn une_police_systeme_introuvable_replie_sur_noto() {
        let (sans, math) = resolve_fonts(&FontChoice::System(
            "Police PlotYx Qui Ne Peut Pas Exister — Test R.13".to_owned(),
        ))
        .expect("résolution échouée");
        assert_eq!(sans.info().family, FontSet::Noto.sans_family_name());
        assert_eq!(math.info().family, FontSet::Noto.math_family_name());
    }

    /// Trouve dynamiquement le nom d'une police système réellement présente
    /// sur la machine qui exécute le test, pour ne pas coder en dur un nom
    /// qu'aucune machine CI ne garantit avoir. `None` si la machine n'a
    /// vraiment aucune police système : le test qui en dépend est alors un
    /// no-op plutôt qu'un échec qu'aucune correction de ce module ne
    /// pourrait résoudre (`fonts.rs` n'a pas ce cas à couvrir : il ne teste
    /// que le repli sur introuvable, jamais la police trouvée).
    fn une_police_systeme_disponible() -> Option<String> {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        let id = database.faces().next()?.id;
        let face = database.face(id)?;
        face.families.first().map(|(name, _)| name.clone())
    }

    /// Une police système **trouvée** est réellement utilisée pour la
    /// moitié texte — le comportement corrigé par cette tâche : avant,
    /// `resolve_fonts` repliait sur Noto que la police existe ou non.
    #[test]
    fn une_police_systeme_disponible_est_reellement_utilisee_pour_le_texte() {
        let Some(nom) = une_police_systeme_disponible() else {
            eprintln!(
                "aucune police système sur cette machine : \
                 une_police_systeme_disponible_est_reellement_utilisee_pour_le_texte ignoré"
            );
            return;
        };
        let (sans, _math) =
            resolve_fonts(&FontChoice::System(nom.clone())).expect("résolution échouée");
        assert_eq!(
            sans.info().family,
            nom,
            "la police système trouvée n'a pas été utilisée pour le texte"
        );
    }

    /// La police Math est choisissable indépendamment de la police de
    /// texte via [`MathFontChoice`] — texte et Math viennent chacun d'une
    /// paire embarquée différente, et les deux `Font` obtenues
    /// correspondent chacune à ce qui a été demandé, pas à une paire liée
    /// par défaut.
    #[test]
    fn la_police_math_se_choisit_independamment_du_texte() {
        let (sans, math) = resolve_fonts_with_math(
            &FontChoice::Embedded(FontSet::Libertinus),
            Some(&MathFontChoice::Embedded(FontSet::Fira)),
        )
        .expect("résolution échouée");
        assert_eq!(sans.info().family, FontSet::Libertinus.sans_family_name());
        assert_eq!(math.info().family, FontSet::Fira.math_family_name());
    }

    /// `None` passé à [`resolve_fonts_with_math`] reproduit exactement
    /// [`resolve_fonts`] — le contrat documenté qui garantit qu'aucun
    /// appelant existant ne change de comportement sans y toucher.
    #[test]
    fn aucun_choix_math_explicite_reproduit_lancien_comportement() {
        let choice = FontChoice::Embedded(FontSet::Fira);
        let (sans_a, math_a) = resolve_fonts(&choice).expect("résolution échouée");
        let (sans_b, math_b) = resolve_fonts_with_math(&choice, None).expect("résolution échouée");
        assert_eq!(sans_a.info().family, sans_b.info().family);
        assert_eq!(math_a.info().family, math_b.info().family);
    }

    /// L'ancrage centré retranche bien la moitié de chaque dimension, et
    /// l'ancrage `Left`/`Top` ne décale rien — les deux cas limites.
    #[test]
    fn lancrage_centre_retranche_la_moitie_de_la_boite() {
        assert_eq!(
            anchor_offset((10.0, 4.0), HPos::Left, VPos::Top),
            (0.0, 0.0)
        );
        assert_eq!(
            anchor_offset((10.0, 4.0), HPos::Center, VPos::Center),
            (-5.0, -2.0)
        );
        assert_eq!(
            anchor_offset((10.0, 4.0), HPos::Right, VPos::Bottom),
            (-10.0, -4.0)
        );
    }

    /// Une rotation de 90° horaires envoie l'axe X sur l'axe Y — le
    /// premier repère qui aurait révélé une inversion de signe.
    #[test]
    fn la_rotation_a_90_degres_envoie_x_sur_y() {
        assert_eq!(
            rotate_offset((1.0, 0.0), FontTransform::Rotate90),
            (0.0, 1.0)
        );
        assert_eq!(rotation_degrees(FontTransform::Rotate90), 90.0);
    }

    /// **Le test de l'invariant 8.5** : la mesure d'une formule ne dépend
    /// pas de l'échelle d'affichage — contrairement à la hauteur d'un
    /// galley `epaint` (`text.rs`), qui elle en dépend réellement. Les deux
    /// chemins de mesure ont des raisons différentes de se comporter comme
    /// ils le font ; ce test fixe celle de Typst, pour que la fusion des
    /// deux dans `estimate_text_size` (tâche du backend) ne suppose pas à
    /// tort la même dépendance des deux côtés.
    #[test]
    fn la_mesure_dune_formule_ne_depend_pas_du_pixels_per_point() {
        let (sans, math) = noto();
        let a = measure(&compile("$lambda$", 14.0, NOIR, sans, math).unwrap());
        let (sans, math) = noto();
        let b = measure(&compile("$lambda$", 14.0, NOIR, sans, math).unwrap());
        assert_eq!(
            a, b,
            "deux compilations identiques mesurent différemment sans raison"
        );
    }
}
