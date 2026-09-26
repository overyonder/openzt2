use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::{
    InformationGraphSeries, InformationGraphType, UiInformationAction,
};

use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;

use super::information_graph_types::InformationGraph;

pub(super) fn apply_authored_information_graph_action(
    action: &UiInformationAction,
    document_owner_entity: Entity,
    activated_node_entity: Entity,
    information_graphs: &mut Query<(&UiDocumentOwner, &mut InformationGraph)>,
    commands: &mut Commands,
) {
    match action {
        UiInformationAction::SelectGraph { graph } => {
            set_information_graph_series_for_document(
                document_owner_entity,
                activated_node_entity,
                Some(*graph),
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
    selected_graph_series: Option<InformationGraphSeries>,
    selected_graph_type: Option<InformationGraphType>,
    information_graphs: &mut Query<(&UiDocumentOwner, &mut InformationGraph)>,
    commands: &mut Commands,
) {
    let mut existing_information_graph_was_updated = false;

    for (candidate_document_owner, mut information_graph) in information_graphs.iter_mut() {
        if candidate_document_owner.0 != document_owner_entity {
            continue;
        }

        if let Some(selected_graph_series) = selected_graph_series {
            information_graph.graph = Some(selected_graph_series);
        }
        if let Some(selected_graph_type) = selected_graph_type {
            information_graph.graph_type = Some(selected_graph_type);
        }
        existing_information_graph_was_updated = true;
    }

    if !existing_information_graph_was_updated {
        commands
            .entity(activated_node_entity)
            .insert(InformationGraph {
                graph: selected_graph_series,
                graph_type: selected_graph_type,
            });
    }
}
