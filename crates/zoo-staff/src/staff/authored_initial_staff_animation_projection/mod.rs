use bevy::prelude::*;

use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::ModelAnimationAttachmentResolved;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_world_state::world_spawn::prefab_object_presentation_attachment_projection::PrefabObjectPresentationAttachmentProjection;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use super::staff_employment_types::{Staff, StaffRole};
use super::staff_presentation_variant_selection::{StaffAttachedHead, StaffModelAnimationSet};

pub(crate) fn request_authored_initial_staff_animation_for_newly_resolved_prefab_models(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    newly_resolved_prefab_models: Query<
        (Entity, Option<&AnimationPlaybackController>),
        Added<ModelAnimationAttachmentResolved>,
    >,
    parents: Query<&ChildOf>,
    staff_roles: Query<&StaffRole>,
    drawn_animation_sets: Query<&StaffModelAnimationSet>,
    attached_heads: Query<(), With<StaffAttachedHead>>,
    definition_identifiers: Query<(
        Option<&DefinitionId>,
        Option<&PrefabObjectPresentationAttachmentProjection>,
    )>,
    construction_previews: Query<&ConstructionPreview>,
    mut animation_clip_playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (prefab_model_entity, animation_playback_controller) in &newly_resolved_prefab_models {
        let mut ancestor = prefab_model_entity;
        let mut drawn_animation_set = None;
        let staff_role_definition = loop {
            if attached_heads.contains(ancestor) {
                break None;
            }
            drawn_animation_set = drawn_animation_set
                .or_else(|| drawn_animation_sets.get(ancestor).ok().map(|set| set.0));
            if let Ok(staff_role) = staff_roles.get(ancestor) {
                break definitions.find_staff(staff_role.0);
            }
            if let Ok((world_definition, presentation_definition)) =
                definition_identifiers.get(ancestor)
            {
                if let Some(definition) = world_definition
                    .map(|definition| definition.0)
                    .or_else(|| presentation_definition.map(|definition| definition.0))
                {
                    break definitions.find_staff_by_object(definition);
                }
            }
            if let Ok(construction_preview) = construction_previews.get(ancestor) {
                break definitions.find_staff_by_object(construction_preview.definition);
            }
            let Ok(parent) = parents.get(ancestor) else {
                break None;
            };
            ancestor = parent.parent();
        };
        let Some(staff_role_definition) = staff_role_definition else {
            continue;
        };
        if animation_playback_controller.is_none() {
            let Some(animation_set_asset_handle) = definitions
                .load_staff_model_animation_set::<AnimationSetAsset>(
                    staff_role_definition.id,
                    drawn_animation_set,
                )
            else {
                continue;
            };
            if staff_role_definition
                .initial_animation_clip_asset_key
                .as_ref()
                .is_some_and(|initial_animation_clip_asset_key| {
                    animation_set_assets
                        .get(&animation_set_asset_handle)
                        .is_some_and(|animation_set_asset| {
                            animation_set_asset.animation_clip_asset_load_has_failed(
                                initial_animation_clip_asset_key,
                            )
                        })
                })
            {
                continue;
            }
            commands
                .entity(prefab_model_entity)
                .insert(PendingModelAnimationAssets {
                    pending_model_animation_set_asset_handles: vec![animation_set_asset_handle]
                        .into_boxed_slice(),
                    requested_initial_animation_clip_asset_key: staff_role_definition
                        .initial_animation_clip_asset_key
                        .clone(),
                })
                .remove::<ModelAnimationAttachmentResolved>();
            continue;
        }
        let Some(animation_clip_asset_key) = staff_role_definition
            .initial_animation_clip_asset_key
            .as_ref()
        else {
            continue;
        };
        animation_clip_playback_requests.write(AnimationClipPlaybackRequest {
            animation_subject_entity: prefab_model_entity,
            animation_clip_asset_key: animation_clip_asset_key.clone(),
            blend_duration_milliseconds: 0,
            playback_speed_permille: 1000,
            playback_repetition_policy: if staff_role_definition.initial_animation_loops {
                AnimationPlaybackRepetitionPolicy::Loop
            } else {
                AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy
            },
        });
    }
}

/// Staff tasks address the gameplay entity, but the body model sits under its
/// prefab node, which attachment records as the owner. Without this, every
/// authored staff clip (keeper refills, heals) is rejected as unowned. An
/// attached head keeps its own owner so it never takes the body's clips.
pub(crate) fn project_staff_animation_owners(
    mut controllers: Query<
        (Entity, &mut AnimationPresentationOwner),
        Changed<AnimationPresentationOwner>,
    >,
    parents: Query<&ChildOf>,
    staff: Query<(), With<Staff>>,
    attached_heads: Query<(), With<StaffAttachedHead>>,
) {
    for (controller, mut owner) in &mut controllers {
        if staff.contains(owner.gameplay_entity) {
            continue;
        }
        let Some(ancestor) = std::iter::once(controller)
            .chain(parents.iter_ancestors(controller))
            .find(|entity| staff.contains(*entity) || attached_heads.contains(*entity))
        else {
            continue;
        };
        if staff.contains(ancestor) {
            owner.gameplay_entity = ancestor;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_body_controller_belongs_to_staff_and_head_keeps_its_owner() {
        let mut app = App::new();
        app.add_systems(Update, project_staff_animation_owners);
        let staff = app.world_mut().spawn(Staff).id();
        let prefab_root = app.world_mut().spawn(ChildOf(staff)).id();
        let body = app
            .world_mut()
            .spawn((
                ChildOf(prefab_root),
                AnimationPresentationOwner {
                    gameplay_entity: prefab_root,
                },
            ))
            .id();
        let head = app
            .world_mut()
            .spawn((ChildOf(staff), StaffAttachedHead))
            .id();
        let head_model = app
            .world_mut()
            .spawn((
                ChildOf(head),
                AnimationPresentationOwner {
                    gameplay_entity: head,
                },
            ))
            .id();
        app.update();
        let owner = |entity| {
            app.world()
                .get::<AnimationPresentationOwner>(entity)
                .unwrap()
                .gameplay_entity
        };
        assert_eq!(owner(body), staff);
        assert_eq!(owner(head_model), head);
    }
}
