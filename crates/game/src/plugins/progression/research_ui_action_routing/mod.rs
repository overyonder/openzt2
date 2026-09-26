use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use bevy::prelude::*;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        information::catalogue_types::SelectedCatalogueEntry,
        ui::{
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
        },
    },
};

use super::research_types::{ResearchProjectAvailability, StartResearchProjectRequest};
use crate::plugins::ui::authored_ui_action_projection_components::UiResearchActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn route_authored_research_ui_actions_to_start_requests(
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    action_nodes: Query<(&UiResearchActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    selected: Res<SelectedCatalogueEntry>,
    research: Query<&ResearchProjectAvailability>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut requests: MessageWriter<StartResearchProjectRequest>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for activation in activations.read() {
        let Ok((range, owner)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        for record in range.authored_action_records(document) {
            if activation.trigger != record.trigger {
                continue;
            }
            if let Some(definition) = selected
                .0
                .and_then(|selected| {
                    definitions.research().find(|project| {
                        project.id == selected || project.unlocks.contains(&selected)
                    })
                })
                .and_then(|project| research.iter().find(|item| item.item == project.id))
                .filter(|item| item.available)
                .map(|item| item.item)
            {
                requests.write(StartResearchProjectRequest { definition });
            }
        }
    }
}
