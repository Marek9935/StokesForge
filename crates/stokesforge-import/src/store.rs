//! Persistance des recettes d'import sur disque : un unique fichier JSON
//! dans le dossier de config utilisateur, jamais de panique si ce fichier
//! est absent, illisible, ou corrompu (invariant 10).

use crate::recipe::ImportRecipe;

/// L'ensemble des recettes enregistrées par l'utilisateur.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RecipeStore {
    recipes: Vec<ImportRecipe>,
}

impl RecipeStore {
    /// Charge depuis le fichier de config utilisateur — jamais de panique :
    /// fichier absent, illisible ou JSON corrompu rendent `Self::default()`
    /// (liste vide), pas une erreur qui bloquerait l'appelant.
    pub fn load() -> Self {
        recipes_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Comme [`RecipeStore::load`], plus les modèles fournis d'office
    /// ([`crate::builtin_recipes`]) que l'utilisateur n'a pas remplacés par un
    /// modèle de même nom. Les siens passent en premier.
    pub fn load_with_builtins() -> Self {
        let mut store = Self::load();
        for builtin in crate::builtin_recipes() {
            if !store.recipes.iter().any(|r| r.name == builtin.name) {
                store.recipes.push(builtin);
            }
        }
        store
    }

    /// Ce modèle est-il un modèle fourni, tel quel (pas une version modifiée) ?
    pub fn is_builtin(recipe: &ImportRecipe) -> bool {
        crate::builtin_recipes().iter().any(|b| b == recipe)
    }

    /// Écrit sur disque (crée le dossier de config s'il n'existe pas). Les
    /// modèles fournis, inchangés, ne sont pas écrits : ils viennent du
    /// programme, pas du fichier de l'utilisateur.
    pub fn save(&self) -> std::io::Result<()> {
        let path = recipes_path().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "aucun dossier de config exploitable pour cet OS",
            )
        })?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let user = Self {
            recipes: self
                .recipes
                .iter()
                .filter(|recipe| !Self::is_builtin(recipe))
                .cloned()
                .collect(),
        };
        let json = serde_json::to_string_pretty(&user)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    pub fn recipes(&self) -> &[ImportRecipe] {
        &self.recipes
    }

    /// Ajoute la recette, ou remplace celle de même `name` si elle existe
    /// déjà.
    pub fn upsert(&mut self, recipe: ImportRecipe) {
        if let Some(existing) = self.recipes.iter_mut().find(|r| r.name == recipe.name) {
            *existing = recipe;
        } else {
            self.recipes.push(recipe);
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.recipes.retain(|r| r.name != name);
    }
}

/// Chemin du fichier de recettes : `directories::ProjectDirs::from("", "", "StokesForge")`,
/// dossier de config, fichier `import_recipes.json`. `None` si l'OS ne
/// fournit aucun dossier de config exploitable (très rare).
fn recipes_path() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("", "", "StokesForge")
        .map(|dirs| dirs.config_dir().join("import_recipes.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::ImportRecipe;

    #[test]
    fn upsert_remplace_sans_dupliquer() {
        let mut store = RecipeStore::default();
        store.upsert(ImportRecipe::new("Jasco"));
        store.upsert(ImportRecipe::new("Autre"));
        assert_eq!(store.recipes().len(), 2);

        let mut modifiee = ImportRecipe::new("Jasco");
        modifiee.footer_skip_lines = 3;
        store.upsert(modifiee);

        assert_eq!(store.recipes().len(), 2);
        let jasco = store.recipes().iter().find(|r| r.name == "Jasco").unwrap();
        assert_eq!(jasco.footer_skip_lines, 3);
    }

    #[test]
    fn remove_retire_par_nom() {
        let mut store = RecipeStore::default();
        store.upsert(ImportRecipe::new("Jasco"));
        store.remove("Jasco");
        assert!(store.recipes().is_empty());
    }

    #[test]
    fn load_sans_fichier_rend_liste_vide_sans_paniquer() {
        // On ne peut pas garantir l'absence du vrai fichier de config sur la
        // machine qui exécute les tests ; on vérifie seulement l'invariant
        // qui compte vraiment : `load` ne panique jamais et rend toujours
        // un `RecipeStore` exploitable, y compris quand le fichier n'existe
        // pas ou n'est pas lisible.
        let store = RecipeStore::load();
        let _ = store.recipes().len();
    }

    #[test]
    fn deserialisation_json_corrompu_rend_defaut() {
        let corrompu = "{ceci n'est pas du json";
        let result: Result<RecipeStore, _> = serde_json::from_str(corrompu);
        let store = result.unwrap_or_default();
        assert!(store.recipes().is_empty());
    }

    /// Protège les tests qui redirigent `XDG_CONFIG_HOME` contre ceux qui en
    /// feraient autant en parallèle dans ce même binaire de test —
    /// `std::env::set_var` est un état du *processus*, jamais scopé à un
    /// thread.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// `save` puis `load`, en aller-retour réel sur disque — jamais exercé
    /// avant ce test (les autres testent la logique en mémoire, ou la
    /// tolérance de `load` à un fichier absent/corrompu, jamais l'écriture
    /// elle-même). `recipes_path` suit `directories::ProjectDirs`, qui
    /// respecte `XDG_CONFIG_HOME` sur Linux : la rediriger vers un
    /// répertoire temporaire isole ce test du vrai dossier de config de la
    /// machine qui l'exécute.
    #[test]
    fn save_puis_load_realise_un_aller_retour_fidele_sur_disque() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let previous = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", dir.path());

        let mut store = RecipeStore::default();
        let mut jasco = ImportRecipe::new("Jasco UV-Vis");
        jasco.footer_skip_lines = 2;
        store.upsert(jasco);
        store.upsert(ImportRecipe::new("Fluo"));

        let save_result = store.save();
        let reloaded = RecipeStore::load();

        match previous {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }

        save_result.expect("l'écriture sur le répertoire temporaire doit réussir");
        assert_eq!(
            reloaded.recipes(),
            store.recipes(),
            "ce qui est relu après `save` doit être bit à bit ce qui a été écrit"
        );
    }
}
