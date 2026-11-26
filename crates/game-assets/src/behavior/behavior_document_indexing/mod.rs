//! Discovery and precedence indexing of behavior documents.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use bevy::{
    asset::{AssetId as DocumentId, AssetLoadFailedEvent, LoadState},
    prelude::*,
};
use openzt2_game_data::{behavior::document::BehaviorDocument, AssetId};

use super::behavior_asset_types::{
    BehaviorDeclarationLocation, BehaviorDeclarationRows, BehaviorDocumentAsset,
    IndexedBehaviorDocument, LoadedBehaviorDocumentCollection,
};
use crate::asset_source::{ArchiveOverlayChanged, AssetArchives};

pub(super) fn discover_behavior_documents(
    archives: Res<AssetArchives>,
    asset_server: Res<AssetServer>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    mut overlay_changes: MessageReader<ArchiveOverlayChanged>,
    mut collection: ResMut<LoadedBehaviorDocumentCollection>,
) {
    let changed_paths: BTreeSet<&Path> = overlay_changes
        .read()
        .flat_map(|message| &message.changes)
        .map(|change| change.normalized_asset_path.as_path())
        .filter(|path| is_behavior_document_asset_path(path))
        .collect();
    if !archives.is_added() && changed_paths.is_empty() {
        return;
    }

    let index = collection.bypass_change_detection();
    let mut published_rows_changed = false;
    let initial = archives
        .is_added()
        .then(|| archives.resolved_archive_entries());
    let updated = if initial.is_some() {
        Vec::new()
    } else {
        changed_paths
            .iter()
            .filter_map(|path| {
                archives
                    .resolved_winning_entry(path)
                    .filter(|entry| entry.normalized_asset_path == *path)
            })
            .collect::<Vec<_>>()
    };
    for entry in initial
        .iter()
        .flat_map(|entries| entries.iter())
        .chain(&updated)
    {
        let path = &entry.normalized_asset_path;
        if !is_behavior_document_asset_path(path) {
            continue;
        }
        let handle = asset_server.load::<BehaviorDocumentAsset>(path.clone());
        let id = handle.id();
        let precedence = (entry.archive_priority_index, entry.zip_entry_ordinal);
        if let Some(document) = index.documents.get_mut(&id) {
            if changed_paths.contains(path.as_path()) {
                index.pending.insert(id);
            }
            if document.precedence != precedence {
                let rows = std::mem::take(&mut document.rows);
                let previous_precedence = document.precedence;
                remove_rows(index, id, previous_precedence, &rows);
                let Some(document) = index.documents.get_mut(&id) else {
                    continue;
                };
                document.precedence = precedence;
                published_rows_changed |= replace_rows(index, id, rows);
            }
            continue;
        }
        index.documents.insert(
            id,
            IndexedBehaviorDocument {
                handle,
                precedence,
                rows: BehaviorDeclarationRows::default(),
            },
        );
        match asset_server.get_load_state(id) {
            Some(LoadState::Loaded) => {
                if let Some(asset) = assets.get(id) {
                    published_rows_changed |= replace_rows(index, id, declaration_rows(asset));
                }
            }
            Some(LoadState::Failed(_)) => {}
            _ => {
                index.pending.insert(id);
            }
        }
    }
    for path in changed_paths {
        if archives
            .resolved_winning_entry(path)
            .is_some_and(|entry| entry.normalized_asset_path == path)
        {
            continue;
        }
        let Some(handle) = asset_server.get_handle::<BehaviorDocumentAsset>(path.to_path_buf())
        else {
            continue;
        };
        let id = handle.id();
        if let Some(document) = index.documents.remove(&id) {
            published_rows_changed |= remove_rows(index, id, document.precedence, &document.rows);
            index.pending.remove(&id);
        }
    }
    publish_readiness(&mut collection, published_rows_changed);
}

fn is_behavior_document_asset_path(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("beh") || extension.eq_ignore_ascii_case("tsk")
    })
}

pub(super) fn update_behavior_declarations(
    assets: Res<Assets<BehaviorDocumentAsset>>,
    mut asset_events: MessageReader<AssetEvent<BehaviorDocumentAsset>>,
    mut failures: MessageReader<AssetLoadFailedEvent<BehaviorDocumentAsset>>,
    mut collection: ResMut<LoadedBehaviorDocumentCollection>,
) {
    let index = collection.bypass_change_detection();
    let mut changed = BTreeSet::new();
    for event in asset_events.read() {
        match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id }
                if index.documents.contains_key(&id) =>
            {
                changed.insert(id);
            }
            _ => {}
        }
    }
    let mut published_rows_changed = false;
    for failure in failures.read() {
        let id = failure.id;
        if index.documents.contains_key(&id) && !changed.contains(&id) {
            index.pending.remove(&id);
            published_rows_changed |= replace_rows(index, id, BehaviorDeclarationRows::default());
        }
    }
    for id in changed {
        index.pending.remove(&id);
        let rows = assets
            .get(id)
            .map_or_else(BehaviorDeclarationRows::default, declaration_rows);
        published_rows_changed |= replace_rows(index, id, rows);
    }
    publish_readiness(&mut collection, published_rows_changed);
}

fn publish_readiness(
    collection: &mut ResMut<LoadedBehaviorDocumentCollection>,
    rows_changed: bool,
) {
    let ready = collection.pending.is_empty();
    if collection.index_ready != ready {
        collection.index_ready = ready;
    } else if ready && rows_changed {
        collection.set_changed();
    }
}

fn declaration_rows(asset: &BehaviorDocumentAsset) -> BehaviorDeclarationRows {
    let mut rows = BehaviorDeclarationRows::default();
    match &asset.behavior_document {
        BehaviorDocument::Sets(sets) => {
            for (index, set) in sets.iter().enumerate() {
                let name = set.name.trim();
                rows.sets.extend([
                    (set.id, index),
                    (AssetId::from_key(name), index),
                    (AssetId::from_key(&name.to_ascii_lowercase()), index),
                ]);
            }
            rows.sets.sort_unstable();
            rows.sets.dedup();
        }
        BehaviorDocument::Tasks(tasks) => {
            rows.tasks
                .extend(tasks.iter().enumerate().map(|(index, task)| {
                    (
                        AssetId::from_key(&task.name.trim().to_ascii_lowercase()),
                        index,
                    )
                }));
        }
    }
    rows
}

fn replace_rows(
    collection: &mut LoadedBehaviorDocumentCollection,
    id: DocumentId<BehaviorDocumentAsset>,
    rows: BehaviorDeclarationRows,
) -> bool {
    let Some(document) = collection.documents.get_mut(&id) else {
        return false;
    };
    if document.rows == rows {
        return false;
    }
    let handle = document.handle.clone();
    let precedence = document.precedence;
    let previous = std::mem::replace(&mut document.rows, rows);
    remove_rows(collection, id, precedence, &previous);
    let document = &collection.documents[&id];
    insert_rows(
        &collection.documents,
        &mut collection.behavior_sets_by_identifier,
        &document.rows.sets,
        &handle,
        document.precedence,
    );
    for &(key, declaration) in &document.rows.tasks {
        collection
            .behavior_task_order
            .insert((precedence, declaration), (key, id));
    }
    insert_rows(
        &collection.documents,
        &mut collection.behavior_tasks_by_unique_identifier,
        &document.rows.tasks,
        &handle,
        document.precedence,
    );
    true
}

fn insert_rows(
    documents: &BTreeMap<DocumentId<BehaviorDocumentAsset>, IndexedBehaviorDocument>,
    index: &mut BTreeMap<AssetId, Vec<BehaviorDeclarationLocation>>,
    rows: &[(AssetId, usize)],
    handle: &Handle<BehaviorDocumentAsset>,
    precedence: (usize, usize),
) {
    for &(key, declaration_index) in rows {
        let locations = index.entry(key).or_default();
        let order = (precedence, declaration_index);
        let position = locations.partition_point(|location| {
            (
                documents[&location.behavior_document_asset.id()].precedence,
                location.declaration_index,
            ) < order
        });
        locations.insert(
            position,
            BehaviorDeclarationLocation {
                behavior_document_asset: handle.clone(),
                declaration_index,
            },
        );
    }
}

fn remove_rows(
    collection: &mut LoadedBehaviorDocumentCollection,
    id: DocumentId<BehaviorDocumentAsset>,
    precedence: (usize, usize),
    rows: &BehaviorDeclarationRows,
) -> bool {
    fn remove_locations(
        index: &mut BTreeMap<AssetId, Vec<BehaviorDeclarationLocation>>,
        rows: &[(AssetId, usize)],
        id: DocumentId<BehaviorDocumentAsset>,
    ) {
        for (key, _) in rows {
            if let std::collections::btree_map::Entry::Occupied(mut entry) = index.entry(*key) {
                entry
                    .get_mut()
                    .retain(|location| location.behavior_document_asset.id() != id);
                if entry.get().is_empty() {
                    entry.remove();
                }
            }
        }
    }
    remove_locations(&mut collection.behavior_sets_by_identifier, &rows.sets, id);
    remove_locations(
        &mut collection.behavior_tasks_by_unique_identifier,
        &rows.tasks,
        id,
    );
    for &(_, declaration) in &rows.tasks {
        collection
            .behavior_task_order
            .remove(&(precedence, declaration));
    }
    !(rows.sets.is_empty() && rows.tasks.is_empty())
}
