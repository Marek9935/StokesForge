//! `CommandBus` : la seule porte d'entrée autorisée pour muter un
//! `plotyx_core::Project`. Voir `docs/rustrewrite/roadmap.md` phase 2 pour
//! le contrat (application avant empilement, delta avant snapshot, macros
//! avec abort propre, durée de vie des objets supprimés via les clés
//! d'arène de `plotyx-core`).
//!
//! Dépendance à sens unique : ce crate connaît `plotyx-core`, jamais
//! l'inverse.

// Squelette de phase 0 — le bus réel (phase 2) n'est pas encore construit.
#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles_and_links() {
        assert_eq!(2 + 2, 4);
    }
}
