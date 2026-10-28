//! "Send to Graph": send the selected columns to an open graph, docked or
//! detached (#165).
//!
//! Dragging columns (#160) does not cross from one system window to another
//! under Wayland (the source window keeps the pointer while the button is
//! held). Instead of a real system drag and drop, the right-click on a
//! selection of whole columns or rows (letters, row numbers, boxes) and the
//! Workbook menu offer, next to "Plot", **each open graph**: the same
//! [`Action::AddSeriesToGraph`] as the drop. With no open graph, the entry is
//! absent (grammar in `docs/menus.md`).
//!
//! The list of open graphs belongs to `StokesForgeApp`. It is published in the
//! `Context` on every frame ([`publish`]), the only channel shared by the
//! viewports and their duplicated states, like the saved colors.

use eframe::egui;
use stokesforge_core::{NodeId, Tree};

use super::graph_window::published_active_panel;
use super::tab::{tab_title_from_tree, Tab};
use stokesforge_app_state::state::{
    drop_payload_action, drop_rows_action, Action, ColumnDragPayload, RowDragPayload,
};
use stokesforge_app_state::tr;

/// The open graphs, in order: docked first, then detached.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct OpenGraphs(Vec<NodeId>);

fn open_graphs_id() -> egui::Id {
    egui::Id::new("stokesforge_open_graphs_for_send_to_graph")
}

/// Publishes the open graphs: called once per frame by
/// `StokesForgeApp::ui`.
pub(super) fn publish<'a>(
    ctx: &egui::Context,
    floating: &[Tab],
    detached: impl Iterator<Item = &'a Tab>,
) {
    let mut graphs: Vec<NodeId> = floating
        .iter()
        .filter_map(|tab| match tab {
            Tab::Graph(graph) => Some(*graph),
            _ => None,
        })
        .collect();
    let mut detached: Vec<NodeId> = detached
        .filter_map(|tab| match tab {
            Tab::Graph(graph) => Some(*graph),
            _ => None,
        })
        .collect();
    // `detached` comes from a `HashMap`: a stable order keeps the menu from
    // changing from one frame to the next.
    detached.sort();
    graphs.extend(detached);
    ctx.data_mut(|data| data.insert_temp(open_graphs_id(), OpenGraphs(graphs)));
}

/// The open graphs that still exist in the tree.
fn targets(ctx: &egui::Context, tree: &Tree) -> Vec<NodeId> {
    let open: OpenGraphs = ctx
        .data(|data| data.get_temp(open_graphs_id()))
        .unwrap_or_default();
    open.0
        .into_iter()
        .filter(|graph| tree.node(*graph).is_some())
        .collect()
}

/// The "Send to" entry (or submenu, with three graphs or more), absent
/// without a target.
pub(super) fn send_to_graph_menu(
    ui: &mut egui::Ui,
    tree: &Tree,
    actions: &mut Vec<Action>,
    sheet: NodeId,
    columns: &[NodeId],
) {
    if columns.is_empty() {
        return;
    }
    let targets = targets(ui.ctx(), tree);
    if targets.is_empty() {
        return;
    }
    let payload = ColumnDragPayload {
        sheet,
        columns: columns.to_vec(),
    };
    let mut entry = |ui: &mut egui::Ui, graph: NodeId| {
        let panel = published_active_panel(ui.ctx(), graph);
        let name = tab_title_from_tree(tree, Tab::Graph(graph));
        if ui
            .button(tr!("workbook-send-to-graph", graph = name))
            .clicked()
        {
            actions.push(drop_payload_action(&payload, graph, panel));
            ui.close();
        }
    };
    if targets.len() < 3 {
        for graph in targets {
            entry(ui, graph);
        }
    } else {
        ui.menu_button(tr!("workbook-send-to-graph-menu"), |ui| {
            for graph in targets {
                let name = tab_title_from_tree(tree, Tab::Graph(graph));
                if ui.button(name).clicked() {
                    let panel = published_active_panel(ui.ctx(), graph);
                    actions.push(drop_payload_action(&payload, graph, panel));
                    ui.close();
                }
            }
        });
    }
}

/// "Send to" for **rows** (#166): the same list of open graphs, but the
/// action plots the rows ([`drop_rows_action`], which asks the axis question
/// if the graph already has an X).
pub(super) fn send_rows_to_graph_menu(
    ui: &mut egui::Ui,
    tree: &Tree,
    actions: &mut Vec<Action>,
    payload: &RowDragPayload,
) {
    let targets = targets(ui.ctx(), tree);
    if targets.is_empty() {
        return;
    }
    let mut send = |ui: &mut egui::Ui, graph: NodeId, label: String| {
        if ui.button(label).clicked() {
            let panel = published_active_panel(ui.ctx(), graph);
            actions.push(drop_rows_action(payload, graph, panel));
            ui.close();
        }
    };
    if targets.len() < 3 {
        for graph in targets {
            let name = tab_title_from_tree(tree, Tab::Graph(graph));
            send(ui, graph, tr!("workbook-send-to-graph", graph = name));
        }
    } else {
        ui.menu_button(tr!("workbook-send-to-graph-menu"), |ui| {
            for graph in targets {
                send(ui, graph, tab_title_from_tree(tree, Tab::Graph(graph)));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_core::Project;
    use stokesforge_plot::PlotSpec;

    /// Every open graph is a target, docked first then detached. A workbook or
    /// a graph gone from the tree is not one, and with no open graph there is
    /// no target.
    #[test]
    fn every_open_graph_is_a_target() {
        let mut project = Project::new("Targets").unwrap();
        let root = project.root();
        let docked = project.add_graph(root, None, PlotSpec::empty("A")).unwrap();
        let detached = project.add_graph(root, None, PlotSpec::empty("B")).unwrap();
        let book = project.add_empty_workbook(root, None).unwrap();
        let ctx = egui::Context::default();
        let tree = project.tree();

        publish(&ctx, &[Tab::Book(book)], [].iter());
        assert!(targets(&ctx, tree).is_empty());

        publish(
            &ctx,
            &[Tab::Graph(docked), Tab::Book(book)],
            [Tab::Graph(detached), Tab::Book(book)].iter(),
        );
        assert_eq!(targets(&ctx, tree), vec![docked, detached]);

        // A graph deleted from the tree stays open for a moment in the
        // published list: it is not offered.
        publish(&ctx, &[Tab::Graph(NodeId::from_raw(9999))], [].iter());
        assert!(targets(&ctx, tree).is_empty());
    }
}
