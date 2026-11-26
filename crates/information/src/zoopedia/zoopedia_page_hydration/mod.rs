use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    action::{
        information::{UiInformationAction, UiInformationActionRecord},
        UiActionRecord, UiTrigger,
    },
    document::UiDocumentRole,
};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;

use super::{
    zoopedia_hierarchy_operations::resolve_zoopedia_entry_subject,
    zoopedia_navigation_types::{
        PendingZoopediaEntry, ZoopediaHistory, ZoopediaPage, ZoopediaRootEntry,
    },
};

pub(crate) fn hydrate_zoopedia_pages_from_authored_root_entry(
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut ui_document_roots: Query<(
        Entity,
        Ref<UiDocumentRoot>,
        &ChildOf,
        Has<ZoopediaPage>,
        Option<&mut ZoopediaRootEntry>,
    )>,
    pending_entries: Query<&PendingZoopediaEntry>,
    mut commands: Commands,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (document_root_entity, document_root, lifecycle, has_page, current_root) in
        &mut ui_document_roots
    {
        if has_page && !document_root.is_changed() && !ui_document_assets.is_changed() {
            continue;
        }
        let Some(document) = ui_document_assets.get(&document_root.document) else {
            continue;
        };
        let document = document.canonical_ui_document();
        if document.role != UiDocumentRole::Zoopedia {
            if current_root.is_some() {
                commands
                    .entity(document_root_entity)
                    .remove::<ZoopediaRootEntry>();
            }
            continue;
        }
        let Some(entry) = document
            .nodes
            .iter()
            .find(|node| node.name.eq_ignore_ascii_case("help home"))
            .and_then(|home| {
                home.actions.iter().find_map(|action| match action {
                    UiActionRecord::Information(UiInformationActionRecord {
                        trigger: UiTrigger::Press,
                        action: UiInformationAction::OpenEncyclopediaEntry { entry },
                    }) => Some(*entry),
                    _ => None,
                })
            })
        else {
            if current_root.is_some() {
                commands
                    .entity(document_root_entity)
                    .remove::<ZoopediaRootEntry>();
            }
            continue;
        };
        if let Some(mut current_root) = current_root {
            current_root.set_if_neq(ZoopediaRootEntry(entry));
        } else {
            commands
                .entity(document_root_entity)
                .insert(ZoopediaRootEntry(entry));
        }
        if has_page {
            continue;
        }
        let Some(root_subject) = world_definitions
            .find_zoopedia_entry(entry)
            .map(resolve_zoopedia_entry_subject)
        else {
            continue;
        };
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
