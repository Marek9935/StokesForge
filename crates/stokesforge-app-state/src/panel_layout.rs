//! Memory of the layout of the three fixed panels (#148): Project Manager,
//! Object Manager and Script Console. Pure data and pure decisions, no egui
//! (#263): `stokesforge-app` reads the live window geometry and applies the result.
//!
//! What is kept, per panel, in `preferences.json` › `panels`:
//! collapsed or expanded, detached into its own OS window or not, and the size
//! and position of that window. Together with the **number of screens** at the
//! time of the save: a detached panel is put back only if the machine has the
//! same number of screens as when it was saved, otherwise it comes back attached
//! (a window left on a screen that is gone would be lost). The collapsed state is
//! always restored.
//!
//! Wayland cannot read or set the position of a window (winit): the position is
//! then absent from the file, and the panel is re-detached at its saved size,
//! the compositor placing it.
//!
//! The file is read entry by entry (like the dialog geometries): a damaged key
//! falls back to the default of that field alone, never of its neighbours.

/// One of the three fixed panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Project,
    Object,
    Script,
}

impl PanelKind {
    pub const ALL: [Self; 3] = [Self::Project, Self::Object, Self::Script];

    /// Key of the panel in the `panels` object of the preferences file.
    pub fn key(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Object => "object",
            Self::Script => "script",
        }
    }
}

/// What is kept of one panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelPrefs {
    /// Folded to its title bar.
    pub collapsed: bool,
    /// Lives in its own OS window.
    pub detached: bool,
    /// Size of the detached window, in points.
    pub size: Option<[f32; 2]>,
    /// Position of the detached window (top-left corner, in points); absent
    /// where the system cannot tell (Wayland).
    pub position: Option<[f32; 2]>,
}

impl PanelPrefs {
    /// The state of a panel at the first launch: expanded, attached; the Script
    /// Console starts collapsed.
    pub fn default_for(kind: PanelKind) -> Self {
        Self {
            collapsed: kind == PanelKind::Script,
            detached: false,
            size: None,
            position: None,
        }
    }
}

/// The layout of the three panels and the screen count it was saved with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelLayout {
    /// Number of screens when it was saved; `None` if unknown (then nothing is
    /// ever re-detached).
    pub monitors: Option<u32>,
    pub project: PanelPrefs,
    pub object: PanelPrefs,
    pub script: PanelPrefs,
}

impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            monitors: None,
            project: PanelPrefs::default_for(PanelKind::Project),
            object: PanelPrefs::default_for(PanelKind::Object),
            script: PanelPrefs::default_for(PanelKind::Script),
        }
    }
}

impl PanelLayout {
    pub fn panel(&self, kind: PanelKind) -> &PanelPrefs {
        match kind {
            PanelKind::Project => &self.project,
            PanelKind::Object => &self.object,
            PanelKind::Script => &self.script,
        }
    }

    pub fn panel_mut(&mut self, kind: PanelKind) -> &mut PanelPrefs {
        match kind {
            PanelKind::Project => &mut self.project,
            PanelKind::Object => &mut self.object,
            PanelKind::Script => &mut self.script,
        }
    }

    /// The `panels` value written to the preferences file.
    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(monitors) = self.monitors {
            map.insert("monitors".into(), monitors.into());
        }
        for kind in PanelKind::ALL {
            let panel = self.panel(kind);
            let mut entry = serde_json::Map::new();
            entry.insert("collapsed".into(), panel.collapsed.into());
            entry.insert("detached".into(), panel.detached.into());
            if let Some(size) = panel.size {
                entry.insert("size".into(), serde_json::json!(size));
            }
            if let Some(position) = panel.position {
                entry.insert("position".into(), serde_json::json!(position));
            }
            map.insert(kind.key().into(), entry.into());
        }
        map.into()
    }

    /// Reads the `panels` value, entry by entry. Anything missing or damaged
    /// takes its own default (a panel as it is at the first launch).
    pub fn from_json(value: Option<&serde_json::Value>) -> Self {
        let mut layout = Self::default();
        let Some(serde_json::Value::Object(map)) = value else {
            return layout;
        };
        layout.monitors = map
            .get("monitors")
            .and_then(serde_json::Value::as_u64)
            .and_then(|count| u32::try_from(count).ok())
            .filter(|count| *count > 0);
        for kind in PanelKind::ALL {
            let Some(serde_json::Value::Object(entry)) = map.get(kind.key()) else {
                continue;
            };
            let panel = layout.panel_mut(kind);
            if let Some(collapsed) = entry.get("collapsed").and_then(serde_json::Value::as_bool) {
                panel.collapsed = collapsed;
            }
            if let Some(detached) = entry.get("detached").and_then(serde_json::Value::as_bool) {
                panel.detached = detached;
            }
            panel.size = entry.get("size").and_then(pair).filter(valid_size);
            panel.position = entry.get("position").and_then(pair);
        }
        layout
    }
}

/// `[a, b]` of two finite numbers.
fn pair(value: &serde_json::Value) -> Option<[f32; 2]> {
    let [a, b] = value.as_array()?.as_slice() else {
        return None;
    };
    let (a, b) = (a.as_f64()? as f32, b.as_f64()? as f32);
    (a.is_finite() && b.is_finite()).then_some([a, b])
}

/// A size a window can have: both sides positive (and finite).
fn valid_size(size: &[f32; 2]) -> bool {
    size.iter().all(|side| side.is_finite() && *side > 0.0)
}

/// How a panel starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StartupPanel {
    pub collapsed: bool,
    /// `Some`: open the panel's OS window at once, with this size and, if known,
    /// this position. `None`: the panel starts attached.
    pub detach: Option<DetachAt>,
}

/// Where to open a re-detached window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetachAt {
    pub size: [f32; 2],
    pub position: Option<[f32; 2]>,
}

/// The decision for one panel: its collapsed state is always restored; it is
/// re-detached only if it was detached, has a usable size, and the machine has
/// the same (known) number of screens as at the save.
pub fn startup_panel(
    saved: &PanelPrefs,
    saved_monitors: Option<u32>,
    current_monitors: Option<u32>,
) -> StartupPanel {
    let same_screens = saved_monitors.is_some() && saved_monitors == current_monitors;
    let detach = match saved.size {
        Some(size) if saved.detached && same_screens && valid_size(&size) => Some(DetachAt {
            size,
            position: saved.position.filter(|p| p.iter().all(|v| v.is_finite())),
        }),
        _ => None,
    };
    StartupPanel {
        collapsed: saved.collapsed,
        detach,
    }
}

/// The three decisions at once.
pub fn startup_panels(saved: &PanelLayout, current_monitors: Option<u32>) -> [StartupPanel; 3] {
    PanelKind::ALL.map(|kind| startup_panel(saved.panel(kind), saved.monitors, current_monitors))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detached(size: [f32; 2], position: Option<[f32; 2]>) -> PanelPrefs {
        PanelPrefs {
            collapsed: false,
            detached: true,
            size: Some(size),
            position,
        }
    }

    fn layout(monitors: u32) -> PanelLayout {
        PanelLayout {
            monitors: Some(monitors),
            object: detached([330.0, 480.0], Some([1500.0, 40.0])),
            script: PanelPrefs {
                collapsed: false,
                ..PanelPrefs::default_for(PanelKind::Script)
            },
            ..PanelLayout::default()
        }
    }

    #[test]
    fn a_missing_key_gives_the_defaults() {
        let layout = PanelLayout::from_json(None);
        assert_eq!(layout, PanelLayout::default());
        assert!(!layout.project.collapsed && !layout.object.collapsed);
        assert!(layout.script.collapsed, "the console starts collapsed");
        assert!(!layout.project.detached && !layout.object.detached && !layout.script.detached);
        assert_eq!(PanelLayout::from_json(Some(&serde_json::json!(7))), layout);
    }

    #[test]
    fn a_layout_round_trips() {
        let saved = layout(2);
        assert_eq!(PanelLayout::from_json(Some(&saved.to_json())), saved);
    }

    /// A garbled key falls back to its own default and spares the others.
    #[test]
    fn a_damaged_key_only_loses_itself() {
        let value = serde_json::json!({
            "monitors": 2,
            "project": {"collapsed": "yes", "detached": true, "size": [300.0, 400.0]},
            "object": {"collapsed": true, "detached": 3, "size": [0.0, 400.0], "position": [1, "x"]},
            "script": 42,
        });
        let layout = PanelLayout::from_json(Some(&value));
        assert_eq!(layout.monitors, Some(2));
        assert!(!layout.project.collapsed, "garbled: the default");
        assert!(layout.project.detached, "its neighbour is kept");
        assert_eq!(layout.project.size, Some([300.0, 400.0]));
        assert!(layout.object.collapsed, "kept next to garbled neighbours");
        assert!(!layout.object.detached);
        assert_eq!(layout.object.size, None, "a zero size is unusable");
        assert_eq!(layout.object.position, None);
        assert_eq!(layout.script, PanelPrefs::default_for(PanelKind::Script));
    }

    #[test]
    fn an_unusable_screen_count_is_unknown() {
        for bad in [
            serde_json::json!(0),
            serde_json::json!(-1),
            serde_json::json!("2"),
        ] {
            let layout = PanelLayout::from_json(Some(&serde_json::json!({ "monitors": bad })));
            assert_eq!(layout.monitors, None);
        }
    }

    #[test]
    fn the_same_screens_redetach_at_the_saved_place() {
        let [project, object, script] = startup_panels(&layout(2), Some(2));
        assert_eq!(
            object.detach,
            Some(DetachAt {
                size: [330.0, 480.0],
                position: Some([1500.0, 40.0])
            })
        );
        assert!(project.detach.is_none(), "it was not detached");
        assert!(!script.collapsed, "the collapsed state is restored");
    }

    #[test]
    fn another_screen_count_reattaches_but_keeps_the_collapsed_state() {
        let mut saved = layout(2);
        saved.object.collapsed = true;
        let [_, object, script] = startup_panels(&saved, Some(1));
        assert!(object.detach.is_none());
        assert!(object.collapsed, "collapsed is kept");
        assert!(!script.collapsed);
    }

    #[test]
    fn an_unknown_screen_count_reattaches() {
        assert!(startup_panels(&layout(2), None)[1].detach.is_none());
        let mut saved = layout(2);
        saved.monitors = None;
        assert!(startup_panels(&saved, Some(2))[1].detach.is_none());
    }

    /// Wayland: no position in the file, same screens: detached at the saved
    /// size, the compositor places the window.
    #[test]
    fn without_a_position_the_window_is_detached_at_its_size_alone() {
        let mut saved = layout(2);
        saved.object.position = None;
        let [_, object, _] = startup_panels(&saved, Some(2));
        assert_eq!(
            object.detach,
            Some(DetachAt {
                size: [330.0, 480.0],
                position: None
            })
        );
    }

    #[test]
    fn an_unusable_size_reattaches() {
        for size in [
            [0.0, 480.0],
            [330.0, f32::NAN],
            [-1.0, 5.0],
            [f32::INFINITY, 5.0],
        ] {
            let mut saved = layout(2);
            saved.object.size = Some(size);
            assert!(
                startup_panels(&saved, Some(2))[1].detach.is_none(),
                "{size:?}"
            );
        }
        let mut saved = layout(2);
        saved.object.size = None;
        assert!(startup_panels(&saved, Some(2))[1].detach.is_none());
    }
}
