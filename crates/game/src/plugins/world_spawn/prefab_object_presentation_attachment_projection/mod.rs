//! Projects default active authored child binders onto primary prefab sockets.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::{
    physical_presentation_state::{
        PendingPhysicalPresentationProjection, PhysicalPresentationControllers,
    },
    prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier,
    prefab_presentation_types::PrefabPresentation,
};

/// Requests the canonical default child-binder presentation for one world
/// object definition beneath an already projected primary prefab tree.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrefabObjectPresentationAttachmentProjection(pub(crate) AssetId);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PrefabObjectPresentationAttachmentProjectionHydrated;

pub(super) fn project_default_authored_world_object_presentation_attachments(
    mut commands: Commands,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    prefab_assets: Res<Assets<crate::assets::scene_prefab::ScenePrefabAsset>>,
    mut pending: Query<
        (
            Entity,
            &PrefabObjectPresentationAttachmentProjection,
            Option<&mut PhysicalPresentationControllers>,
        ),
        Or<(
            Without<PrefabObjectPresentationAttachmentProjectionHydrated>,
            With<PendingPhysicalPresentationProjection>,
        )>,
    >,
    children: Query<&Children>,
    authored_attachment_identifiers: Query<&PrefabAuthoredAttachmentIdentifier>,
) {
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    for (owner, request, controllers) in &mut pending {
        let Some(object) = definitions.find_object(request.0) else {
            continue;
        };
        let Some(mut controllers) = controllers else {
            commands
                .entity(owner)
                .insert(PhysicalPresentationControllers::from_definitions(
                    &object.presentation_attachments,
                ));
            continue;
        };
        let requested_attachments = controllers
            .pending()
            .map(|(controller, selected, old_child)| {
                let Some(selected) = selected else {
                    return Some((controller, None, old_child));
                };
                let attachment = object
                    .presentation_attachments
                    .get(controller)?
                    .states
                    .get(selected)?;
                let parent = children
                    .iter_descendants_depth_first::<Children>(owner)
                    .find(|entity| {
                        authored_attachment_identifiers
                            .get(*entity)
                            .is_ok_and(|identifier| identifier.0 == attachment.parent_attachment)
                    })?;
                let prefab = definitions.scene(attachment.prefab)?;
                prefab_assets.get(&prefab)?;
                Some((
                    controller,
                    Some((
                        parent,
                        prefab,
                        selected,
                        attachment.child_animation.is_some(),
                        attachment.inherit_parent_rotation,
                    )),
                    old_child,
                ))
            })
            .collect::<Option<Vec<_>>>();
        let Some(requested_attachments) = requested_attachments else {
            continue;
        };
        for (controller, attachment, old_child) in requested_attachments {
            let child = attachment.map(|(parent, prefab, state, animated, inherit_rotation)| {
                let mut child = commands
                    .spawn((
                        Transform::IDENTITY,
                        Visibility::Inherited,
                        ChildOf(parent),
                        PrefabPresentation::new(prefab),
                    ));
                if !inherit_rotation {
                    child.insert(super::prefab_attachment_independent_rotation::PrefabAttachmentIndependentRotation);
                }
                if animated {
                    child.insert(super::physical_presentation_state::child_lifecycle::PhysicalChildPlayback {
                        owner, controller, state, started_tick: None, completed_periods: 0,
                    });
                }
                child.id()
            });
            if let Some(old_child) = old_child {
                if let Ok(mut child) = commands.get_entity(old_child) {
                    child.despawn();
                }
            }
            controllers.projected(controller, child);
        }
        commands
            .entity(owner)
            .insert(PrefabObjectPresentationAttachmentProjectionHydrated)
            .remove::<PendingPhysicalPresentationProjection>();
    }
}
