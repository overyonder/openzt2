use bevy::prelude::*;
use openzt2_game_data::ui_document::document::UiDocumentRole;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;

use super::{
    zoopedia_hierarchy_operations::{find_root_zoopedia_entry, resolve_zoopedia_entry_subject},
    zoopedia_navigation_types::{PendingZoopediaEntry, ZoopediaHistory, ZoopediaPage},
};

pub(crate) fn hydrate_zoopedia_pages_from_authored_root_entry(
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    ui_document_roots: Query<(Entity, &UiDocumentRoot, &ChildOf), Without<ZoopediaPage>>,
    pending_entries: Query<&PendingZoopediaEntry>,
    mut commands: Commands,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(root_subject) =
        find_root_zoopedia_entry(world_definitions).map(resolve_zoopedia_entry_subject)
    else {
        return;
    };

    for (document_root_entity, document_root, lifecycle) in &ui_document_roots {
        let Some(document) = ui_document_assets.get(&document_root.document) else {
            continue;
        };
        if matches!(
            &document.canonical_ui_document().role,
            UiDocumentRole::Zoopedia
        ) {
            let owner = lifecycle.parent();
            let subject = pending_entries.get(owner).map_or(root_subject, |entry| {
                commands.entity(owner).remove::<PendingZoopediaEntry>();
                entry.0
            });
            commands.entity(document_root_entity).insert((
                ZoopediaPage {
                    subject,
                    section: 0,
                },
                ZoopediaHistory::from_transition(root_subject, subject),
            ));
        }
    }
}
