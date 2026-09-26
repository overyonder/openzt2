use bevy::prelude::*;

use crate::assets::animation::animation_set_asset_types::AnimationSetAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use crate::plugins::animation_graph::model_animation_asset_binding_types::ModelAnimationAttachmentResolved;
use crate::plugins::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use crate::plugins::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use crate::plugins::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use crate::plugins::construction::construction_interaction_types::ConstructionPreview;
use crate::plugins::world_spawn::prefab_object_presentation_attachment_projection::PrefabObjectPresentationAttachmentProjection;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::staff_employment_types::StaffRole;

pub(in crate::plugins::staff) fn request_authored_initial_staff_animation_for_newly_resolved_prefab_models(
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
        let staff_role_definition = loop {
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
                .load_staff_model_animation_set::<AnimationSetAsset>(staff_role_definition.id)
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
