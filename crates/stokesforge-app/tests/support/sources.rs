//! Lecture des sources de `stokesforge-app` **et de `stokesforge-app-state`**
//! pour les garde-fous qui lisent le code (#116, chantier #115 ; #263).
//!
//! # Une seule liste de fichiers
//!
//! [`sources`] lit **récursivement** `src/` : un module déplacé dans `src/app/`
//! reste lu sans rien déclarer. Toutes les gardes qui lisent le code passent
//! par elle — `tests/libelles.rs`, `tests/history_text.rs`, `tests/gardes.rs`
//! et, par `src/lib.rs`, les tests d'`icons`, de `theme` et d'`i18n`. Aucune ne
//! nomme plus un fichier : déplacer une fonction ne la fait sortir d'aucune
//! garde.
//!
//! # Une seule vue « production »
//!
//! [`Fichier::production`] blanchit les commentaires et **chaque** item
//! `#[cfg(test)]` — pas seulement ce qui suit le premier : `errors.rs` alterne
//! code et modules de test. Un module de test écrit dans son propre fichier
//! (`#[cfg(test)] mod tests;`, avec ou sans `#[path]`) rend ce fichier entier
//! de test ([`Fichier::de_test`]), et tout module qu'il déclare à son tour.
//! Les blancs gardent les fins de ligne : les numéros de ligne rapportés
//! restent justes.
//!
//! # Limites, toutes bruyantes
//!
//! Non reconnus : `#[cfg(all(test, …))]` / `#[cfg(any(test, …))]`, et un
//! `#[cfg(test)] mod x;` déclaré **dans un module en ligne** (`mod a { … }`),
//! dont le fichier est cherché au mauvais endroit. Dans les deux cas, du code
//! de test est lu comme de la production : faux positif d'une garde, jamais un
//! trou. Écrire `#[cfg(test)]` seul, sur un module déclaré au niveau du fichier.
//!
//! # Inclusion
//!
//! Ce fichier n'est pas un test à lui seul (Cargo ne découvre que
//! `tests/*.rs`) : il est inclus par `#[path]` là où on s'en sert. Ses témoins
//! sont dans `tests/gardes.rs`.

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// La liste des fichiers
// ---------------------------------------------------------------------------

/// Un fichier source de `src/`.
#[derive(Debug, Clone)]
pub struct Fichier {
    /// Chemin relatif à `src/`, séparé par `/` : `app.rs`, `i18n/pseudo.rs`.
    pub chemin: String,
    pub texte: String,
    /// Le fichier entier est du test : il porte un module déclaré sous
    /// `#[cfg(test)]` (ou déclaré par un fichier qui est lui-même de test).
    pub de_test: bool,
}

impl Fichier {
    /// Le fichier vu comme du code de production : commentaires et items
    /// `#[cfg(test)]` blanchis ; tout blanchi si le fichier est de test.
    pub fn production(&self) -> Masque {
        let mut source = masquer(&self.texte);
        if self.de_test || entierement_de_test(&source.masque) {
            let fin = source.masque.len();
            blanchir(&mut source, 0, fin);
        } else {
            for zone in zones_de_test(&source.masque) {
                blanchir(&mut source, zone.0, zone.1);
            }
        }
        source
    }

    /// Le texte sans ses commentaires, chaînes intactes, tests compris.
    pub fn code(&self) -> String {
        masquer(&self.texte).code.into_iter().collect()
    }
}

/// Le dossier `src/` de `stokesforge-app`.
pub fn dossier_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Le dossier `src/` de `stokesforge-app-state`, l'état sans egui (#263).
pub fn dossier_src_etat() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("stokesforge-app-state")
        .join("src")
}

/// Le préfixe des chemins des sources de `stokesforge-app-state` dans la liste
/// de [`sources`] : `app-state/state.rs`, `app-state/i18n/pseudo.rs`. Les
/// sources de `stokesforge-app` n'en ont pas (`app.rs`, `app/menus.rs`).
pub const PREFIXE_ETAT: &str = "app-state/";

/// Toutes les sources de l'application, sous-dossiers compris, triées par
/// chemin : celles de `stokesforge-app` (chemin relatif à son `src/`) **et**
/// celles de `stokesforge-app-state` (même chemin, préfixé de
/// [`PREFIXE_ETAT`]). Le texte d'interface, lui, se pose dans les deux.
pub fn sources() -> Vec<Fichier> {
    lire_dossiers(&[
        (dossier_src().as_path(), ""),
        (dossier_src_etat().as_path(), PREFIXE_ETAT),
    ])
}

/// Toutes les sources `.rs` sous `racine`, sous-dossiers compris, triées par
/// chemin ; les fichiers de test sont marqués.
pub fn lire(racine: &Path) -> Vec<Fichier> {
    lire_dossiers(&[(racine, "")])
}

/// Les sources `.rs` de chaque `(racine, préfixe)`, sous-dossiers compris, dont
/// le chemin est précédé de son préfixe ; triées par chemin, fichiers de test
/// marqués.
pub fn lire_dossiers(dossiers: &[(&Path, &str)]) -> Vec<Fichier> {
    let mut chemins: Vec<(PathBuf, String)> = Vec::new();
    for (racine, prefixe) in dossiers {
        let mut relatifs = Vec::new();
        parcourir(racine, "", &mut relatifs);
        for relatif in relatifs {
            chemins.push((racine.join(&relatif), format!("{prefixe}{relatif}")));
        }
    }
    chemins.sort_by(|a, b| a.1.cmp(&b.1));
    let mut fichiers: Vec<Fichier> = chemins
        .into_iter()
        .map(|(complet, chemin)| {
            let texte = std::fs::read_to_string(complet).expect("source lisible");
            Fichier {
                chemin,
                texte,
                de_test: false,
            }
        })
        .collect();
    marquer_les_fichiers_de_test(&mut fichiers);
    fichiers
}

fn parcourir(dossier: &Path, prefixe: &str, sortie: &mut Vec<String>) {
    for entree in std::fs::read_dir(dossier)
        .expect("dossier de sources")
        .flatten()
    {
        let chemin = entree.path();
        let nom = entree.file_name().to_string_lossy().into_owned();
        let relatif = format!("{prefixe}{nom}");
        if chemin.is_dir() {
            parcourir(&chemin, &format!("{relatif}/"), sortie);
        } else if chemin.extension().is_some_and(|ext| ext == "rs") {
            sortie.push(relatif);
        }
    }
}

// ---------------------------------------------------------------------------
// Les fichiers de test
// ---------------------------------------------------------------------------

/// Une déclaration `mod nom;` dont le module vit dans un autre fichier.
struct Declaration {
    position: usize,
    nom: String,
    /// La valeur d'un `#[path = "…"]` posé sur la déclaration.
    chemin: Option<String>,
}

/// Marque, jusqu'à point fixe, les fichiers des modules déclarés sous
/// `#[cfg(test)]` et ceux que déclare un fichier déjà de test.
fn marquer_les_fichiers_de_test(fichiers: &mut [Fichier]) {
    let connus: BTreeSet<String> = fichiers.iter().map(|f| f.chemin.clone()).collect();
    for fichier in fichiers.iter_mut() {
        fichier.de_test = fichier.de_test || entierement_de_test(&masquer(&fichier.texte).masque);
    }
    loop {
        let mut nouveaux = BTreeSet::new();
        for fichier in fichiers.iter() {
            let source = masquer(&fichier.texte);
            let zones = zones_de_test(&source.masque);
            for declaration in declarations(&source) {
                let sous_test = fichier.de_test
                    || zones
                        .iter()
                        .any(|(debut, fin)| (*debut..*fin).contains(&declaration.position));
                if !sous_test {
                    continue;
                }
                if let Some(cible) = fichier_du_module(&fichier.chemin, &declaration)
                    .into_iter()
                    .find(|candidat| connus.contains(candidat))
                {
                    nouveaux.insert(cible);
                }
            }
        }
        let mut change = false;
        for fichier in fichiers.iter_mut() {
            if !fichier.de_test && nouveaux.contains(&fichier.chemin) {
                fichier.de_test = true;
                change = true;
            }
        }
        if !change {
            return;
        }
    }
}

/// Les déclarations `mod nom;` (hors module en ligne `mod nom { … }`).
fn declarations(source: &Masque) -> Vec<Declaration> {
    let masque = &source.masque;
    let mut trouvees = Vec::new();
    for i in 0..masque.len() {
        if !mot_en(masque, i, "mod") {
            continue;
        }
        let mut j = i + 3;
        while masque.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        let nom = ident_en(masque, j);
        if nom.is_empty() {
            continue;
        }
        j += nom.chars().count();
        while masque.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        if masque.get(j) != Some(&';') {
            continue;
        }
        let debut = debut_d_item(masque, i);
        trouvees.push(Declaration {
            position: i,
            nom,
            chemin: attribut_path(source, debut, i),
        });
    }
    trouvees
}

/// La valeur du `#[path = "…"]` écrit entre `debut` et `fin`, s'il y en a un.
fn attribut_path(source: &Masque, debut: usize, fin: usize) -> Option<String> {
    let masque = &source.masque;
    let attribut = (debut..fin).find(|k| commence_par(masque, *k, "#[path"))?;
    let ouvrant = (attribut..fin).find(|k| masque[*k] == '"')?;
    let fermant = (ouvrant + 1..fin).find(|k| masque[*k] == '"')?;
    Some(source.original[ouvrant + 1..fermant].iter().collect())
}

/// Les fichiers où peut vivre le module `declaration`, déclaré dans `chemin` —
/// règles de rustc : à côté d'un `lib.rs` / `main.rs` / `mod.rs`, dans le
/// dossier du même nom sinon ; un `#[path]` est relatif au dossier du fichier.
fn fichier_du_module(chemin: &str, declaration: &Declaration) -> Vec<String> {
    let (dossier, fichier) = match chemin.rsplit_once('/') {
        Some((dossier, fichier)) => (format!("{dossier}/"), fichier),
        None => (String::new(), chemin),
    };
    if let Some(attribut) = &declaration.chemin {
        // Un chemin qui sort de `src/` ne désigne aucune source lue.
        return normaliser(&format!("{dossier}{attribut}"))
            .into_iter()
            .collect();
    }
    let base = if ["lib.rs", "main.rs", "mod.rs"].contains(&fichier) {
        dossier
    } else {
        format!("{dossier}{}/", fichier.trim_end_matches(".rs"))
    };
    let nom = &declaration.nom;
    vec![format!("{base}{nom}.rs"), format!("{base}{nom}/mod.rs")]
}

/// `a/./b/../c.rs` → `a/c.rs` ; `None` si le chemin remonte au-dessus de
/// `src/` (`../tests/support/sources.rs`).
fn normaliser(chemin: &str) -> Option<String> {
    let mut parties: Vec<&str> = Vec::new();
    for partie in chemin.split('/') {
        match partie {
            "" | "." => {}
            ".." => {
                parties.pop()?;
            }
            _ => parties.push(partie),
        }
    }
    Some(parties.join("/"))
}

// ---------------------------------------------------------------------------
// Les items de test
// ---------------------------------------------------------------------------

/// Le fichier s'ouvre sur un attribut intérieur `#![cfg(test)]` (parmi ses
/// attributs intérieurs de tête) : il est entier de test.
fn entierement_de_test(masque: &[char]) -> bool {
    let mut k = 0;
    loop {
        while masque.get(k).is_some_and(|c| c.is_whitespace()) {
            k += 1;
        }
        if commence_par(masque, k, "#![cfg(test)]") {
            return true;
        }
        if !commence_par(masque, k, "#![") {
            return false;
        }
        match fermante(masque, k + 2) {
            Some(fin) => k = fin + 1,
            None => return false,
        }
    }
}

/// Les étendues `[début, fin)` des éléments `#[cfg(test)]` : leurs attributs,
/// puis l'item jusqu'à son `;` ou à l'accolade qui ferme son corps — ou, pour
/// un champ, un bras de `match`, un argument, jusqu'à sa virgule.
pub fn zones_de_test(masque: &[char]) -> Vec<(usize, usize)> {
    const ATTRIBUT: &str = "#[cfg(test)]";
    let mut zones = Vec::new();
    let mut i = 0;
    while i < masque.len() {
        if !commence_par(masque, i, ATTRIBUT) {
            i += 1;
            continue;
        }
        let debut = debut_d_item(masque, i);
        let fin = fin_d_item(masque, i + ATTRIBUT.len());
        zones.push((debut, fin));
        i = fin.max(i + 1);
    }
    zones
}

/// Le début de l'item dont un attribut ou le mot-clé est en `position` :
/// recule sur les blancs, les attributs extérieurs `#[…]` et la visibilité.
fn debut_d_item(masque: &[char], position: usize) -> usize {
    let mut debut = position;
    loop {
        let mut k = debut;
        while k > 0 && masque[k - 1].is_whitespace() {
            k -= 1;
        }
        if k > 0 && masque[k - 1] == ']' {
            match ouvrante(masque, k - 1) {
                Some(crochet) if crochet > 0 && masque[crochet - 1] == '#' => {
                    debut = crochet - 1;
                    continue;
                }
                _ => {}
            }
        }
        if k > 0 && masque[k - 1] == ')' {
            if let Some(parenthese) = ouvrante(masque, k - 1) {
                if parenthese >= 3 && mot_en(masque, parenthese - 3, "pub") {
                    debut = parenthese - 3;
                    continue;
                }
            }
        }
        if k >= 3 && mot_en(masque, k - 3, "pub") {
            debut = k - 3;
            continue;
        }
        return debut;
    }
}

/// Les mots qui ouvrent un item ou une instruction terminés par `;` ou par un
/// corps entre accolades.
const MOTS_D_ITEM: &[&str] = &[
    "fn",
    "mod",
    "use",
    "impl",
    "struct",
    "enum",
    "union",
    "const",
    "static",
    "type",
    "trait",
    "macro_rules",
    "pub",
    "unsafe",
    "async",
    "extern",
    "let",
];

/// La fin (exclue) de l'élément qui suit l'attribut fini en `position`,
/// attributs suivants compris. Un item (`fn`, `mod`, `struct`…) finit après son
/// `;` de premier niveau, ou après l'accolade fermante de son corps. Tout autre
/// élément (champ, bras de `match`, argument) finit à sa virgule, ou avant
/// l'accolade ou la parenthèse fermante qui l'englobe : il n'avale jamais le
/// code qui le suit.
fn fin_d_item(masque: &[char], position: usize) -> usize {
    let mut k = position;
    loop {
        while masque.get(k).is_some_and(|c| c.is_whitespace()) {
            k += 1;
        }
        if !commence_par(masque, k, "#[") {
            break;
        }
        match fermante(masque, k + 1) {
            Some(fin) => k = fin + 1,
            None => return masque.len(),
        }
    }
    let item = MOTS_D_ITEM.iter().any(|mot| mot_en(masque, k, mot));
    let mut profondeur = 0usize;
    while k < masque.len() {
        match masque[k] {
            '(' | '[' => profondeur += 1,
            ')' | ']' | '}' if profondeur == 0 => return k,
            ')' | ']' => profondeur -= 1,
            ';' if profondeur == 0 => return k + 1,
            ',' if profondeur == 0 && !item => return k + 1,
            '{' if profondeur == 0 => {
                let fin = fermante(masque, k).map_or(masque.len(), |fin| fin + 1);
                if item {
                    return fin;
                }
                k = fin;
                continue;
            }
            '{' => profondeur += 1,
            '}' => profondeur -= 1,
            _ => {}
        }
        k += 1;
    }
    masque.len()
}

fn blanchir(source: &mut Masque, debut: usize, fin: usize) {
    for k in debut..fin.min(source.masque.len()) {
        if source.original[k] != '\n' {
            source.masque[k] = ' ';
            source.code[k] = ' ';
        }
    }
}

// ---------------------------------------------------------------------------
// Le masque
// ---------------------------------------------------------------------------

/// Une source sous trois vues de même longueur (en caractères), donc aux mêmes
/// positions.
#[derive(Debug, Clone)]
pub struct Masque {
    /// Commentaires blanchis, contenu des chaînes remplacé : `A` pour une
    /// lettre, `_` pour le reste. Un motif cherché ici n'est jamais dans un
    /// commentaire ni dans une chaîne.
    pub masque: Vec<char>,
    /// Commentaires blanchis, chaînes intactes.
    pub code: Vec<char>,
    pub original: Vec<char>,
}

impl Masque {
    pub fn code_en_texte(&self) -> String {
        self.code.iter().collect()
    }
}

/// Le masque du texte, sans vue de fichier : tests compris.
pub fn masquer(texte: &str) -> Masque {
    let original: Vec<char> = texte.chars().collect();
    let mut masque = original.clone();
    let mut code = original.clone();
    let n = original.len();
    let mut i = 0;
    while i < n {
        let c = original[i];
        let suivant = original.get(i + 1).copied();
        // Commentaire de ligne.
        if c == '/' && suivant == Some('/') {
            while i < n && original[i] != '\n' {
                masque[i] = ' ';
                code[i] = ' ';
                i += 1;
            }
            continue;
        }
        // Commentaire de bloc, imbriqué.
        if c == '/' && suivant == Some('*') {
            let mut profondeur = 0;
            while i < n {
                if original[i] == '/' && original.get(i + 1) == Some(&'*') {
                    profondeur += 1;
                    for k in [i, i + 1] {
                        masque[k] = ' ';
                        code[k] = ' ';
                    }
                    i += 2;
                } else if original[i] == '*' && original.get(i + 1) == Some(&'/') {
                    profondeur -= 1;
                    for k in [i, i + 1] {
                        masque[k] = ' ';
                        code[k] = ' ';
                    }
                    i += 2;
                    if profondeur == 0 {
                        break;
                    }
                } else {
                    if original[i] != '\n' {
                        masque[i] = ' ';
                        code[i] = ' ';
                    }
                    i += 1;
                }
            }
            continue;
        }
        let precedent_ident = i > 0 && est_ident(original[i - 1]);
        // Chaîne brute : r"…", r#"…"#, br#"…"#.
        if (c == 'r' || (c == 'b' && suivant == Some('r'))) && !precedent_ident {
            let mut j = i + if c == 'b' { 2 } else { 1 };
            let mut dieses = 0;
            while original.get(j) == Some(&'#') {
                dieses += 1;
                j += 1;
            }
            if original.get(j) == Some(&'"') {
                j += 1;
                while j < n {
                    if original[j] == '"'
                        && (0..dieses).all(|k| original.get(j + 1 + k) == Some(&'#'))
                    {
                        break;
                    }
                    masquer_caractere(&original, &mut masque, j);
                    j += 1;
                }
                i = (j + 1 + dieses).min(n);
                continue;
            }
        }
        // Chaîne ordinaire.
        if c == '"' {
            let mut j = i + 1;
            while j < n && original[j] != '"' {
                if original[j] == '\\' {
                    masque[j] = '_';
                    j += 1;
                    if j < n {
                        let echappe = original[j];
                        masquer_neutre(&original, &mut masque, j);
                        j += 1;
                        if echappe == 'u' && original.get(j) == Some(&'{') {
                            while j < n && original[j] != '}' {
                                masquer_neutre(&original, &mut masque, j);
                                j += 1;
                            }
                            if j < n {
                                masquer_neutre(&original, &mut masque, j);
                                j += 1;
                            }
                        }
                    }
                    continue;
                }
                // Un champ de format (`{code}`, `{label:>4}`) n'est pas du texte :
                // son contenu est neutre. `{{` est une accolade littérale.
                if original[j] == '{' && original.get(j + 1) != Some(&'{') {
                    while j < n && original[j] != '}' && original[j] != '"' {
                        masquer_neutre(&original, &mut masque, j);
                        j += 1;
                    }
                    continue;
                }
                masquer_caractere(&original, &mut masque, j);
                j += 1;
            }
            i = (j + 1).min(n);
            continue;
        }
        // Littéral de caractère (et non une durée de vie).
        if c == '\'' {
            if suivant == Some('\\') {
                let mut j = i + 2;
                while j < n && original[j] != '\'' {
                    masque[j] = '_';
                    j += 1;
                }
                masque[i + 1] = '_';
                i = (j + 1).min(n);
                continue;
            }
            if original.get(i + 2) == Some(&'\'') {
                masque[i + 1] = '_';
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    Masque {
        masque,
        code,
        original,
    }
}

/// Dans une chaîne : une lettre devient `A`, le reste `_`, sauf les fins de ligne.
fn masquer_caractere(original: &[char], masque: &mut [char], i: usize) {
    masque[i] = if original[i] == '\n' {
        '\n'
    } else if original[i].is_alphabetic() {
        'A'
    } else {
        '_'
    };
}

/// Dans une séquence d'échappement : jamais une lettre de texte.
fn masquer_neutre(original: &[char], masque: &mut [char], i: usize) {
    masque[i] = if original[i] == '\n' { '\n' } else { '_' };
}

// ---------------------------------------------------------------------------
// Outils de lecture du masque
// ---------------------------------------------------------------------------

pub fn est_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Position du délimiteur fermant qui répond à celui ouvert en `ouvrante`.
pub fn fermante(masque: &[char], ouvrante: usize) -> Option<usize> {
    let mut profondeur = 0usize;
    for (i, c) in masque.iter().enumerate().skip(ouvrante) {
        match c {
            '(' | '[' | '{' => profondeur += 1,
            ')' | ']' | '}' => {
                profondeur = profondeur.checked_sub(1)?;
                if profondeur == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Position du délimiteur ouvrant qui répond à celui fermé en `fermante`.
pub fn ouvrante(masque: &[char], fermante: usize) -> Option<usize> {
    let mut profondeur = 0usize;
    for i in (0..=fermante).rev() {
        match masque[i] {
            ')' | ']' | '}' => profondeur += 1,
            '(' | '[' | '{' => {
                profondeur = profondeur.checked_sub(1)?;
                if profondeur == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn commence_par(masque: &[char], position: usize, motif: &str) -> bool {
    motif
        .chars()
        .enumerate()
        .all(|(k, c)| masque.get(position + k) == Some(&c))
}

/// `motif` écrit en `position` comme un mot entier : ni précédé ni suivi d'un
/// caractère d'identifiant.
pub fn mot_en(masque: &[char], position: usize, motif: &str) -> bool {
    commence_par(masque, position, motif)
        && (position == 0 || !est_ident(masque[position - 1]))
        && !masque
            .get(position + motif.chars().count())
            .is_some_and(|c| est_ident(*c))
}

pub fn ident_en(masque: &[char], position: usize) -> String {
    masque[position.min(masque.len())..]
        .iter()
        .take_while(|c| est_ident(**c))
        .collect()
}

pub fn ligne_de(original: &[char], position: usize) -> usize {
    1 + original[..position].iter().filter(|c| **c == '\n').count()
}

// ---------------------------------------------------------------------------
// Les fonctions
// ---------------------------------------------------------------------------

/// Les fonctions du masque : position du `fn`, nom qualifié par le bloc `impl`
/// qui la contient (`Type::nom`, `nom` hors `impl`).
pub fn fonctions(masque: &[char]) -> Vec<(usize, String)> {
    let impls = blocs_impl(masque);
    let mut trouvees = Vec::new();
    for i in 0..masque.len() {
        if commence_par(masque, i, "fn ") && (i == 0 || !est_ident(masque[i - 1])) {
            let nom = ident_en(masque, i + 3);
            if nom.is_empty() {
                continue;
            }
            let qualifie = impls
                .iter()
                .find(|(debut, fin, _)| *debut < i && i < *fin)
                .map_or(nom.clone(), |(_, _, cible)| format!("{cible}::{nom}"));
            trouvees.push((i, qualifie));
        }
    }
    trouvees
}

/// Le dernier segment d'un nom qualifié : `nom` pour `Type::nom`.
pub fn nom_court(qualifie: &str) -> &str {
    qualifie.rsplit("::").next().unwrap_or(qualifie)
}

/// L'étendue `[accolade ouvrante, accolade fermante]` du corps de la fonction
/// dont le `fn` est en `position` ; `None` pour une déclaration sans corps.
pub fn corps(masque: &[char], position: usize) -> Option<(usize, usize)> {
    // Le `;` d'un type tableau (`[u8; 4]`) de la signature n'est pas la fin
    // d'une déclaration : seuls comptent ceux de premier niveau.
    let mut profondeur = 0usize;
    for k in position..masque.len() {
        match masque[k] {
            '(' | '[' => profondeur += 1,
            ')' | ']' => profondeur = profondeur.saturating_sub(1),
            ';' if profondeur == 0 => return None,
            '{' if profondeur == 0 => return Some((k, fermante(masque, k)?)),
            _ => {}
        }
    }
    None
}

/// Une fonction de production trouvée par [`fonctions_nommees`].
#[derive(Debug, Clone)]
pub struct Fonction {
    pub chemin: String,
    pub qualifie: String,
    pub ligne: usize,
    /// Le corps, commentaires blanchis, chaînes intactes.
    pub corps: String,
}

/// Les fonctions de production nommées `nom` (dernier segment du nom qualifié :
/// une méthode `Type::nom` compte), où qu'elles soient dans `sources`.
pub fn fonctions_nommees(sources: &[Fichier], nom: &str) -> Vec<Fonction> {
    let mut trouvees = Vec::new();
    for fichier in sources {
        let source = fichier.production();
        for (position, qualifie) in fonctions(&source.masque) {
            if nom_court(&qualifie) != nom {
                continue;
            }
            let Some((debut, fin)) = corps(&source.masque, position) else {
                continue;
            };
            trouvees.push(Fonction {
                chemin: fichier.chemin.clone(),
                qualifie,
                ligne: ligne_de(&source.original, position),
                corps: source.code[debut..=fin].iter().collect(),
            });
        }
    }
    trouvees
}

/// L'unique fonction de production nommée `nom` — libre ou méthode
/// `Type::nom` : une fonction devenue méthode reste trouvée (#122). Échoue, en
/// le disant, si elle est introuvable ou en plusieurs exemplaires.
pub fn fonction_unique(sources: &[Fichier], nom: &str) -> Fonction {
    let mut trouvees = fonctions_nommees(sources, nom);
    assert!(
        trouvees.len() == 1,
        "`fn {nom}` attendue une fois dans les sources de production de src/, trouvée {} fois : {:?}",
        trouvees.len(),
        trouvees
            .iter()
            .map(|f| format!("{}:{}", f.chemin, f.ligne))
            .collect::<Vec<_>>()
    );
    trouvees.remove(0)
}

/// Les blocs `impl` : (début, fin, type visé).
pub fn blocs_impl(masque: &[char]) -> Vec<(usize, usize, String)> {
    let mut blocs = Vec::new();
    for i in 0..masque.len() {
        if !commence_par(masque, i, "impl")
            || (i > 0 && est_ident(masque[i - 1]))
            || masque.get(i + 4).is_some_and(|c| est_ident(*c))
        {
            continue;
        }
        let Some(accolade) = (i..masque.len()).find(|k| masque[*k] == '{' || masque[*k] == ';')
        else {
            continue;
        };
        if masque[accolade] != '{' {
            continue;
        }
        let Some(fin) = fermante(masque, accolade) else {
            continue;
        };
        let entete: String = masque[i + 4..accolade].iter().collect();
        blocs.push((i, fin, cible_de_impl(&entete)));
    }
    blocs
}

/// Le type visé par l'en-tête d'un `impl` : `Type` pour `impl Type`,
/// `impl<T> Type<T>`, `impl Trait for Type`.
fn cible_de_impl(entete: &str) -> String {
    let mut reste = entete.trim();
    if let Some((_, apres)) = reste.rsplit_once(" for ") {
        reste = apres.trim();
    } else if reste.starts_with('<') {
        let mut profondeur = 0usize;
        for (k, c) in reste.char_indices() {
            match c {
                '<' => profondeur += 1,
                '>' => {
                    profondeur = profondeur.saturating_sub(1);
                    if profondeur == 0 {
                        reste = reste[k + 1..].trim();
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    reste.chars().take_while(|c| est_ident(*c)).collect()
}
