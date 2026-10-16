pub mod authored_document_projection;
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
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;

use super::authored_image_selection_diagnostic_override::UiAuthoredImageSelectionDiagnosticOverride;
use super::authored_reusable_list_and_table_runtime_types::{
    SetUiListRowCount, UiListPolicy, UiListRow,
};
use super::authored_ui_node_projection_components::UiDocumentRoot;
use super::ui_document_asset_load_failure::UiDocumentAssetLoadFailed;
use super::ui_document_lifecycle_contracts::{
    HideUiDocument, ShowUiDocument, ShowUiRole, UiRoleRequests,
};
use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::ui_document::ui_document_role_asset_path_selection::select_bevy_asset_path_for_canonical_ui_document_role;

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

pub(super) fn cancel_pending_ui_role_projection(
    world: &mut World,
    owner: Entity,
    role: openzt2_game_data::ui_document::document::UiDocumentRole,
) {
    super::cross_document_node_visibility::cancel_pending_cross_document_visibility(
        world,
        owner,
        Some(role),
    );
    let Some(path) = select_bevy_asset_path_for_canonical_ui_document_role(role) else {
        return;
    };
    let path = bevy::asset::AssetPath::parse(&path);
    let pending: Vec<_> = world
        .query::<(Entity, &PendingUiProjection)>()
        .iter(world)
        .filter_map(|(entity, pending)| {
            (pending.owner == owner && pending.document.path() == Some(&path)).then_some(entity)
        })
        .collect();
    for entity in pending {
        world.entity_mut(entity).despawn();
    }
}

pub(super) fn resolve_ui_roles(
    mut commands: Commands,
    mut requests: MessageReader<ShowUiRole>,
    asset_server: Res<AssetServer>,
    roots: Query<(Entity, &ChildOf, &UiDocumentRoot)>,
    children: Query<&Children>,
    mut visibility: Query<&mut Visibility>,
    pending: Query<(Entity, &PendingUiProjection)>,
    owners: Query<()>,
    mut show: MessageWriter<ShowUiDocument>,
    mut deferred: ResMut<UiRoleRequests>,
) {
    requests
        .read()
        .copied()
        .for_each(|request| deferred.request(request.role, request.owner));
    let mut requested = Vec::new();
    for pending_role in deferred.take() {
        let request = pending_role.request;
        if !owners.contains(request.owner) {
            continue;
        }
        let Some(document_asset_path) =
            select_bevy_asset_path_for_canonical_ui_document_role(request.role)
        else {
            deferred.retain(pending_role);
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
        if let Some((entity, _)) = pending
            .iter()
            .find(|(_, pending)| pending.owner == request.owner && pending.document == document)
        {
            if pending_role.preserve_temporary_pause {
                commands.entity(entity).insert(openzt2_world_state::simulation_time::simulation_control_types::AuthoredUiTemporarySimulationPause::holding());
            }
            continue;
        }
        if requested
            .iter()
            .any(|(owner, handle)| *owner == request.owner && *handle == document)
        {
            continue;
        }
        if pending_role.preserve_temporary_pause {
            commands.spawn((
                PendingUiProjection { document: document.clone(), owner: request.owner },
                ChildOf(request.owner),
                openzt2_world_state::simulation_time::simulation_control_types::AuthoredUiTemporarySimulationPause::holding(),
            ));
            requested.push((request.owner, document));
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
    mut cancellations: MessageReader<HideUiDocument>,
    pending: Query<(Entity, &PendingUiProjection)>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    asset_server: Res<AssetServer>,
    image_selection: Option<Res<UiAuthoredImageSelectionDiagnosticOverride>>,
    owners: Query<()>,
    mut failures: MessageWriter<UiDocumentAssetLoadFailed>,
) {
    let cancelled_owners: Vec<_> = cancellations.read().map(|request| request.owner).collect();
    let image_selection_index = image_selection
        .as_deref()
        .map(UiAuthoredImageSelectionDiagnosticOverride::candidate_index);
    for (request_entity, request) in &pending {
        if cancelled_owners.contains(&request.owner) || owners.get(request.owner).is_err() {
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
                    document: request.document.clone(),
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
        if cancelled_owners.contains(&request.owner) || owners.get(request.owner).is_err() {
            continue;
        }
        let Some(document) = documents.get(&request.document) else {
            if matches!(
                asset_server.load_state(request.document.id()),
                LoadState::Failed(_)
            ) {
                failures.write(UiDocumentAssetLoadFailed {
                    owner: request.owner,
                    document: request.document.clone(),
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
        if lists
            .get(pending_row.list)
            .is_ok_and(|policy| policy.source == UiWidgetLiveCollectionSource::ScenarioObjectives)
        {
            // `ZTGoalPanel` places each objective's status list box directly in
            // the panel, and each authors `walign="max"`: the row and its list
            // boxes span the panel, and the label (`x="30" walign="max"`) runs
            // to the panel's right edge. Without a definite row width the
            // label's grid column collapses to its min-content and wraps one
            // character per line. The row stacks its status list boxes and
            // stretches them across its width, as their authored region does.
            commands
                .entity(root)
                .entry::<Node>()
                .and_modify(|mut node| {
                    node.width = percent(100.0);
                    node.display = Display::Flex;
                    node.flex_direction = FlexDirection::Column;
                    node.align_items = AlignItems::Stretch;
                });
            commands
                .entity(root)
                .entry::<super::authored_ui_layout_participation::UiAuthoredLayoutDisplay>()
                .and_modify(|mut authored| authored.set_authored_display(Display::Flex));
        }
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
    pending: Query<(Entity, &PendingUiProjection)>,
    mut roles: ResMut<UiRoleRequests>,
) {
    for request in requests.read() {
        roles.cancel(request.owner, None);
        let owner = request.owner;
        commands.queue(move |world: &mut World| {
            super::cross_document_node_visibility::cancel_pending_cross_document_visibility(
                world, owner, None,
            );
        });
        for (entity, pending) in &pending {
            if pending.owner == request.owner {
                commands.entity(entity).despawn();
            }
        }
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
            .init_resource::<UiRoleRequests>()
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
