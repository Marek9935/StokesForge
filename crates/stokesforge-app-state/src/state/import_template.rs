//! Templates seen from the import screen: the offer to save one, the "Save as
//! Template" and "Manage Templates" windows, the plain-words conditions, the
//! answers remembered per kind of file.
//!
//! A template is a recipe and its **conditions** in the one template store
//! (`stokesforge_import::RecipeStore`). The detector's own reading of the
//! file gives conditions that are offered pre-filled and can each be unchecked
//! ([`automatic_conditions`]); the user adds others (number of columns, a
//! header key, the file name, the extension). Every write goes to the store at
//! once; a failure is a status-bar message.

use stokesforge_import::sniff::SepKind;
use stokesforge_import::{
    Condition, DecimalStyle, ImportRecipe, NameSource, PropField, PropTarget,
};

use super::import_check::{sep_name, short_data_type, Fingerprint};
use super::AppState;
use crate::tr;

// -- Conditions in plain words -----------------------------------------------

/// The delimiter of a numbers condition, as a word or a sign.
fn sep_words(sep: &str) -> String {
    match sep {
        "tab" => tr!("import-sepsymbol-tab"),
        "space" => tr!("import-sepsymbol-space"),
        other => other.to_string(),
    }
}

/// A plain sentence continuing « Used for files that ».
pub fn condition_sentence(condition: &Condition) -> String {
    match condition {
        Condition::Columns { at_least: false, n } => {
            tr!("import-condition-columns-eq", n = n.to_string())
        }
        Condition::Columns { at_least: true, n } => {
            tr!("import-condition-columns-min", n = n.to_string())
        }
        Condition::HeaderKey {
            key,
            value,
            contains: false,
        } => tr!(
            "import-condition-key-eq",
            key = key.clone(),
            value = value.clone()
        ),
        Condition::HeaderKey {
            key,
            value,
            contains: true,
        } => tr!(
            "import-condition-key-has",
            key = key.clone(),
            value = value.clone()
        ),
        Condition::FileName { text } => tr!("import-condition-name", text = text.clone()),
        Condition::Extension { exts } => tr!(
            "import-condition-ext",
            ext = exts
                .iter()
                .map(|e| e.trim().trim_start_matches('.'))
                .filter(|e| !e.is_empty())
                .collect::<Vec<_>>()
                .join(", .")
        ),
        Condition::FileStart { text } => tr!("import-condition-start", text = text.clone()),
        Condition::Origin { name } => tr!("import-condition-auto-origin", name = name.clone()),
        Condition::DataType { value } => {
            tr!("import-condition-auto-type", name = short_data_type(value))
        }
        Condition::Marker { text } => tr!("import-condition-auto-marker", marker = text.clone()),
        Condition::Numbers { sep, comma: true } => {
            tr!("import-condition-auto-numbers-comma", sep = sep_words(sep))
        }
        Condition::Numbers { sep, comma: false } => {
            tr!("import-condition-auto-numbers-point", sep = sep_words(sep))
        }
        Condition::Kinetic => tr!("import-condition-auto-kinetic"),
    }
}

/// The conditions the detector's reading of a file offers, in the order the
/// "Save as Template" window lists them: origin, data type, marker, numbers,
/// kinetic. One that the file does not give is not offered.
pub fn automatic_conditions(fp: &Fingerprint) -> Vec<Condition> {
    let facts = &fp.facts;
    let mut out = Vec::new();
    if !facts.origin.is_empty() {
        out.push(Condition::Origin {
            name: facts.origin.clone(),
        });
    }
    if !facts.data_type.is_empty() {
        out.push(Condition::DataType {
            value: facts.data_type.clone(),
        });
    }
    if !facts.marker.is_empty() {
        out.push(Condition::Marker {
            text: facts.marker.to_uppercase(),
        });
    }
    if !facts.sep.is_empty() {
        out.push(Condition::Numbers {
            sep: facts.sep.clone(),
            comma: facts.comma,
        });
    }
    if facts.kinetic {
        out.push(Condition::Kinetic);
    }
    out
}

// -- Rules of a template in plain sentences ------------------------------------

/// What a template sets, in plain sentences.
pub fn template_rules(r: &ImportRecipe) -> Vec<String> {
    let mut rules = Vec::new();
    let marker_text = r.main_until_marker.clone();
    if r.subheader_roles.x_prime.is_some() {
        match &marker_text {
            Some(m) => rules.push(tr!("import-template-rule-xprime", marker = m.clone())),
            None => rules.push(tr!("import-template-rule-xprime-plain")),
        }
    } else if let Some(m) = &marker_text {
        rules.push(tr!("import-template-rule-data-after", marker = m.clone()));
    }
    let unit_key = |target: PropTarget| {
        r.header_props
            .iter()
            .find(|p| {
                p.field == PropField::Units && (p.target == target || p.target == PropTarget::All)
            })
            .map(|p| p.key.clone())
    };
    if let (Some(x), Some(y)) = (unit_key(PropTarget::X), unit_key(PropTarget::Y)) {
        rules.push(tr!("import-template-rule-units", x = x, y = y));
    }
    let long_y = r
        .header_props
        .iter()
        .find(|p| p.field == PropField::LongName && p.target == PropTarget::Y);
    match (&r.name_source, long_y) {
        (NameSource::HeaderKey(k), None) => {
            rules.push(tr!("import-template-rule-name-sheet", key = k.clone()))
        }
        (_, Some(p)) => rules.push(tr!("import-template-rule-name-all", key = p.key.clone())),
        _ => {}
    }
    let decimal = if r.decimal == DecimalStyle::Comma {
        tr!("import-check-decimal-comma")
    } else {
        tr!("import-check-decimal-point")
    };
    let sep = match &r.separator {
        stokesforge_import::Separator::Semicolon => sep_name(SepKind::Semicolon),
        stokesforge_import::Separator::Tab => sep_name(SepKind::Tab),
        stokesforge_import::Separator::Comma => sep_name(SepKind::Comma),
        _ => sep_name(SepKind::Whitespace),
    };
    rules.push(tr!(
        "import-template-rule-numbers",
        decimal = decimal,
        sep = sep
    ));
    if let Some(m) = &marker_text {
        rules.push(tr!("import-template-rule-marker", marker = m.clone()));
    }
    rules
}

// -- Types ---------------------------------------------------------------------

/// A condition the detector offers, with its sentence and whether it is kept.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoCondition {
    pub condition: Condition,
    /// Checked: the template will require it.
    pub on: bool,
    pub sentence: String,
}

/// The "Save as Template" window being filled in.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveDraft {
    pub name: String,
    /// The detector's conditions, each one uncheckable.
    pub automatic: Vec<AutoCondition>,
    /// The conditions the user added.
    pub custom: Vec<Condition>,
    /// What the template sets, in plain sentences.
    pub rules: Vec<String>,
}

impl SaveDraft {
    /// The conditions the template will carry: the checked automatic ones,
    /// then the custom ones.
    pub fn conditions(&self) -> Vec<Condition> {
        self.automatic
            .iter()
            .filter(|c| c.on)
            .map(|c| c.condition.clone())
            .chain(self.custom.iter().cloned())
            .filter(|c| !c.is_empty())
            .collect()
    }

    /// Why "Save" is greyed out, if it is.
    pub fn issue(&self) -> Option<stokesforge_import::SaveIssue> {
        if self.name.trim().is_empty() {
            Some(stokesforge_import::SaveIssue::NoName)
        } else if self.conditions().is_empty() {
            Some(stokesforge_import::SaveIssue::NoCondition)
        } else {
            None
        }
    }
}

/// A template offered for the current file.
#[derive(Debug, Clone)]
pub struct Offer {
    pub name: String,
    pub display: String,
    pub rules: Vec<String>,
}

/// One condition of a saved template, with its sentence.
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionRow {
    pub condition: Condition,
    pub sentence: String,
}

/// A template as listed in the template menu and the manager.
#[derive(Debug, Clone)]
pub struct TemplateRow {
    pub name: String,
    pub rules: Vec<String>,
    /// The file shown is read with it.
    pub in_use: bool,
    pub conditions: Vec<ConditionRow>,
}

/// An answer to a doubt, remembered for a kind of file (shown in "Manage
/// Templates", with a "Forget" action).
#[derive(Debug, Clone, PartialEq)]
pub struct RememberedAnswer {
    /// Key of the kind of file (give it back to
    /// [`AppState::forget_remembered_answer`]).
    pub kind: String,
    /// Plain description of the kind of file.
    pub kind_text: String,
    /// Id of the doubt (give it back to forget).
    pub doubt: String,
    /// What the doubt was about, in a sentence.
    pub doubt_text: String,
}

/// The "Manage Templates" window.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ManageState {
    pub selected: Option<String>,
    /// The name being typed; applied on Enter.
    pub rename: String,
}

/// Plain sentence naming a doubt, by its id.
pub fn doubt_text(id: &str) -> String {
    match id {
        "check-separator" => tr!("import-doubt-name-check-separator"),
        "check-decimal" => tr!("import-doubt-name-check-decimal"),
        "check-encoding" => tr!("import-doubt-name-check-encoding"),
        "check-data" => tr!("import-doubt-name-check-data"),
        "check-points" => tr!("import-doubt-name-check-points"),
        "check-x-prime" => tr!("import-doubt-name-check-x-prime"),
        "check-names" => tr!("import-doubt-name-check-names"),
        "check-names-x-prime" => tr!("import-doubt-name-check-names-x-prime"),
        "check-x-unit" => tr!("import-doubt-name-check-x-unit"),
        "check-y-unit" => tr!("import-doubt-name-check-y-unit"),
        "check-x-prime-unit" => tr!("import-doubt-name-check-x-prime-unit"),
        "check-y-name" => tr!("import-doubt-name-check-y-name"),
        "check-sheet-name" => tr!("import-doubt-name-check-sheet-name"),
        "check-footer" => tr!("import-doubt-name-check-footer"),
        "check-comment" => tr!("import-doubt-name-check-comment"),
        "text-rows-in-data" => tr!("import-doubt-name-text-rows-in-data"),
        "several-blocks" => tr!("import-doubt-name-several-blocks"),
        "several-blocks-split" => tr!("import-doubt-name-several-blocks-split"),
        "decimal-or-separator" => tr!("import-doubt-name-decimal-or-separator"),
        "jcamp-npoints" => tr!("import-doubt-name-jcamp-npoints"),
        "jcamp-ntuples-not-read" => tr!("import-doubt-name-jcamp-ntuples-not-read"),
        "jcamp-unreadable-table" => tr!("import-doubt-name-jcamp-unreadable-table"),
        "jcamp-bad-lines" => tr!("import-doubt-name-jcamp-bad-lines"),
        "jcamp-dup-capped" => tr!("import-doubt-name-jcamp-dup-capped"),
        "jcamp-points-capped" => tr!("import-doubt-name-jcamp-points-capped"),
        "jcamp-nmr-left-out" => tr!("import-doubt-name-jcamp-nmr-left-out"),
        other => match other.strip_prefix(stokesforge_import::TIE_PREFIX) {
            Some(rest) => match rest.split_once(stokesforge_import::TIE_PICK_SEPARATOR) {
                Some((names, chosen)) => tr!(
                    "import-doubt-name-template-pick",
                    names = names.replace('|', ", "),
                    chosen = chosen
                ),
                None => tr!(
                    "import-doubt-name-template-tie",
                    names = rest.replace('|', ", ")
                ),
            },
            None => other.to_string(),
        },
    }
}

/// The name of an instrument or software.
fn instrument_name(instrument: stokesforge_import::instrument::Instrument) -> String {
    use stokesforge_import::instrument::Instrument as I;
    match instrument {
        I::Jasco => tr!("import-instrument-jasco"),
        I::Jcamp => tr!("import-instrument-jcamp"),
        I::JcampOcean => tr!("import-instrument-jcamp-ocean"),
        I::OceanOptics => tr!("import-instrument-ocean-optics"),
        I::Avantes => tr!("import-instrument-avantes"),
        I::Netzsch => tr!("import-instrument-netzsch"),
        I::PerkinElmerPyris => tr!("import-instrument-perkinelmer-pyris"),
        I::TaInstruments => tr!("import-instrument-ta-instruments"),
        I::Specs => tr!("import-instrument-specs"),
        I::Craic => tr!("import-instrument-craic"),
        I::PlateReader => tr!("import-instrument-plate-reader"),
    }
}

/// Plain description of a kind of file from its key
/// ([`stokesforge_import::kind_summary`]).
fn kind_text(kind: &str) -> String {
    use stokesforge_import::sniff::Family;
    let Some(summary) = stokesforge_import::kind_summary(kind) else {
        return tr!("import-kind-other");
    };
    let what = match (summary.instrument, summary.family) {
        (Some(instrument), _) => instrument_name(instrument),
        (None, Some(Family::Jasco)) => tr!("import-auto-family-jasco"),
        (None, Some(Family::Jcamp)) => tr!("import-auto-family-jcamp"),
        (None, Some(Family::PlainTable)) => tr!("import-auto-family-table"),
        (None, None) => tr!("import-auto-family-text"),
    };
    tr!(
        "import-kind-summary",
        what = what,
        sep = sep_words(summary.sep),
        decimal = if summary.comma { "," } else { "." }
    )
}

// -- The state ---------------------------------------------------------------

impl AppState {
    /// The offer to save a template, when the file has a recognised instrument,
    /// no template matches it, nothing is left to check and the offer was not
    /// declined.
    pub fn template_offer(&self) -> Option<Offer> {
        let state = self.import_state.as_ref()?;
        if state.checks.review.is_some() {
            return None;
        }
        let path = self.import_preview_path()?;
        let view = self.import_check()?;
        let fp = &view.fingerprint;
        if !fp.recognised
            || self.to_check_count() > 0
            || state.checks.offer_dismissed.contains(&fp.id)
            || state.checks.template_of_file.contains_key(&path)
            || state.store.matching(&fp.facts).is_some()
        {
            return None;
        }
        Some(Offer {
            name: fp.template_name.clone(),
            display: fp.facts.origin.clone(),
            rules: template_rules(&state.recipe),
        })
    }

    pub fn template_offer_dismiss(&mut self) {
        let Some(fp) = self.import_fingerprint() else {
            return;
        };
        if let Some(state) = self.import_state.as_mut() {
            state.checks.offer_dismissed.insert(fp.id.clone());
            state.checks.review_done = None;
        }
    }

    // -- Save as Template ------------------------------------------------------

    /// Opens "Save as Template" for the file shown: the proposed name and
    /// every condition of the detection, all checked.
    pub fn template_save_open(&mut self) {
        let Some(view) = self.import_check() else {
            return;
        };
        let Some(state) = self.import_state.as_mut() else {
            return;
        };
        let fp = &view.fingerprint;
        state.checks.save_draft = Some(SaveDraft {
            name: fp.template_name.clone(),
            automatic: automatic_conditions(fp)
                .into_iter()
                .map(|condition| AutoCondition {
                    sentence: condition_sentence(&condition),
                    condition,
                    on: true,
                })
                .collect(),
            custom: Vec::new(),
            rules: template_rules(&state.recipe),
        });
    }

    pub fn template_save_cancel(&mut self) {
        if let Some(state) = self.import_state.as_mut() {
            state.checks.save_draft = None;
        }
    }

    pub fn template_save_draft(&self) -> Option<&SaveDraft> {
        self.import_state.as_ref()?.checks.save_draft.as_ref()
    }

    pub fn template_save_draft_mut(&mut self) -> Option<&mut SaveDraft> {
        self.import_state.as_mut()?.checks.save_draft.as_mut()
    }

    /// Why "Save" is greyed out, if it is.
    pub fn template_save_issue(&self) -> Option<stokesforge_import::SaveIssue> {
        self.template_save_draft()?.issue()
    }

    /// Would "Save" replace a template of the same name?
    pub fn template_save_replaces(&self) -> bool {
        let Some(draft) = self.template_save_draft() else {
            return false;
        };
        self.import_state.as_ref().is_some_and(|s| {
            s.store
                .recipes()
                .iter()
                .any(|r| r.name == draft.name.trim())
        })
    }

    /// "Save": keeps the recipe and the conditions, and settles what was in
    /// doubt: a later file the template reads does not raise it again.
    pub fn template_save_commit(&mut self) {
        if self.template_save_issue().is_some() {
            return;
        }
        let Some((path, kind)) = self.check_kind_id() else {
            return;
        };
        let ids = self
            .to_check()
            .into_iter()
            .map(|p| p.id)
            .collect::<Vec<_>>();
        let Some(state) = self.import_state.as_mut() else {
            return;
        };
        let Some(draft) = state.checks.save_draft.take() else {
            return;
        };
        let name = draft.name.trim().to_string();
        let mut recipe = state.recipe.clone();
        recipe.name = name.clone();
        recipe.auto_match = Default::default();
        let conditions = draft.conditions();
        if let Err(issue) = state.store.upsert_with_conditions(recipe, conditions) {
            let message = crate::errors::import::save_issue_message(issue);
            self.fail(message);
            return;
        }
        for id in &ids {
            state.store.remember_answer(&kind, id);
        }
        let fingerprint_id = state
            .checks
            .cached_fingerprint_id(&path)
            .unwrap_or_default();
        state.checks.template_of_file.insert(path, name.clone());
        state.checks.offer_dismissed.insert(fingerprint_id);
        state.checks.review_done = None;
        state.checks.generation += 1;
        self.save_import_store();
        self.notice(tr!("status-template-saved", name = name));
    }

    // -- Manage Templates --------------------------------------------------------

    pub fn template_manage_open(&mut self) {
        let first = self
            .import_state
            .as_ref()
            .and_then(|s| s.store.recipes().first().map(|t| t.name.clone()));
        if let Some(state) = self.import_state.as_mut() {
            state.checks.manage = Some(ManageState {
                rename: first.clone().unwrap_or_default(),
                selected: first,
            });
        }
    }

    pub fn template_manage_close(&mut self) {
        if let Some(state) = self.import_state.as_mut() {
            state.checks.manage = None;
            state.checks.forget_pending = None;
        }
    }

    pub fn template_manage(&self) -> Option<&ManageState> {
        self.import_state.as_ref()?.checks.manage.as_ref()
    }

    pub fn template_manage_mut(&mut self) -> Option<&mut ManageState> {
        self.import_state.as_mut()?.checks.manage.as_mut()
    }

    pub fn template_manage_select(&mut self, name: &str) {
        if let Some(manage) = self.template_manage_mut() {
            manage.selected = Some(name.to_string());
            manage.rename = name.to_string();
        }
    }

    /// Renames a template (applied on Enter); an empty name or one that
    /// another template carries is refused. Its conditions follow.
    pub fn template_rename(&mut self, old: &str, new: &str) {
        let new = new.trim().to_string();
        let Some(state) = self.import_state.as_mut() else {
            return;
        };
        if new == old || !state.store.rename(old, &new) {
            return;
        }
        for name in state.checks.template_of_file.values_mut() {
            if name == old {
                *name = new.clone();
            }
        }
        if let Some(manage) = state.checks.manage.as_mut() {
            manage.selected = Some(new);
        }
        state.checks.generation += 1;
        self.save_import_store();
    }

    /// Replaces the conditions of a template, applied live. Refused when the
    /// template would meet no condition (it could never be chosen).
    pub fn template_set_conditions(&mut self, name: &str, conditions: Vec<Condition>) {
        let Some(state) = self.import_state.as_mut() else {
            return;
        };
        if conditions.iter().all(Condition::is_empty) || state.store.conditions(name) == conditions
        {
            return;
        }
        if !state.store.set_conditions(name, conditions) {
            return;
        }
        state.checks.generation += 1;
        self.save_import_store();
    }

    /// The automatic conditions of the file shown that a template does not
    /// carry yet (what "Add condition" can offer besides the typed ones).
    pub fn template_addable_conditions(&self, name: &str) -> Vec<AutoCondition> {
        let Some(state) = self.import_state.as_ref() else {
            return Vec::new();
        };
        let Some(view) = self.import_check() else {
            return Vec::new();
        };
        let have = state.store.conditions(name);
        automatic_conditions(&view.fingerprint)
            .into_iter()
            .filter(|c| !have.contains(c))
            .map(|condition| AutoCondition {
                sentence: condition_sentence(&condition),
                condition,
                on: false,
            })
            .collect()
    }

    pub fn template_forget_ask(&mut self, name: &str) {
        if let Some(state) = self.import_state.as_mut() {
            state.checks.forget_pending = Some(name.to_string());
        }
    }

    pub fn template_forget_pending(&self) -> Option<&str> {
        self.import_state.as_ref()?.checks.forget_pending.as_deref()
    }

    pub fn template_forget_cancel(&mut self) {
        if let Some(state) = self.import_state.as_mut() {
            state.checks.forget_pending = None;
        }
    }

    /// Confirmed: forgets the pending template and selects the next one.
    pub fn template_forget_confirm(&mut self) {
        let Some(name) = self.template_forget_pending().map(str::to_owned) else {
            return;
        };
        self.template_forget(&name);
        let next = self
            .import_state
            .as_ref()
            .and_then(|s| s.store.recipes().first().map(|t| t.name.clone()));
        if let Some(state) = self.import_state.as_mut() {
            state.checks.forget_pending = None;
            if let Some(manage) = state.checks.manage.as_mut() {
                manage.rename = next.clone().unwrap_or_default();
                manage.selected = next;
            }
        }
    }

    /// "Forget" a template: removed from the store, its files go back to Auto.
    pub fn template_forget(&mut self, name: &str) {
        let Some(state) = self.import_state.as_mut() else {
            return;
        };
        state.store.remove(name);
        state.checks.template_of_file.retain(|_, n| n != name);
        state.checks.generation += 1;
        self.save_import_store();
        self.notice(tr!("status-template-deleted", name = name));
    }

    /// Reads the file shown with a saved template.
    pub fn template_use(&mut self, name: &str) {
        let Some(path) = self.import_preview_path() else {
            return;
        };
        let bytes = self.import_bytes(&path).ok();
        let Some(state) = self.import_state.as_mut() else {
            return;
        };
        let Some(template) = state
            .store
            .recipes()
            .iter()
            .find(|r| r.name == name)
            .cloned()
        else {
            return;
        };
        // Choosing a template that fits as well as the one in use settles the
        // tie for this kind of file: later files of that kind use it.
        let tie_pick = state
            .checks
            .template_of_file
            .get(&path)
            .zip(state.checks.tied_of_file.get(&path))
            .filter(|(chosen, others)| *chosen != name && others.iter().any(|o| o == name))
            .and_then(|(chosen, others)| {
                let kind = state
                    .auto
                    .sniffs
                    .get(&path)?
                    .as_ref()
                    .ok()
                    .map(stokesforge_import::kind_id)?;
                let tie = super::import_tocheck::tie_id(chosen, others);
                Some((kind, tie))
            });
        if let Some((kind, tie)) = &tie_pick {
            state.store.remember_tie_pick(kind, tie, name);
        }
        let mut recipe = template;
        recipe.unify_header();
        if let Some(bytes) = &bytes {
            if let Some(main) = stokesforge_import::sniff::marker_main_lines(bytes, &recipe) {
                recipe.header_layout.main_header_lines = main;
            }
        }
        state.recipe = recipe;
        state.seen_recipe = state.recipe.clone();
        state.checks.auto_paths.remove(&path);
        state.checks.tied_of_file.remove(&path);
        state.checks.template_of_file.insert(path, name.to_string());
        state.checks.generation += 1;
        if tie_pick.is_some() {
            self.save_import_store();
        }
    }

    /// "Auto": forgets the template of the file shown, detects again.
    pub fn template_use_auto(&mut self) {
        let Some(path) = self.import_preview_path() else {
            return;
        };
        if let Some(state) = self.import_state.as_mut() {
            state.checks.template_of_file.remove(&path);
            state.checks.tied_of_file.remove(&path);
            state.checks.auto_paths.insert(path);
            state.checks.generation += 1;
            state.checks.review = None;
            state.checks.review_done = None;
        }
        self.import_auto_redetect();
    }

    /// The saved templates, for the template menu and the manager.
    pub fn template_rows(&self) -> Vec<TemplateRow> {
        let Some(state) = self.import_state.as_ref() else {
            return Vec::new();
        };
        let in_use = self
            .import_preview_path()
            .and_then(|p| state.checks.template_of_file.get(&p).cloned());
        state
            .store
            .recipes()
            .iter()
            .map(|t| TemplateRow {
                name: t.name.clone(),
                rules: template_rules(t),
                in_use: in_use.as_deref() == Some(t.name.as_str()),
                conditions: state
                    .store
                    .conditions(&t.name)
                    .iter()
                    .map(|c| ConditionRow {
                        condition: c.clone(),
                        sentence: condition_sentence(c),
                    })
                    .collect(),
            })
            .collect()
    }

    /// Name of the template the file shown is read with.
    pub fn template_in_use(&self) -> Option<String> {
        let state = self.import_state.as_ref()?;
        let path = self.import_preview_path()?;
        state.checks.template_of_file.get(&path).cloned()
    }

    // -- Remembered answers --------------------------------------------------

    /// Every answer to a doubt remembered in the store, per kind of file.
    pub fn remembered_answers(&self) -> Vec<RememberedAnswer> {
        let Some(state) = self.import_state.as_ref() else {
            return Vec::new();
        };
        state
            .store
            .remembered_answers()
            .into_iter()
            .map(|(kind, doubt)| RememberedAnswer {
                kind: kind.to_string(),
                kind_text: kind_text(kind),
                doubt: doubt.to_string(),
                doubt_text: doubt_text(doubt),
            })
            .collect()
    }

    /// "Forget" one remembered answer: the doubt comes back for that kind.
    pub fn forget_remembered_answer(&mut self, kind: &str, doubt: &str) {
        let Some(state) = self.import_state.as_mut() else {
            return;
        };
        if state.store.forget_answer(kind, doubt) {
            state.checks.generation += 1;
            self.save_import_store();
        }
    }
}
