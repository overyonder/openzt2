use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use bevy::prelude::*;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::information::catalogue_types::SelectedCatalogueEntry;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;

use super::research_types::{
    ResearchProjectAvailability, StartResearchProjectRequest, WorldResearchItems,
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiResearchActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn route_authored_research_ui_actions_to_start_requests(
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    action_nodes: Query<(&UiResearchActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    selected: Res<SelectedCatalogueEntry>,
    research: Query<&ResearchProjectAvailability>,
    world_items: Query<&WorldResearchItems>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut requests: MessageWriter<StartResearchProjectRequest>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(items) = world_items.iter().next() else {
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
            if let Some(item) = selected
                .0
                .and_then(|selected| definitions.research_for_definition(selected))
                .and_then(|id| items.definitions.get(&id).copied())
                .filter(|&entity| research.get(entity).is_ok_and(|item| item.available))
            {
                requests.write(StartResearchProjectRequest { item });
            }
        }
    }
}
