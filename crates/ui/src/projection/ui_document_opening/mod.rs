use std::collections::{HashMap, HashSet};

use bevy::{asset::LoadState, ecs::system::SystemParam, prelude::*};
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
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_interface_state::ui::{
    authored_ui_activation_contracts::UiNodeActivated,
    authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
    authored_ui_selection_state::UiSelected,
    ui_document_asset_load_failure::UiDocumentAssetLoadFailed,
    ui_document_lifecycle_contracts::{
        AuthoredUiDocumentOperation, HideUiDocument, ShowUiDocument, ShowUiRole, ShowUiRoleInScope,
        UiDocumentOperation, UiRoleScope,
    },
};
use openzt2_world_state::simulation_time::simulation_control_types::UiDocumentPauseTransfer;

use super::{
    authored_document_projection::{project_document_on_entity, UiDocumentLifetimeAttachment},
    document_asset_readiness::{document_dependencies_resolved, UiDocumentAssetEvents},
};
use crate::{
    animation::{UiAnimationPlaybackOperation, UiAnimationPlaybackRequest, UiShowHideAnimation},
    authored_image_selection_diagnostic_override::UiAuthoredImageSelectionDiagnosticOverride,
    authored_ui_change_activation_dispatch::{
        apply_visibility_transition, set_authored_visibility,
    },
    authored_ui_target_lookup::AuthoredUiTargetLookup,
};

#[derive(Component)]
pub(crate) struct PendingUiProjection(Handle<UiDocumentAsset>);

#[derive(Component, Default)]
pub(crate) struct UiDocumentOpeningEffects(Vec<UiDocumentOperation>);

impl UiDocumentOpeningEffects {
    pub(crate) fn drain(&mut self) -> impl Iterator<Item = UiDocumentOperation> + '_ {
        self.0.drain(..)
    }
}

pub(crate) fn ingest_ui_document_opening_requests(
    mut commands: Commands,
    mut roles: MessageReader<ShowUiRole>,
    scoped: (
        Option<MessageReader<ShowUiRoleInScope>>,
        Query<Option<&mut UiRoleScope>>,
    ),
    mut openings: MessageReader<ShowUiDocument>,
    mut cancellations: MessageReader<HideUiDocument>,
    server: Res<AssetServer>,
    owners: Query<Option<&Children>>,
    current: Query<(Option<&PendingUiProjection>, Option<&UiDocumentRoot>)>,
    mut visibility: Query<&mut Visibility>,
    mut activations: MessageWriter<UiNodeActivated>,
    children: Query<&Children>,
) {
    let (mut scopes, mut scope_owners) = scoped;
    let cancelled = cancellations
        .read()
        .map(|request| request.owner)
        .collect::<HashSet<_>>();
    for &owner in &cancelled {
        if scope_owners.get(owner).is_ok_and(|scope| scope.is_some()) {
            commands.entity(owner).remove::<UiRoleScope>();
        }
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
    let mut scoped_requests = Vec::new();
    let mut request_indices = bevy::ecs::entity::EntityHashMap::default();
    for request in scopes.as_mut().into_iter().flat_map(|scopes| scopes.read()) {
        if cancelled.contains(&request.owner)
            || cancelled.contains(&request.host)
            || request.owner == request.host
            || !owners.contains(request.host)
        {
            continue;
        }
        let Some(path) = select_bevy_asset_path_for_canonical_ui_document_role(request.role) else {
            continue;
        };
        if !scope_owners.contains(request.owner) {
            continue;
        }
        let next = (
            request.owner,
            UiRoleScope {
                host: request.host,
                role: request.role,
            },
            path,
        );
        let index = *request_indices
            .entry(request.owner)
            .or_insert(scoped_requests.len());
        if index == scoped_requests.len() {
            scoped_requests.push(next);
        } else {
            scoped_requests[index] = next;
        }
    }
    let mut scoped_openings = Vec::new();
    for (owner, next, path) in scoped_requests {
        let Ok(current) = scope_owners.get_mut(owner) else {
            continue;
        };
        let host = next.host;
        if let Some(mut scope) = current {
            scope.set_if_neq(next);
        } else {
            commands.entity(owner).insert(next);
        }
        scoped_openings.push((host, server.load::<UiDocumentAsset>(path), false));
    }
    let document_openings = openings
        .read()
        .map(|request| (request.owner, request.document.clone(), false));
    for (owner, handle, reopen_entry) in role_openings
        .chain(document_openings)
        .chain(scoped_openings)
    {
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
                show_document_entry(entity, &children, &mut visibility, &mut |event| {
                    activations.write(event);
                });
            }
        } else {
            commands.spawn((
                PendingUiProjection(handle),
                UiDocumentOpeningEffects::default(),
                Visibility::Inherited,
                UiDocumentPauseTransfer(false),
                UiDocumentLifetimeAttachment,
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
    mut activations: MessageWriter<UiNodeActivated>,
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
                    if let Some(entry) = children
                        .get(entity)
                        .ok()
                        .and_then(|children| children.first().copied())
                    {
                        if let Ok(mut visibility) = visibility.get_mut(entry) {
                            apply_visibility_transition(
                                entry,
                                &mut visibility,
                                Visibility::Hidden,
                                true,
                                &mut activations,
                            );
                        }
                    }
                }
                UiDocumentOperation::Open => {
                    effects.0.push(UiDocumentOperation::Open);
                    if request.transfer_pause {
                        pause.set_if_neq(UiDocumentPauseTransfer(true));
                    }
                }
                UiDocumentOperation::ShowNode { target, visible } => {
                    effects
                        .0
                        .push(UiDocumentOperation::ShowNode { target, visible });
                    if visible && request.transfer_pause {
                        pause.set_if_neq(UiDocumentPauseTransfer(true));
                    }
                }
                UiDocumentOperation::PressNode { .. }
                | UiDocumentOperation::ActivateSelectedChild { .. } => {
                    effects.0.push(request.operation);
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
                effects
                    .0
                    .push(UiDocumentOperation::ShowNode { target, visible });
            }
            UiDocumentOperation::PressNode { .. }
            | UiDocumentOperation::ActivateSelectedChild { .. }
                if *open =>
            {
                effects.0.push(request.operation);
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
                UiDocumentLifetimeAttachment,
                ChildOf(owner),
            ));
        }
    }
}

pub(crate) fn project_ui_documents(
    mut commands: Commands,
    pending: Query<(Entity, Ref<PendingUiProjection>, &ChildOf)>,
    mut asset_events: UiDocumentAssetEvents,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    server: Res<AssetServer>,
    image_selection: Option<Res<UiAuthoredImageSelectionDiagnosticOverride>>,
    mut failures: MessageWriter<UiDocumentAssetLoadFailed>,
) {
    let changes = asset_events.collect_changes();
    if changes.is_empty() && !pending.iter().any(|(_, pending, _)| pending.is_added()) {
        return;
    }
    let localization = active_localization.borrow_loaded_localization_view(&localizations);
    let image_selection = image_selection
        .as_deref()
        .map(UiAuthoredImageSelectionDiagnosticOverride::candidate_index);
    for (entity, pending, parent) in &pending {
        if !pending.is_added() && !changes.affects(pending.0.id()) {
            continue;
        }
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
        if !document_dependencies_resolved(&server, &pending.0) {
            continue;
        }
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

#[derive(SystemParam)]
pub(crate) struct LiveUiDocumentEffects<'w, 's> {
    documents: Res<'w, Assets<UiDocumentAsset>>,
    roots: Query<'w, 's, &'static UiDocumentRoot>,
    children: Query<'w, 's, &'static Children>,
    nodes: Query<'w, 's, (&'static UiDocumentOwner, &'static UiSelected)>,
    targets: AuthoredUiTargetLookup<'w, 's>,
    visibility: Query<'w, 's, &'static mut Visibility>,
    animations: Query<'w, 's, (), With<UiShowHideAnimation>>,
    playback: MessageWriter<'w, UiAnimationPlaybackRequest>,
}

impl LiveUiDocumentEffects<'_, '_> {
    pub(crate) fn document_of_role(
        &self,
        owner: Entity,
        role: openzt2_game_data::ui_document::document::UiDocumentRole,
    ) -> Option<Entity> {
        self.children.get(owner).ok()?.iter().find(|child| {
            self.roots
                .get(*child)
                .ok()
                .and_then(|root| self.documents.get(&root.document))
                .is_some_and(|document| document.canonical_ui_document().role == role)
        })
    }

    pub(crate) fn apply(
        &mut self,
        document: Entity,
        effect: UiDocumentOperation,
        emit: &mut impl FnMut(UiNodeActivated),
    ) {
        match effect {
            UiDocumentOperation::Open => {
                show_document_entry(document, &self.children, &mut self.visibility, emit);
            }
            UiDocumentOperation::ShowNode { target, visible } => {
                if visible {
                    show_document_entry(document, &self.children, &mut self.visibility, emit);
                }
                let Some(node) = self.targets.candidates(document, target).next() else {
                    return;
                };
                let animated = self.animations.contains(node);
                if animated {
                    self.playback.write(UiAnimationPlaybackRequest {
                        entity: node,
                        operation: UiAnimationPlaybackOperation::Transition(visible),
                    });
                }
                if visible || !animated {
                    if let Ok(mut visibility) = self.visibility.get_mut(node) {
                        if let Some(event) = set_authored_visibility(
                            node,
                            &mut visibility,
                            if visible {
                                Visibility::Inherited
                            } else {
                                Visibility::Hidden
                            },
                            true,
                        ) {
                            emit(event);
                        }
                    }
                }
            }
            UiDocumentOperation::PressNode { target, source } => {
                if let Some(node) = self.targets.candidates(document, target).next() {
                    emit(UiNodeActivated {
                        node,
                        source,
                        trigger: UiTrigger::Press,
                    });
                }
            }
            UiDocumentOperation::ActivateSelectedChild { target, source } => {
                let selected = self
                    .targets
                    .candidates(document, target)
                    .next()
                    .and_then(|node| self.children.get(node).ok())
                    .and_then(|children| {
                        children.iter().find(|node| {
                            self.nodes
                                .get(*node)
                                .is_ok_and(|(owner, selected)| owner.0 == document && selected.0)
                        })
                    });
                if let Some(node) = selected {
                    emit(UiNodeActivated {
                        node,
                        source,
                        trigger: UiTrigger::On,
                    });
                }
            }
            UiDocumentOperation::Hide => {
                if let Ok(mut visibility) = self.visibility.get_mut(document) {
                    if let Some(event) =
                        set_authored_visibility(document, &mut visibility, Visibility::Hidden, true)
                    {
                        emit(event);
                    }
                }
            }
        }
    }
}

fn show_document_entry(
    document: Entity,
    children: &Query<&Children>,
    visibility: &mut Query<&mut Visibility>,
    emit: &mut impl FnMut(UiNodeActivated),
) {
    let entry = children
        .get(document)
        .ok()
        .and_then(|children| children.first().copied());
    for node in std::iter::once(document).chain(entry) {
        if let Ok(mut visibility) = visibility.get_mut(node) {
            if let Some(event) =
                set_authored_visibility(node, &mut visibility, Visibility::Inherited, true)
            {
                emit(event);
            }
        }
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
            .add_message::<UiNodeActivated>()
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

pub(crate) fn open_authored_award_panel(
    mut requests: MessageReader<
        openzt2_interface_state::information::award_presentation_types::AwardProgramEffect,
    >,
    owners: Query<
        (
            Entity,
            Option<&openzt2_world_state::world_spawn::world_membership_types::WorldMember>,
        ),
        With<openzt2_interface_state::ui::ui_document_lifecycle_contracts::InGameUiOwner>,
    >,
    mut documents: MessageWriter<AuthoredUiDocumentOperation>,
) {
    use openzt2_game_data::ui_document::document::UiDocumentRole;
    use openzt2_interface_state::information::award_presentation_types::AwardProgramEffect;

    for request in requests.read() {
        let AwardProgramEffect::ShowAwardPanel { root } = *request else {
            continue;
        };
        let Some((owner, _)) = owners
            .iter()
            .find(|(_, world)| world.is_some_and(|world| world.root == root))
            .or_else(|| owners.single().ok().filter(|(_, world)| world.is_none()))
        else {
            continue;
        };
        let role = UiDocumentRole::ZooStatus;
        for operation in [
            UiDocumentOperation::Open,
            UiDocumentOperation::PressNode {
                target: role.node_id("awards tab"),
                source: openzt2_interface_state::input::input_types::ActionSource::System,
            },
        ] {
            documents.write(AuthoredUiDocumentOperation {
                owner,
                role,
                operation,
                transfer_pause: false,
            });
        }
    }
}
