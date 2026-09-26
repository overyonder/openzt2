//! Discovery and precedence indexing of all behavior documents in the active archive overlay.

use std::{collections::BTreeMap, path::Path};

use bevy::prelude::*;
use openzt2_game_data::{behavior::document::BehaviorDocument, AssetId};

use super::behavior_asset_types::{
    BehaviorDeclarationLocation, BehaviorDocumentAsset, LoadedBehaviorDocumentCollection,
};
use crate::asset_source::AssetArchives;

pub(super) fn refresh_behavior_documents_after_archive_revision_change(
    behavior_source_archives: Res<AssetArchives>,
    asset_server: Res<AssetServer>,
    mut loaded_behavior_documents: ResMut<LoadedBehaviorDocumentCollection>,
) {
    let current_archive_revision = behavior_source_archives.revision();
    if loaded_behavior_documents.archive_revision == Some(current_archive_revision) {
        return;
    }
    loaded_behavior_documents.behavior_document_assets = behavior_source_archives
        .resolved_paths()
        .1
        .iter()
        .cloned()
        .filter(|asset_path| is_behavior_document_asset_path(asset_path))
        .map(|asset_path| asset_server.load::<BehaviorDocumentAsset>(asset_path))
        .collect();
    loaded_behavior_documents.archive_revision = Some(current_archive_revision);
    loaded_behavior_documents.indexed_archive_revision = None;
}

fn is_behavior_document_asset_path(asset_path: &Path) -> bool {
    asset_path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("beh") || extension.eq_ignore_ascii_case("tsk")
    })
}

pub(super) fn rebuild_behavior_declaration_indexes_after_asset_change(
    asset_server: Res<AssetServer>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    mut behavior_asset_events: MessageReader<AssetEvent<BehaviorDocumentAsset>>,
    mut loaded_behavior_documents: ResMut<LoadedBehaviorDocumentCollection>,
) {
    let any_behavior_asset_changed = behavior_asset_events.read().fold(false, |changed, event| {
        changed | !matches!(event, AssetEvent::Unused { .. })
    });
    if any_behavior_asset_changed {
        loaded_behavior_documents.indexed_archive_revision = None;
    }
    if loaded_behavior_documents.indexed_archive_revision
        == loaded_behavior_documents.archive_revision
        && !any_behavior_asset_changed
    {
        return;
    }
    if loaded_behavior_documents
        .behavior_document_assets
        .iter()
        .any(|handle| {
            !matches!(
                asset_server.get_load_state(handle.id()),
                Some(bevy::asset::LoadState::Loaded | bevy::asset::LoadState::Failed(_))
            )
        })
    {
        return;
    }
    let mut behavior_sets_by_identifier =
        BTreeMap::<AssetId, Vec<BehaviorDeclarationLocation>>::new();
    let mut behavior_tasks_by_unique_identifier =
        BTreeMap::<AssetId, Vec<BehaviorDeclarationLocation>>::new();
    for behavior_document_handle in &loaded_behavior_documents.behavior_document_assets {
        let Some(behavior_document_asset) = behavior_document_assets.get(behavior_document_handle)
        else {
            continue;
        };
        match &behavior_document_asset.behavior_document {
            BehaviorDocument::Sets(behavior_set_declarations) => {
                behavior_set_declarations.iter().enumerate().for_each(
                    |(declaration_index, behavior_set)| {
                        let declaration_location = BehaviorDeclarationLocation {
                            behavior_document_asset: behavior_document_handle.clone(),
                            declaration_index,
                        };
                        behavior_sets_by_identifier
                            .entry(behavior_set.id)
                            .or_default()
                            .push(declaration_location.clone());
                        behavior_sets_by_identifier
                            .entry(AssetId::from_key(behavior_set.name.trim()))
                            .or_default()
                            .push(declaration_location.clone());
                        behavior_sets_by_identifier
                            .entry(AssetId::from_key(
                                &behavior_set.name.trim().to_ascii_lowercase(),
                            ))
                            .or_default()
                            .push(declaration_location);
                    },
                );
            }
            BehaviorDocument::Tasks(behavior_tasks) => {
                behavior_tasks
                    .iter()
                    .enumerate()
                    .for_each(|(declaration_index, behavior_task)| {
                        behavior_tasks_by_unique_identifier
                            .entry(AssetId::from_key(
                                &behavior_task.name.trim().to_ascii_lowercase(),
                            ))
                            .or_default()
                            .push(BehaviorDeclarationLocation {
                                behavior_document_asset: behavior_document_handle.clone(),
                                declaration_index,
                            });
                    });
            }
        }
    }
    behavior_sets_by_identifier
        .values_mut()
        .for_each(remove_duplicate_behavior_declaration_locations);
    loaded_behavior_documents.behavior_sets_by_identifier = behavior_sets_by_identifier;
    behavior_tasks_by_unique_identifier
        .values_mut()
        .for_each(remove_duplicate_behavior_declaration_locations);
    loaded_behavior_documents.behavior_tasks_by_unique_identifier =
        behavior_tasks_by_unique_identifier;
    loaded_behavior_documents.indexed_archive_revision = loaded_behavior_documents.archive_revision;
}

fn remove_duplicate_behavior_declaration_locations(
    declaration_locations: &mut Vec<BehaviorDeclarationLocation>,
) {
    declaration_locations.dedup_by(|left_location, right_location| {
        left_location.behavior_document_asset.id() == right_location.behavior_document_asset.id()
            && left_location.declaration_index == right_location.declaration_index
    });
}
