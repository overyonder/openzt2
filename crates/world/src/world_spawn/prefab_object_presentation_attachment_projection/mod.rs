use bevy::prelude::*;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::prefab_object_presentation_attachment_projection::{
    PrefabObjectPresentationAttachmentProjection, PrefabObjectPresentationAttachmentProjectionHydrated,
};
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;

use super::physical_presentation_state::{
    child_lifecycle::PhysicalChildPlayback, physical_presentation_state_index,
    ActivePhysicalPresentationChild, ObjectPhysicalPresentationControllerOf,
    PendingPhysicalPresentationChild, PhysicalPresentationChildOf,
    PhysicalPresentationControllerDefinition, QueuedPhysicalPresentationStates,
    SelectedPhysicalPresentationState,
};
use super::prefab_attachment_independent_rotation::PrefabAttachmentIndependentRotation;

pub(super) fn initialize_authored_physical_presentation_controllers(
    mut commands: Commands,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    owners: Query<
        (Entity, &PrefabObjectPresentationAttachmentProjection),
        Without<PrefabObjectPresentationAttachmentProjectionHydrated>,
    >,
) {
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    for (owner, identity) in &owners {
        let Some(object) = definitions.find_object(identity.0) else {
            continue;
        };
        for (controller_index, controller) in object.presentation_attachments.iter().enumerate() {
            let selected = controller
                .initial_state
                .and_then(|initial| physical_presentation_state_index(controller, initial));
            commands.spawn((
                ObjectPhysicalPresentationControllerOf(owner),
                PhysicalPresentationControllerDefinition {
                    object: identity.0,
                    controller_index,
                },
                SelectedPhysicalPresentationState(selected),
                PendingPhysicalPresentationChild,
                QueuedPhysicalPresentationStates::default(),
            ));
        }
        commands
            .entity(owner)
            .insert(PrefabObjectPresentationAttachmentProjectionHydrated);
    }
}

pub(super) fn project_pending_physical_presentation_children(
    mut commands: Commands,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    prefab_assets: Res<Assets<ScenePrefabAsset>>,
    pending: Query<
        (
            Entity,
            &ObjectPhysicalPresentationControllerOf,
            &PhysicalPresentationControllerDefinition,
            &SelectedPhysicalPresentationState,
            Option<&ActivePhysicalPresentationChild>,
        ),
        With<PendingPhysicalPresentationChild>,
    >,
    children: Query<&Children>,
    attachment_ids: Query<&PrefabAuthoredAttachmentIdentifier>,
) {
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    for (controller, owner, identity, selected, active) in &pending {
        if let Some(state_index) = selected.0 {
            let Some(attachment) = definitions
                .find_object(identity.object)
                .and_then(|object| {
                    object
                        .presentation_attachments
                        .get(identity.controller_index)
                })
                .and_then(|controller| controller.states.get(state_index))
            else {
                continue;
            };
            let Some(parent) = children
                .iter_descendants_depth_first(owner.0)
                .find(|&entity| {
                    attachment_ids
                        .get(entity)
                        .is_ok_and(|id| id.0 == attachment.parent_attachment)
                })
            else {
                continue;
            };
            let Some(prefab) = definitions.scene(attachment.prefab) else {
                continue;
            };
            if !prefab_assets.contains(prefab.id()) {
                continue;
            }
            if let Some(active) = active {
                commands.entity(active.entity()).try_despawn();
            }
            let mut child = commands.spawn((
                Transform::IDENTITY,
                Visibility::Inherited,
                ChildOf(parent),
                PhysicalPresentationChildOf(controller),
                PrefabPresentation::new(prefab),
                PrefabAuthoredAttachmentIdentifier(attachment.child_binder),
            ));
            if !attachment.inherit_parent_rotation {
                child.insert(PrefabAttachmentIndependentRotation);
            }
            if attachment.child_animation.is_some() {
                child.insert(PhysicalChildPlayback::new(state_index));
            }
        } else if let Some(active) = active {
            commands.entity(active.entity()).try_despawn();
        }
        commands
            .entity(controller)
            .remove::<PendingPhysicalPresentationChild>();
    }
}
