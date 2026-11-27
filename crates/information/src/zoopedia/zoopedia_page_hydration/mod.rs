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
    mut document_events: Option<MessageReader<AssetEvent<UiDocumentAsset>>>,
    mut ui_document_roots: Query<(
        Entity,
        Ref<UiDocumentRoot>,
        &ChildOf,
        Option<&mut ZoopediaPage>,
        Option<&mut ZoopediaHistory>,
        Option<&mut ZoopediaRootEntry>,
    )>,
    pending_entries: Query<&PendingZoopediaEntry>,
    mut commands: Commands,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let all_documents_changed = document_events.is_none() && ui_document_assets.is_changed();
    let changed_documents: bevy::platform::collections::HashSet<_> = document_events
        .as_mut()
        .map(|events| {
            events
                .read()
                .filter_map(|event| match *event {
                    AssetEvent::Added { id }
                    | AssetEvent::Modified { id }
                    | AssetEvent::Removed { id } => Some(id),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    for (document_root_entity, document_root, lifecycle, page, mut history, current_root) in
        &mut ui_document_roots
    {
        if page.is_some()
            && !document_root.is_changed()
            && !all_documents_changed
            && !changed_documents.contains(&document_root.document.id())
        {
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
        let previous = current_root.as_deref().map(|root| root.0);
        if let Some(mut current_root) = current_root {
            current_root.set_if_neq(ZoopediaRootEntry(entry));
        } else {
            commands
                .entity(document_root_entity)
                .insert(ZoopediaRootEntry(entry));
        }
        let Some(root_subject) = world_definitions
            .find_zoopedia_entry(entry)
            .map(resolve_zoopedia_entry_subject)
        else {
            continue;
        };
        if let Some(mut page) = page {
            if let Some(previous) = previous.filter(|&previous| previous != entry) {
                let old_root = world_definitions
                    .find_zoopedia_entry(previous)
                    .map_or(previous, resolve_zoopedia_entry_subject);
                if page.subject == old_root {
                    page.set_if_neq(ZoopediaPage {
                        subject: root_subject,
                        section: 0,
                    });
                }
                if let Some(history) = history.as_deref_mut() {
                    history.replace_subject(old_root, root_subject);
                }
            }
            continue;
        }
        let owner = lifecycle.parent();
        let subject = pending_entries.get(owner).map_or(root_subject, |entry| {
            commands.entity(owner).remove::<PendingZoopediaEntry>();
            entry.0
        });
        let mut entity = commands.entity(document_root_entity);
        entity.insert(ZoopediaPage {
            subject,
            section: 0,
        });
        let next_history = ZoopediaHistory::from_transition(root_subject, subject);
        if let Some(mut current) = history {
            current.set_if_neq(next_history);
        } else {
            entity.insert(next_history);
        }
    }
}
