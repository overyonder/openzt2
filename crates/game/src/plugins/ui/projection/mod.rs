pub(crate) mod authored_document_projection;
mod authored_hotkey_projection;
mod authored_node_action_routing_projection;
mod authored_node_layout_projection;
mod authored_node_picking_projection;
mod authored_node_property_binding_projection;
mod authored_node_visual_projection;
mod authored_text_projection;
mod authored_widget_projection;

use authored_document_projection::project_document;

use bevy::{asset::LoadState, prelude::*};

use super::authored_image_selection_diagnostic_override::UiAuthoredImageSelectionDiagnosticOverride;
use super::authored_reusable_list_and_table_runtime_types::{
    SetUiListRowCount, UiListPolicy, UiListRow,
};
use super::authored_ui_node_projection_components::UiDocumentRoot;
use super::ui_document_asset_load_failure::UiDocumentAssetLoadFailed;
use super::ui_document_lifecycle_contracts::{
    HideUiDocument, ShowUiDocument, ShowUiRole, UiRoleRequests,
};
use crate::assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::{
        ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
        ui_document_role_asset_path_selection::select_bevy_asset_path_for_canonical_ui_document_role,
    },
};

#[derive(Component)]
pub(super) struct PendingUiProjection {
    document: Handle<UiDocumentAsset>,
    owner: Entity,
}

#[derive(Component)]
pub(super) struct PendingUiListRow {
    document: Handle<UiDocumentAsset>,
    list: Entity,
    index: u16,
}

pub(super) fn resolve_ui_roles(
    mut requests: MessageReader<ShowUiRole>,
    asset_server: Res<AssetServer>,
    roots: Query<(Entity, &ChildOf, &UiDocumentRoot)>,
    children: Query<&Children>,
    mut visibility: Query<&mut Visibility>,
    pending: Query<&PendingUiProjection>,
    mut show: MessageWriter<ShowUiDocument>,
    mut deferred: ResMut<UiRoleRequests>,
) {
    requests
        .read()
        .copied()
        .for_each(|request| deferred.request(request.role, request.owner));
    let mut requested = Vec::new();
    for request in deferred.take() {
        let Some(document_asset_path) =
            select_bevy_asset_path_for_canonical_ui_document_role(request.role)
        else {
            deferred.retain(request);
            continue;
        };
        let document = asset_server.load(document_asset_path);
        if let Some((root_entity, _, _)) = roots
            .iter()
            .find(|(_, parent, root)| parent.parent() == request.owner && root.document == document)
        {
            if let Ok(mut root_visibility) = visibility.get_mut(root_entity) {
                *root_visibility = Visibility::Inherited;
            }
            if let Some(entry) = children
                .get(root_entity)
                .ok()
                .and_then(|children| children.first())
            {
                if let Ok(mut entry_visibility) = visibility.get_mut(*entry) {
                    *entry_visibility = Visibility::Inherited;
                }
            }
            continue;
        }
        if pending
            .iter()
            .any(|pending| pending.owner == request.owner && pending.document == document)
            || requested
                .iter()
                .any(|(owner, handle)| *owner == request.owner && *handle == document)
        {
            continue;
        }
        trace!(role = ?request.role, "requesting UI role asset");
        show.write(ShowUiDocument {
            document: document.clone(),
            owner: request.owner,
        });
        requested.push((request.owner, document));
    }
}

pub(super) fn project_ui_documents(
    mut commands: Commands,
    mut requests: MessageReader<ShowUiDocument>,
    pending: Query<(Entity, &PendingUiProjection)>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    asset_server: Res<AssetServer>,
    image_selection: Option<Res<UiAuthoredImageSelectionDiagnosticOverride>>,
    owners: Query<()>,
    mut failures: MessageWriter<UiDocumentAssetLoadFailed>,
) {
    let image_selection_index = image_selection
        .as_deref()
        .map(UiAuthoredImageSelectionDiagnosticOverride::candidate_index);
    for (request_entity, request) in &pending {
        if owners.get(request.owner).is_err() {
            commands.entity(request_entity).despawn();
            continue;
        }
        let Some(document) = documents.get(&request.document) else {
            if matches!(
                asset_server.load_state(request.document.id()),
                LoadState::Failed(_)
            ) {
                failures.write(UiDocumentAssetLoadFailed {
                    owner: request.owner,
                });
                commands.entity(request_entity).despawn();
            }
            continue;
        };
        let Some(localization) =
            active_localization.borrow_loaded_localization_view(&localizations)
        else {
            continue;
        };
        trace!(
            nodes = &document.canonical_ui_document().nodes.len(),
            "projecting pending UI document"
        );
        project_document(
            &mut commands,
            &ShowUiDocument {
                document: request.document.clone(),
                owner: request.owner,
            },
            document,
            localization,
            image_selection_index,
            true,
        );
        commands.entity(request_entity).despawn();
    }
    for request in requests.read() {
        if owners.get(request.owner).is_err() {
            continue;
        }
        let Some(document) = documents.get(&request.document) else {
            if matches!(
                asset_server.load_state(request.document.id()),
                LoadState::Failed(_)
            ) {
                failures.write(UiDocumentAssetLoadFailed {
                    owner: request.owner,
                });
                continue;
            }
            commands.spawn((
                PendingUiProjection {
                    document: request.document.clone(),
                    owner: request.owner,
                },
                ChildOf(request.owner),
            ));
            continue;
        };
        let Some(localization) =
            active_localization.borrow_loaded_localization_view(&localizations)
        else {
            commands.spawn((
                PendingUiProjection {
                    document: request.document.clone(),
                    owner: request.owner,
                },
                ChildOf(request.owner),
            ));
            continue;
        };
        trace!(
            nodes = &document.canonical_ui_document().nodes.len(),
            "projecting UI document"
        );
        project_document(
            &mut commands,
            request,
            document,
            localization,
            image_selection_index,
            true,
        );
    }
}

/// Adds or removes list rows to match the requested count, waiting for their assets.
pub(super) fn reconcile_ui_list_rows(
    mut commands: Commands,
    mut requests: MessageReader<SetUiListRowCount>,
    lists: Query<&UiListPolicy>,
    rows: Query<(Entity, &UiListRow)>,
    pending: Query<(Entity, &PendingUiListRow)>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    image_selection: Option<Res<UiAuthoredImageSelectionDiagnosticOverride>>,
) {
    let image_selection_index = image_selection
        .as_deref()
        .map(UiAuthoredImageSelectionDiagnosticOverride::candidate_index);
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };

    // Several authored activation paths can size the same list in one frame.
    // Only the last request is observable, and it must be known before a
    // pending row from an earlier request is projected into the live tree.
    let mut final_requested_row_counts = Vec::<(Entity, u16)>::new();
    for request in requests.read() {
        let count = request.count.min(256);
        if let Some((_, current_count)) = final_requested_row_counts
            .iter_mut()
            .find(|(list, _)| *list == request.list)
        {
            *current_count = count;
        } else {
            final_requested_row_counts.push((request.list, count));
        }
    }

    for pending_row in &pending {
        let (pending_entity, pending_row) = pending_row;
        if final_requested_row_counts
            .iter()
            .find(|(list, _)| *list == pending_row.list)
            .is_some_and(|(_, count)| pending_row.index >= *count)
        {
            commands.entity(pending_entity).despawn();
            continue;
        }
        let Some(document) = documents.get(&pending_row.document) else {
            continue;
        };
        debug!(
            list = ?pending_row.list,
            row = pending_row.index,
            "projecting authored UI list row"
        );
        let root = project_document(
            &mut commands,
            &ShowUiDocument {
                document: pending_row.document.clone(),
                owner: pending_row.list,
            },
            document,
            localization,
            image_selection_index,
            false,
        );
        commands.entity(root).insert(UiListRow {
            list: pending_row.list,
            index: pending_row.index,
        });
        commands.entity(pending_entity).despawn();
    }

    for (list, count) in final_requested_row_counts {
        let Ok(policy) = lists.get(list) else {
            continue;
        };
        rows.iter()
            .filter(|(_, row)| row.list == list && row.index >= count)
            .for_each(|(entity, _)| commands.entity(entity).despawn());
        pending
            .iter()
            .filter(|(_, row)| row.list == list && row.index >= count)
            .for_each(|(entity, _)| commands.entity(entity).despawn());

        let Some(document) = policy.row_document.as_ref() else {
            debug!(?list, count, "UI list has no authored row document");
            continue;
        };
        for index in 0..count {
            let exists = rows
                .iter()
                .any(|(_, row)| row.list == list && row.index == index)
                || pending
                    .iter()
                    .any(|(_, row)| row.list == list && row.index == index);
            if !exists {
                debug!(
                    ?list,
                    row = index,
                    document = ?document.id(),
                    "requesting authored UI list row"
                );
                commands.spawn((
                    PendingUiListRow {
                        document: document.clone(),
                        list,
                        index,
                    },
                    ChildOf(list),
                ));
            }
        }
    }
}

pub(super) fn hide_ui_documents(
    mut commands: Commands,
    mut requests: MessageReader<HideUiDocument>,
    roots: Query<(Entity, &ChildOf), With<UiDocumentRoot>>,
) {
    for request in requests.read() {
        for (root, parent) in &roots {
            if parent.parent() == request.owner {
                commands.entity(root).despawn();
            }
        }
    }
}

/// Removes a projected document whose lifecycle owner disappeared before the
/// deferred hierarchy attachment was applied.
///
/// UI requests and domain-owned lifecycle changes can be emitted by unrelated
/// systems in the same schedule. Attaching through a checked world command
/// keeps that race harmless; this system then reclaims the unattached tree.
pub(super) fn cleanup_orphaned_ui_documents(
    mut commands: Commands,
    roots: Query<Entity, (With<UiDocumentRoot>, Without<ChildOf>)>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_removes_the_complete_owned_tree() {
        let mut app = App::new();
        app.add_message::<HideUiDocument>()
            .add_systems(Update, hide_ui_documents);
        let owner = app.world_mut().spawn_empty().id();
        let root = app
            .world_mut()
            .spawn((
                UiDocumentRoot {
                    document: Handle::default(),
                },
                ChildOf(owner),
            ))
            .id();
        let child = app.world_mut().spawn(ChildOf(root)).id();
        app.world_mut().write_message(HideUiDocument { owner });

        app.update();

        assert!(!app.world().entities().contains(root));
        assert!(!app.world().entities().contains(child));
        assert!(app.world().entities().contains(owner));
    }
}
