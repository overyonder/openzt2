use std::collections::{HashMap, HashSet};

use bevy::{asset::LoadState, prelude::*};
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::{
        ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
        ui_document_role_asset_path_selection::select_bevy_asset_path_for_canonical_ui_document_role,
    },
};
use openzt2_game_data::{ui_document::action::UiTrigger, AssetId};
use openzt2_interface_state::{
    input::input_types::ActionSource,
    ui::{
        authored_ui_activation_contracts::UiNodeActivated,
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId},
        authored_ui_selection_state::UiSelected,
        ui_document_asset_load_failure::UiDocumentAssetLoadFailed,
        ui_document_lifecycle_contracts::{
            AuthoredUiDocumentOperation, HideUiDocument, ShowUiDocument, ShowUiRole,
            UiDocumentOperation,
        },
    },
};
use openzt2_world_state::simulation_time::simulation_control_types::UiDocumentPauseTransfer;

use super::authored_document_projection::project_document_on_entity;
use crate::{
    animation::UiShowHideAnimation,
    authored_image_selection_diagnostic_override::UiAuthoredImageSelectionDiagnosticOverride,
};

#[derive(Component)]
pub(crate) struct PendingUiProjection(Handle<UiDocumentAsset>);

#[derive(Component, Default)]
pub(crate) struct UiDocumentOpeningEffects {
    visibility: Vec<(AssetId, bool)>,
    selected_children: Vec<(AssetId, ActionSource)>,
    reopen_entry: bool,
}

impl UiDocumentOpeningEffects {
    fn set_node_visibility(&mut self, target: AssetId, visible: bool) {
        if let Some((_, current)) = self
            .visibility
            .iter_mut()
            .find(|(known, _)| *known == target)
        {
            *current = visible;
        } else {
            self.visibility.push((target, visible));
        }
    }
}

pub(crate) fn ingest_ui_document_opening_requests(
    mut commands: Commands,
    mut roles: MessageReader<ShowUiRole>,
    mut openings: MessageReader<ShowUiDocument>,
    mut cancellations: MessageReader<HideUiDocument>,
    server: Res<AssetServer>,
    owners: Query<Option<&Children>>,
    current: Query<(Option<&PendingUiProjection>, Option<&UiDocumentRoot>)>,
    mut visibility: Query<&mut Visibility>,
    children: Query<&Children>,
) {
    let cancelled = cancellations
        .read()
        .map(|request| request.owner)
        .collect::<HashSet<_>>();
    for &owner in &cancelled {
        for document in owners.get(owner).into_iter().flatten().flatten() {
            if current
                .get(*document)
                .is_ok_and(|(pending, loaded)| pending.is_some() || loaded.is_some())
            {
                commands.entity(*document).despawn();
            }
        }
    }
    let mut requested = HashSet::new();
    let role_openings = roles.read().filter_map(|request| {
        let path = select_bevy_asset_path_for_canonical_ui_document_role(request.role)?;
        Some((request.owner, server.load::<UiDocumentAsset>(path), true))
    });
    let document_openings = openings
        .read()
        .map(|request| (request.owner, request.document.clone(), false));
    for (owner, handle, reopen_entry) in role_openings.chain(document_openings) {
        if cancelled.contains(&owner) || !requested.insert((owner, handle.id())) {
            continue;
        }
        let Ok(owned) = owners.get(owner) else {
            continue;
        };
        let existing = owned.into_iter().flatten().copied().find_map(|entity| {
            let (pending, loaded) = current.get(entity).ok()?;
            (pending.is_some_and(|pending| pending.0 == handle)
                || loaded.is_some_and(|loaded| loaded.document == handle))
            .then_some((entity, loaded.is_some()))
        });
        if let Some((entity, loaded)) = existing {
            if loaded && reopen_entry {
                show_document_entry(entity, &children, &mut visibility);
            }
        } else {
            commands.spawn((
                PendingUiProjection(handle),
                UiDocumentOpeningEffects::default(),
                Visibility::Inherited,
                UiDocumentPauseTransfer(false),
                ChildOf(owner),
            ));
        }
    }
}

pub(crate) fn apply_authored_ui_document_opening_operations(
    mut commands: Commands,
    mut operations: MessageReader<AuthoredUiDocumentOperation>,
    server: Res<AssetServer>,
    owners: Query<Option<&Children>>,
    mut current: Query<(
        Option<&PendingUiProjection>,
        Option<&UiDocumentRoot>,
        &mut UiDocumentOpeningEffects,
        &mut UiDocumentPauseTransfer,
    )>,
    children: Query<&Children>,
    mut visibility: Query<&mut Visibility>,
) {
    let mut new_openings = Vec::<(
        Entity,
        Handle<UiDocumentAsset>,
        UiDocumentOpeningEffects,
        bool,
        bool,
    )>::new();
    let mut new_indices = HashMap::new();
    let mut cancelled = HashSet::new();
    for request in operations.read() {
        let Ok(owned) = owners.get(request.owner) else {
            continue;
        };
        let Some(path) = select_bevy_asset_path_for_canonical_ui_document_role(request.role) else {
            continue;
        };
        let opens = matches!(
            request.operation,
            UiDocumentOperation::Open | UiDocumentOperation::ShowNode { visible: true, .. }
        );
        let handle = if opens {
            server.load::<UiDocumentAsset>(path)
        } else {
            let Some(handle) = server.get_handle::<UiDocumentAsset>(path) else {
                continue;
            };
            handle
        };
        let existing = owned.into_iter().flatten().copied().find(|entity| {
            !cancelled.contains(entity)
                && current.get(*entity).is_ok_and(|(pending, loaded, ..)| {
                    pending.is_some_and(|pending| pending.0 == handle)
                        || loaded.is_some_and(|loaded| loaded.document == handle)
                })
        });
        if let Some(entity) = existing {
            let Ok((pending, _, mut effects, mut pause)) = current.get_mut(entity) else {
                continue;
            };
            match request.operation {
                UiDocumentOperation::Hide if pending.is_some() => {
                    cancelled.insert(entity);
                    commands.entity(entity).despawn();
                }
                UiDocumentOperation::Hide => {
                    *effects = UiDocumentOpeningEffects::default();
                    pause.set_if_neq(UiDocumentPauseTransfer(false));
                    if let Some(mut visibility) = children
                        .get(entity)
                        .ok()
                        .and_then(|children| children.first())
                        .and_then(|entry| visibility.get_mut(*entry).ok())
                    {
                        visibility.set_if_neq(Visibility::Hidden);
                    }
                }
                UiDocumentOperation::Open => {
                    effects.reopen_entry = true;
                    if request.transfer_pause {
                        pause.set_if_neq(UiDocumentPauseTransfer(true));
                    }
                }
                UiDocumentOperation::ShowNode { target, visible } => {
                    effects.reopen_entry |= visible;
                    effects.set_node_visibility(target, visible);
                    if visible && request.transfer_pause {
                        pause.set_if_neq(UiDocumentPauseTransfer(true));
                    }
                }
                UiDocumentOperation::ActivateSelectedChild { target, source } => {
                    effects.selected_children.push((target, source));
                }
            }
            continue;
        }
        let index = *new_indices
            .entry((request.owner, handle.id()))
            .or_insert_with(|| {
                let index = new_openings.len();
                new_openings.push((
                    request.owner,
                    handle,
                    UiDocumentOpeningEffects::default(),
                    false,
                    false,
                ));
                index
            });
        let (_, _, effects, paused, open) = &mut new_openings[index];
        match request.operation {
            UiDocumentOperation::Hide => {
                *open = false;
                *paused = false;
                *effects = UiDocumentOpeningEffects::default();
            }
            UiDocumentOperation::Open => {
                *open = true;
                *paused |= request.transfer_pause;
            }
            UiDocumentOperation::ShowNode { target, visible } if *open || visible => {
                *open = true;
                *paused |= visible && request.transfer_pause;
                effects.reopen_entry |= visible;
                effects.set_node_visibility(target, visible);
            }
            UiDocumentOperation::ActivateSelectedChild { target, source } if *open => {
                effects.selected_children.push((target, source));
            }
            _ => {}
        }
    }
    for (owner, handle, effects, paused, open) in new_openings {
        if open {
            commands.spawn((
                PendingUiProjection(handle),
                effects,
                Visibility::Inherited,
                UiDocumentPauseTransfer(paused),
                ChildOf(owner),
            ));
        }
    }
}

pub(crate) fn project_ui_documents(
    mut commands: Commands,
    pending: Query<(Entity, &PendingUiProjection, &ChildOf)>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    server: Res<AssetServer>,
    image_selection: Option<Res<UiAuthoredImageSelectionDiagnosticOverride>>,
    mut failures: MessageWriter<UiDocumentAssetLoadFailed>,
) {
    let localization = active_localization.borrow_loaded_localization_view(&localizations);
    let image_selection = image_selection
        .as_deref()
        .map(UiAuthoredImageSelectionDiagnosticOverride::candidate_index);
    for (entity, pending, parent) in &pending {
        let Some(document) = documents.get(&pending.0) else {
            if matches!(server.load_state(pending.0.id()), LoadState::Failed(_)) {
                failures.write(UiDocumentAssetLoadFailed {
                    owner: parent.parent(),
                    document: pending.0.clone(),
                });
                commands.entity(entity).despawn();
            }
            continue;
        };
        let Some(localization) = localization else {
            continue;
        };
        project_document_on_entity(
            &mut commands,
            &ShowUiDocument {
                document: pending.0.clone(),
                owner: parent.parent(),
            },
            document,
            localization,
            image_selection,
            true,
            Some(entity),
        );
        commands.entity(entity).remove::<PendingUiProjection>();
    }
}

pub(crate) fn apply_ready_ui_document_opening_effects(
    mut documents: Query<
        (
            Entity,
            &mut UiDocumentOpeningEffects,
            &mut UiDocumentPauseTransfer,
        ),
        (
            With<UiDocumentRoot>,
            Or<(Changed<UiDocumentOpeningEffects>, Added<UiDocumentRoot>)>,
        ),
    >,
    children: Query<&Children>,
    nodes: Query<(&UiNodeId, &UiDocumentOwner, Option<&UiSelected>)>,
    mut visibility: Query<&mut Visibility>,
    mut animation: Query<&mut UiShowHideAnimation>,
    mut activations: MessageWriter<UiNodeActivated>,
) {
    for (document, mut effects, mut pause) in &mut documents {
        if effects.reopen_entry {
            effects.reopen_entry = false;
            show_document_entry(document, &children, &mut visibility);
        }
        let has_target = |node, target| {
            nodes
                .get(node)
                .is_ok_and(|(id, owner, _)| owner.0 == document && id.id == target)
        };
        for (target, visible) in effects.visibility.drain(..) {
            if let Some(node) = std::iter::once(document)
                .chain(children.iter_descendants(document))
                .find(|&node| has_target(node, target))
            {
                let animated = if let Ok(mut animation) = animation.get_mut(node) {
                    animation.start_authored_visibility_transition(visible);
                    true
                } else {
                    false
                };
                if let Ok(mut visibility) = visibility.get_mut(node) {
                    if visible {
                        visibility.set_if_neq(Visibility::Inherited);
                    } else if !animated {
                        visibility.set_if_neq(Visibility::Hidden);
                    }
                }
            }
        }
        for (target, source) in effects.selected_children.drain(..) {
            let selected = children
                .iter_descendants(document)
                .find(|&node| has_target(node, target))
                .and_then(|node| children.get(node).ok())
                .and_then(|children| {
                    children.into_iter().copied().find(|&child| {
                        nodes.get(child).is_ok_and(|(_, owner, selected)| {
                            owner.0 == document && selected.is_some_and(|selected| selected.0)
                        })
                    })
                });
            if let Some(node) = selected {
                activations.write(UiNodeActivated {
                    node,
                    source,
                    trigger: UiTrigger::On,
                });
            }
        }
        pause.set_if_neq(UiDocumentPauseTransfer(false));
    }
}

fn show_document_entry(
    document: Entity,
    children: &Query<&Children>,
    visibility: &mut Query<&mut Visibility>,
) {
    if let Ok(mut visibility) = visibility.get_mut(document) {
        visibility.set_if_neq(Visibility::Inherited);
    }
    if let Some(mut visibility) = children
        .get(document)
        .ok()
        .and_then(|children| children.first())
        .and_then(|entry| visibility.get_mut(*entry).ok())
    {
        visibility.set_if_neq(Visibility::Inherited);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_removes_the_complete_owned_tree_and_cancels_a_same_update_open() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .add_message::<HideUiDocument>()
            .add_message::<ShowUiDocument>()
            .add_message::<ShowUiRole>()
            .add_systems(Update, ingest_ui_document_opening_requests);
        let owner = app.world_mut().spawn_empty().id();
        let document = Handle::<UiDocumentAsset>::default();
        let root = app
            .world_mut()
            .spawn((
                UiDocumentRoot {
                    document: document.clone(),
                },
                ChildOf(owner),
            ))
            .id();
        let child = app.world_mut().spawn(ChildOf(root)).id();
        app.world_mut()
            .write_message(ShowUiDocument { owner, document });
        app.world_mut().write_message(HideUiDocument { owner });

        app.update();

        assert!(!app.world().entities().contains(root));
        assert!(!app.world().entities().contains(child));
        assert!(app.world().entities().contains(owner));
        assert!(app
            .world()
            .get::<Children>(owner)
            .is_none_or(Children::is_empty));
    }
}
