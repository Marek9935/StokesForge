//! Identité des nœuds de l'arbre de projet.

/// Identité d'un nœud : compteur monotone, jamais réutilisé sur toute la vie
/// d'un projet (rechargement compris). Opaque pour les clients ; c'est aussi
/// la clé que l'API de script exposera.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u64);

impl NodeId {
    /// Reconstruit un identifiant depuis sa valeur brute (persistance uniquement).
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Valeur brute (persistance uniquement).
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "nœud {}", self.0)
    }
}

/// Allocateur d'identifiants : `next()` rend des valeurs strictement croissantes.
/// N'est jamais remis en arrière — c'est le `Project` (tranche suivante) qui le
/// possède, hors de l'arbre, pour qu'une annulation ne le fasse pas reculer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdAllocator {
    next: u64,
}

impl IdAllocator {
    /// Premier identifiant rendu : `NodeId::from_raw(1)`. Zéro n'est jamais attribué.
    pub fn new() -> Self {
        Self { next: 1 }
    }

    /// Reprend après un chargement. `hint` est le **prochain identifiant à attribuer**, tel que
    /// [`IdAllocator::peek`] le rend et que la persistance l'écrit ; `highest_seen` est le plus grand
    /// identifiant effectivement lu dans le fichier. Le compteur repart à
    /// `max(hint, highest_seen + 1)` — la seconde moitié est une ceinture gratuite contre un fichier
    /// dont le compteur persisté serait incohérent avec son contenu.
    pub fn resume(hint: u64, highest_seen: u64) -> Self {
        Self {
            next: hint.max(highest_seen.saturating_add(1)),
        }
    }

    /// Attribue l'identifiant suivant.
    // `IdAllocator` n'est pas un itérateur (il n'y a pas de fin, pas d'`Iterator::next`
    // à honorer) ; le nom `next` reste le plus clair pour cette API.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> NodeId {
        let id = NodeId(self.next);
        self.next += 1;
        id
    }

    /// Valeur du prochain identifiant qui serait rendu (à persister).
    pub fn peek(&self) -> u64 {
        self.next
    }
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_allocator_starts_at_one() {
        let mut alloc = IdAllocator::new();
        assert_eq!(alloc.next().raw(), 1);
    }

    #[test]
    fn successive_ids_are_strictly_increasing() {
        let mut alloc = IdAllocator::new();
        let a = alloc.next();
        let b = alloc.next();
        assert!(b > a);
    }

    #[test]
    fn resume_uses_the_hint_when_nothing_contradicts_it() {
        // Le fichier dit « le prochain est 10 » ; son plus grand identifiant
        // lu est 3, ce qui ne contredit rien : le compteur repart bien à 10.
        let mut alloc = IdAllocator::resume(10, 3);
        assert_eq!(alloc.next().raw(), 10);
    }

    #[test]
    fn resume_falls_back_to_the_belt_when_the_hint_is_stale() {
        // Le compteur persisté (3) est incohérent avec le contenu du
        // fichier (plus grand identifiant lu : 10) : il rendrait un
        // identifiant déjà attribué. La ceinture le rattrape au-dessus.
        let mut alloc = IdAllocator::resume(3, 10);
        assert_eq!(alloc.next().raw(), 11);
    }

    #[test]
    fn resume_of_an_empty_project_starts_at_one() {
        // Projet vide : aucun identifiant lu, donc `highest_seen == 0`
        // (zéro n'est jamais attribué).
        let mut alloc = IdAllocator::resume(1, 0);
        assert_eq!(alloc.next().raw(), 1);
    }

    #[test]
    fn resume_does_not_panic_at_the_u64_boundary() {
        // `highest_seen.saturating_add(1)` doit saturer plutôt que déborder ;
        // un `highest_seen` de `u64::MAX` est inatteignable en pratique, mais
        // le crate s'interdit toute panique.
        let alloc = IdAllocator::resume(u64::MAX, u64::MAX);
        assert_eq!(alloc.peek(), u64::MAX);
    }

    #[test]
    fn peek_does_not_consume() {
        let alloc = IdAllocator::new();
        assert_eq!(alloc.peek(), 1);
        assert_eq!(alloc.peek(), 1);
    }

    #[test]
    fn display_formats_as_expected() {
        assert_eq!(NodeId::from_raw(412).to_string(), "nœud 412");
    }

    #[test]
    fn from_raw_and_raw_are_inverse() {
        let id = NodeId::from_raw(42);
        assert_eq!(NodeId::from_raw(id.raw()), id);
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(IdAllocator::default(), IdAllocator::new());
    }
}
