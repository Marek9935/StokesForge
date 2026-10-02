//! Identité des nœuds de l'arbre de projet.
//!
//! `NodeId` lui-même vit dans `stokesforge-id`, un crate minimal sans dépendance :
//! voir sa documentation de tête pour la raison (un cycle Cargo à casser en
//! phase R3, entre ce crate et `stokesforge-plot`). Ce module ne fait que le
//! réexporter — `stokesforge_core::NodeId` désigne exactement le même type
//! qu'avant, aucun appelant n'a rien à changer — et porte l'allocateur, qui
//! lui reste ici : il n'a besoin que de construire des `NodeId`, jamais de
//! `stokesforge-plot`.
pub use stokesforge_id::NodeId;

use crate::error::{CoreError, CoreResult};

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

    /// Attribue l'identifiant suivant. `Err(IdentifiersExhausted)` si `u64`
    /// est épuisé, plutôt que de déborder silencieusement (B1,
    /// `guide-de-correction-robustesse.md` §F2/F3/F4) : avant ce correctif,
    /// `self.next += 1` sur `u64::MAX` débordait — panique en debug,
    /// compteur qui **recule** en release, ouvrant la porte à deux nœuds
    /// sous le même identifiant. `self.next` n'est incrémenté qu'après
    /// vérification, donc un appel qui échoue laisse l'allocateur inchangé
    /// (et tout appel suivant échoue pareillement, jamais de régression).
    // `IdAllocator` n'est pas un itérateur (il n'y a pas de fin, pas d'`Iterator::next`
    // à honorer) ; le nom `next` reste le plus clair pour cette API.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> CoreResult<NodeId> {
        let id = NodeId::from_raw(self.next);
        self.next = self
            .next
            .checked_add(1)
            .ok_or(CoreError::IdentifiersExhausted)?;
        Ok(id)
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
        assert_eq!(alloc.next().unwrap().raw(), 1);
    }

    #[test]
    fn successive_ids_are_strictly_increasing() {
        let mut alloc = IdAllocator::new();
        let a = alloc.next().unwrap();
        let b = alloc.next().unwrap();
        assert!(b > a);
    }

    #[test]
    fn resume_uses_the_hint_when_nothing_contradicts_it() {
        // Le fichier dit « le prochain est 10 » ; son plus grand identifiant
        // lu est 3, ce qui ne contredit rien : le compteur repart bien à 10.
        let mut alloc = IdAllocator::resume(10, 3);
        assert_eq!(alloc.next().unwrap().raw(), 10);
    }

    #[test]
    fn resume_falls_back_to_the_belt_when_the_hint_is_stale() {
        // Le compteur persisté (3) est incohérent avec le contenu du
        // fichier (plus grand identifiant lu : 10) : il rendrait un
        // identifiant déjà attribué. La ceinture le rattrape au-dessus.
        let mut alloc = IdAllocator::resume(3, 10);
        assert_eq!(alloc.next().unwrap().raw(), 11);
    }

    #[test]
    fn resume_of_an_empty_project_starts_at_one() {
        // Projet vide : aucun identifiant lu, donc `highest_seen == 0`
        // (zéro n'est jamais attribué).
        let mut alloc = IdAllocator::resume(1, 0);
        assert_eq!(alloc.next().unwrap().raw(), 1);
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

    // `display_formats_as_expected` et `from_raw_and_raw_are_inverse` ont
    // déménagé dans `stokesforge-id` avec le type qu'elles testent (phase R3).

    #[test]
    fn default_matches_new() {
        assert_eq!(IdAllocator::default(), IdAllocator::new());
    }
}
