//! « Envoyer vers un graphique » : envoyer les colonnes sélectionnées vers un
//! graphique ouvert, attaché ou détaché (#165).
//!
//! Le glisser de colonnes (#160) ne passe pas d'une fenêtre système à l'autre
//! sous Wayland (la fenêtre source garde le pointeur tant que le bouton est
//! pris) : plutôt qu'un vrai glisser-déposer du système, le clic droit sur une
//! sélection de colonnes ou de lignes entières — lettres, numéros de ligne,
//! cases — et le menu Classeur proposent, à côté de « Plot », **chaque
//! graphique ouvert** : la même [`Action::AddSeriesToGraph`] que le dépôt.
//! Sans graphique ouvert, l'entrée est absente (grammaire de `docs/menus.md`).
//!
//! La liste des graphiques ouverts appartient à `StokesForgeApp` ; elle est
//! publiée dans le `Context` à chaque image ([`publish`]), seul canal commun
//! aux viewports et à leurs états dupliqués, comme les couleurs enregistrées.

use eframe::egui;
use stokesforge_core::{NodeId, Tree};

use super::graph_window::published_active_panel;
use super::tab::{tab_title_from_tree, Tab};
use crate::state::{
    drop_payload_action, drop_rows_action, Action, ColumnDragPayload, RowDragPayload,
};
use crate::tr;

/// Les graphiques ouverts, dans l'ordre : attachés puis détachés.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct OpenGraphs(Vec<NodeId>);

fn open_graphs_id() -> egui::Id {
    egui::Id::new("stokesforge_open_graphs_for_send_to_graph")
}

/// Publie les graphiques ouverts : appelée une fois par image par
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
    // `detached` vient d'une `HashMap` : un ordre stable évite un menu qui
    // change d'une image à l'autre.
    detached.sort();
    graphs.extend(detached);
    ctx.data_mut(|data| data.insert_temp(open_graphs_id(), OpenGraphs(graphs)));
}

/// Les graphiques ouverts qui existent encore dans l'arbre.
fn targets(ctx: &egui::Context, tree: &Tree) -> Vec<NodeId> {
    let open: OpenGraphs = ctx
        .data(|data| data.get_temp(open_graphs_id()))
        .unwrap_or_default();
    open.0
        .into_iter()
        .filter(|graph| tree.node(*graph).is_some())
        .collect()
}

/// L'entrée (ou le sous-menu, à trois graphiques ou plus) « Envoyer vers »,
/// absente sans cible.
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

/// « Envoyer vers » pour des **lignes** (#166) : la même liste de graphiques
/// ouverts, mais l'action trace les lignes ([`drop_rows_action`], qui pose la
/// question de l'axe si le graphique a déjà un X).
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

    /// Tous les graphiques ouverts sont des cibles, attachés d'abord puis
    /// détachés ; un classeur ou un graphique disparu de l'arbre n'en est pas
    /// une, et sans graphique ouvert il n'y a pas de cible.
    #[test]
    fn tous_les_graphiques_ouverts_sont_des_cibles() {
        let mut project = Project::new("Cibles").unwrap();
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

        // Un graphique supprimé de l'arbre reste ouvert un instant dans la
        // liste publiée : il n'est pas proposé.
        publish(&ctx, &[Tab::Graph(NodeId::from_raw(9999))], [].iter());
        assert!(targets(&ctx, tree).is_empty());
    }
}
