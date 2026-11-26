use bevy::prelude::*;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::action::information::{
    InformationGraphSeries, InformationGraphType, UiInformationAction,
};

use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;

use openzt2_interface_state::information::information_graph_types::InformationGraph;

pub(super) fn apply_authored_information_graph_action(
    action: &UiInformationAction,
    document: &UiDocumentAsset,
    document_owner_entity: Entity,
    activated_node_entity: Entity,
    information_graphs: &mut Query<(&UiDocumentOwner, &mut InformationGraph)>,
    commands: &mut Commands,
) {
    match action {
        UiInformationAction::SelectGraph {
            graph,
            label_document,
        } => {
            set_information_graph_series_for_document(
                document_owner_entity,
                activated_node_entity,
                Some((
                    *graph,
                    document.nested_ui_document_handle(*label_document).cloned(),
                )),
                None,
                information_graphs,
                commands,
            );
        }
        UiInformationAction::SelectGraphType { graph_type } => {
            set_information_graph_series_for_document(
                document_owner_entity,
                activated_node_entity,
                None,
                Some(*graph_type),
                information_graphs,
                commands,
            );
        }
        _ => unreachable!("non-graph action sent to the information graph action owner"),
    }
}

fn set_information_graph_series_for_document(
    document_owner_entity: Entity,
    activated_node_entity: Entity,
    selected_graph_series: Option<(InformationGraphSeries, Option<Handle<UiDocumentAsset>>)>,
    selected_graph_type: Option<InformationGraphType>,
    information_graphs: &mut Query<(&UiDocumentOwner, &mut InformationGraph)>,
    commands: &mut Commands,
) {
    let mut existing_information_graph_was_updated = false;

    for (candidate_document_owner, mut information_graph) in information_graphs.iter_mut() {
        if candidate_document_owner.0 != document_owner_entity {
            continue;
        }

        if let Some((series, label)) = &selected_graph_series {
            information_graph
                .reborrow()
                .map_unchanged(|graph| &mut graph.graph)
                .set_if_neq(Some(*series));
            information_graph
                .reborrow()
                .map_unchanged(|graph| &mut graph.label_document)
                .set_if_neq(label.clone());
        }
        if let Some(selected_graph_type) = selected_graph_type {
            information_graph
                .map_unchanged(|graph| &mut graph.graph_type)
                .set_if_neq(Some(selected_graph_type));
        }
        existing_information_graph_was_updated = true;
    }

    if !existing_information_graph_was_updated {
        let (graph, label_document) =
            selected_graph_series.map_or((None, None), |(series, label)| (Some(series), label));
        commands
            .entity(activated_node_entity)
            .insert(InformationGraph {
                graph,
                label_document,
                graph_type: selected_graph_type,
            });
    }
}
