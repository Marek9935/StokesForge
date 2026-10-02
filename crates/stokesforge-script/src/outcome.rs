//! Ce qu'une exécution de script a produit.

/// Résultat d'une exécution de script.
///
/// La sortie du script est **capturée**, jamais mêlée à celle du processus
/// hôte : `print` atterrit dans [`ScriptOutcome::stdout`].
///
/// # Limite connue : la sortie n'est pas diffusée au fil de l'eau
///
/// Dans le socle, `stdout` et `stderr` ne sont lisibles qu'**après** la fin du
/// script : une exécution longue n'affiche rien avant de finir. C'est une
/// limite assumée, pas un oubli — la diffusion incrémentale demanderait un
/// canal de progression que le protocole de ce socle ne porte pas.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptOutcome {
    /// Ce que le script a écrit sur la sortie standard (`print`), jamais perdu
    /// dans celle du processus.
    pub stdout: String,
    /// Ce que le script a écrit sur la sortie d'erreur.
    pub stderr: String,
    /// Message d'erreur **avec sa trace d'appels Python complète**, si le
    /// script a échoué.
    ///
    /// Quand [`ScriptOutcome::interrupted`] est vrai, ce champ contient
    /// normalement la trace du `KeyboardInterrupt` qui a arrêté le script :
    /// c'est un renseignement (*où* le script s'est arrêté), pas un échec à
    /// présenter en rouge.
    pub error: Option<String>,
    /// Vrai si l'exécution s'est arrêtée parce qu'une interruption avait été
    /// demandée pendant qu'elle tournait.
    pub interrupted: bool,
}

impl ScriptOutcome {
    /// Vrai si le script s'est terminé sans exception et sans interruption.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.error.is_none() && !self.interrupted
    }
}
