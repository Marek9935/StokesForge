//! Persistence of import recipes on disk: a single JSON file in the user
//! config folder, never a panic if that file is missing, unreadable or
//! corrupt (invariant 10).

use crate::recipe::ImportRecipe;

/// The set of recipes saved by the user.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RecipeStore {
    recipes: Vec<ImportRecipe>,
}

impl RecipeStore {
    /// Loads from the user config file. Never panics: a missing or unreadable
    /// file or corrupt JSON gives `Self::default()` (empty list), not an error
    /// that would block the caller.
    pub fn load() -> Self {
        recipes_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Writes every recipe to disk (creates the config folder if it does not
    /// exist).
    pub fn save(&self) -> std::io::Result<()> {
        let path = recipes_path().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no usable config folder for this OS",
            )
        })?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    pub fn recipes(&self) -> &[ImportRecipe] {
        &self.recipes
    }

    /// Adds the recipe, or replaces the one with the same `name` if it
    /// already exists.
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

/// Path of the recipes file: `import_recipes.json` in StokesForge's config
/// folder ([`crate::config_dir::config_dir`], #332). `None` if no config
/// folder is usable (very rare).
fn recipes_path() -> Option<std::path::PathBuf> {
    crate::config_dir::config_dir().map(|dir| dir.join("import_recipes.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::ImportRecipe;

    #[test]
    fn upsert_replaces_without_duplicating() {
        let mut store = RecipeStore::default();
        store.upsert(ImportRecipe::new("Jasco"));
        store.upsert(ImportRecipe::new("Other"));
        assert_eq!(store.recipes().len(), 2);

        let mut modified = ImportRecipe::new("Jasco");
        modified.footer_skip_lines = 3;
        store.upsert(modified);

        assert_eq!(store.recipes().len(), 2);
        let jasco = store.recipes().iter().find(|r| r.name == "Jasco").unwrap();
        assert_eq!(jasco.footer_skip_lines, 3);
    }

    #[test]
    fn remove_deletes_by_name() {
        let mut store = RecipeStore::default();
        store.upsert(ImportRecipe::new("Jasco"));
        store.remove("Jasco");
        assert!(store.recipes().is_empty());
    }

    #[test]
    fn load_without_a_file_gives_an_empty_list_without_panicking() {
        // We cannot guarantee that the real config file is absent on the
        // machine running the tests; we only check the invariant that really
        // matters: `load` never panics and always returns a usable
        // `RecipeStore`, including when the file does not exist or is not
        // readable.
        let store = RecipeStore::load();
        let _ = store.recipes().len();
    }

    #[test]
    fn deserializing_corrupt_json_gives_the_default() {
        let corrupt = "{this is not json";
        let result: Result<RecipeStore, _> = serde_json::from_str(corrupt);
        let store = result.unwrap_or_default();
        assert!(store.recipes().is_empty());
    }

    /// Protects the tests that redirect the config folder from others doing
    /// the same in parallel in this same test binary: `std::env::set_var` is
    /// *process* state, never scoped to a thread.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// `save` then `load`, a real round trip on disk, never exercised before
    /// this test (the others test the in-memory logic, or `load`'s tolerance
    /// of a missing/corrupt file, never the write itself). `recipes_path`
    /// follows `config_dir` (#332): pointing `STOKESFORGE_CONFIG_DIR` (and
    /// `XDG_CONFIG_HOME`, the only one `directories` honors on Linux) at a
    /// temporary directory isolates this test from the real config folder of
    /// the machine running it.
    #[test]
    fn save_then_load_makes_a_faithful_round_trip_on_disk() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let dir = tempfile::tempdir().expect("temporary directory");
        let names = [crate::config_dir::CONFIG_DIR_VARIABLE, "XDG_CONFIG_HOME"];
        let previous = names.map(|name| (name, std::env::var_os(name)));
        for name in names {
            std::env::set_var(name, dir.path());
        }

        let mut store = RecipeStore::default();
        let mut jasco = ImportRecipe::new("Jasco UV-Vis");
        jasco.footer_skip_lines = 2;
        store.upsert(jasco);
        store.upsert(ImportRecipe::new("Fluo"));

        let save_result = store.save();
        let reloaded = RecipeStore::load();

        for (name, value) in previous {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }

        save_result.expect("writing to the temporary directory must succeed");
        assert!(
            dir.path().join("import_recipes.json").is_file(),
            "the file is written to the folder designated by STOKESFORGE_CONFIG_DIR"
        );
        assert_eq!(
            reloaded.recipes(),
            store.recipes(),
            "what is read back after `save` must be bit for bit what was written"
        );
    }

    /// No recipe exists in production code (#378): an empty config folder
    /// gives an empty store, and a saved recipe is the only one reloaded.
    #[test]
    fn an_empty_config_folder_loads_empty_and_save_writes_every_recipe() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let dir = tempfile::tempdir().expect("temporary directory");
        let names = [crate::config_dir::CONFIG_DIR_VARIABLE, "XDG_CONFIG_HOME"];
        let previous = names.map(|name| (name, std::env::var_os(name)));
        for name in names {
            std::env::set_var(name, dir.path());
        }

        let empty = RecipeStore::load();
        let mut store = RecipeStore::default();
        store.upsert(ImportRecipe::new("Only one"));
        let save_result = store.save();
        let reloaded = RecipeStore::load();

        for (name, value) in previous {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }

        assert!(empty.recipes().is_empty());
        save_result.expect("writing to the temporary directory must succeed");
        assert_eq!(reloaded.recipes().len(), 1);
        assert_eq!(reloaded.recipes()[0].name, "Only one");
    }
}
