//! Source-backed default ground locomotion through the existing animation owner.

use super::locomotion_types::{Destination, DirectLocomotion, LocomotionMode, NavAgent, NavFlags};
use crate::{
    assets::{
        animation::animation_set_asset_types::AnimationSetAsset,
        species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    },
    plugins::{
        animal_lifecycle::{
            animal_presentation_attachment::AnimalPresentationOwner,
            types::{Animal, AnimalVariant},
        },
        animation_graph::{
            animation_graph_playback_message_types::AnimationClipPlaybackRequest,
            animation_graph_transition_operations::select_preferred_animation_clip_from_graph_node,
        },
        animation_playback::{
            animation_joint_target_types::AnimationJointTarget,
            animation_playback_controller_types::{
                AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
                AnimationPlaybackState,
            },
            locomotion_root_motion::{clip_motion_velocity, ConsumedLocomotionRootMotion},
        },
        behavior_task_execution_types::PendingBehaviorAnimationClipCompletion,
    },
};
use bevy::prelude::*;

/// Identifies only navigation created here; other agent owners are untouched.
#[derive(Component, Clone, Copy)]
pub(super) struct AnimalNavigationAnimationOwner {
    controller: Entity,
    move_request_id: Option<usize>,
    requested_generation: u64,
}

pub(super) fn project_animal_animation_motion_to_navigation(
    mut commands: Commands,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    mut animations: ResMut<Assets<AnimationSetAsset>>,
    asset_server: Res<AssetServer>,
    controllers: Query<(
        Entity,
        &AnimalPresentationOwner,
        &AnimationPlaybackController,
    )>,
    children: Query<&Children>,
    joints: Query<(&AnimationJointTarget, &ChildOf)>,
    transforms: Query<&GlobalTransform>,
    mut animals: Query<
        (
            Entity,
            &AnimalVariant,
            Option<&mut NavAgent>,
            Option<&mut AnimalNavigationAnimationOwner>,
            Option<&LocomotionMode>,
            Has<Destination>,
            Has<DirectLocomotion>,
            Has<PendingBehaviorAnimationClipCompletion>,
        ),
        With<Animal>,
    >,
    mut playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
) {
    // Suspend owned agents first. A missing/reloaded controller or asset must
    // never leave stale movement enabled; successful projection restores speed.
    for (_, _, agent, owner, _, _, _, _) in &mut animals {
        if owner.is_some() {
            if let Some(mut agent) = agent {
                agent.max_speed_mps = 0.0;
            }
        }
    }
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    for (controller_entity, presentation, controller) in &controllers {
        let Ok((
            animal_entity,
            selected_variant,
            agent,
            owner,
            mode,
            navigating,
            direct,
            task_animation,
        )) = animals.get_mut(presentation.0)
        else {
            continue;
        };
        if agent.is_some() && owner.is_none() {
            continue;
        }
        if owner.as_ref().is_some_and(|owner| {
            owner.controller != controller_entity && controllers.get(owner.controller).is_ok()
        }) {
            continue;
        }
        if mode.is_some_and(|mode| *mode != LocomotionMode::Ground) {
            continue;
        }
        let Some(variant) = species.find_variant(selected_variant.0) else {
            continue;
        };
        let (Some(radius), Some(locomotion_key)) = (
            variant
                .navigation_collision_policy
                .as_ref()
                .map(|policy| policy.radius_m),
            variant.ground_navigation_clip_asset_key.as_ref(),
        ) else {
            continue;
        };
        if !radius.is_finite() || radius < 0.0 {
            continue;
        }
        let Some(set) = animations.get(&controller.animation_set_asset) else {
            continue;
        };
        let clip_key = if let Some(node) = set
            .animation_graph_nodes
            .iter()
            .find(|node| node.animation_graph_node_asset_key == *locomotion_key)
        {
            let pending_keys = node
                .animation_clip_asset_keys
                .iter()
                .filter(|key| {
                    set.find_animation_clip_record_by_asset_key(key).is_none()
                        && !set.animation_clip_asset_load_has_failed(key)
                })
                .cloned()
                .collect::<Vec<_>>();
            if !pending_keys.is_empty() {
                if let Some(mut set) = animations.get_mut(&controller.animation_set_asset) {
                    for key in pending_keys {
                        if !set.animation_clip_asset_load_is_pending(&key) {
                            set.request_animation_clip_asset_load(&asset_server, &key);
                        }
                    }
                }
                continue;
            }
            let Some(selected) = select_preferred_animation_clip_from_graph_node(set, node) else {
                continue;
            };
            selected
        } else {
            locomotion_key.clone()
        };
        if set
            .find_animation_clip_record_by_asset_key(&clip_key)
            .is_none()
        {
            if !set.animation_clip_asset_load_is_pending(&clip_key)
                && !set.animation_clip_asset_load_has_failed(&clip_key)
            {
                if let Some(mut set) = animations.get_mut(&controller.animation_set_asset) {
                    set.request_animation_clip_asset_load(&asset_server, &clip_key);
                }
            }
            continue;
        }
        let Some(clip) = set.find_animation_clip_record_by_asset_key(&clip_key) else {
            continue;
        };
        let Some(root) = clip
            .authored_animation_clip_metadata
            .as_ref()
            .and_then(|metadata| metadata.root_motion_joint.as_ref())
        else {
            continue;
        };
        let parent = std::iter::once(controller_entity)
            .chain(children.iter_descendants_depth_first::<Children>(controller_entity))
            .find_map(|entity| {
                let (binding, parent) = joints.get(entity).ok()?;
                (binding.animation_playback_controller_entity == controller_entity
                    && binding.joint_asset_key == root.joint_asset_key)
                    .then(|| transforms.get(parent.parent()).ok())
                    .flatten()
            });
        let Some(parent) = parent else {
            continue;
        };
        let Some(default_velocity) = clip_motion_velocity(clip, parent, 1000) else {
            continue;
        };
        let default_speed = default_velocity.length();
        if !default_speed.is_finite() || default_speed <= 0.0 {
            continue;
        }
        let mut ownership = owner
            .as_deref()
            .copied()
            .filter(|owner| owner.controller == controller_entity)
            .unwrap_or(AnimalNavigationAnimationOwner {
                controller: controller_entity,
                move_request_id: None,
                requested_generation: controller.playback_generation,
            });
        let moving = navigating || direct;
        if !task_animation
            && ownership.move_request_id.is_some()
            && ((controller.playback_generation != ownership.requested_generation
                && controller.explicit_clip_request_id != ownership.move_request_id)
                || (controller.explicit_clip_request_id == ownership.move_request_id
                    && controller.animation_clip_asset_key != clip_key))
        {
            ownership.move_request_id = None;
        }
        if moving && !task_animation && ownership.move_request_id.is_none() {
            let request = playback_requests.write(AnimationClipPlaybackRequest {
                animation_subject_entity: controller_entity,
                animation_clip_asset_key: clip_key.clone(),
                blend_duration_milliseconds: 0,
                playback_speed_permille: 1000,
                playback_repetition_policy: AnimationPlaybackRepetitionPolicy::Loop,
            });
            ownership.move_request_id = Some(request.id);
            ownership.requested_generation = controller.playback_generation;
            commands
                .entity(controller_entity)
                .insert(ConsumedLocomotionRootMotion {
                    playback_request_id: request.id,
                });
        } else if !moving && ownership.move_request_id.take().is_some() {
            commands
                .entity(controller_entity)
                .remove::<ConsumedLocomotionRootMotion>();
            if !task_animation {
                if let Some(idle) = &variant.initial_animation_clip_asset_key {
                    playback_requests.write(AnimationClipPlaybackRequest {
                        animation_subject_entity: controller_entity,
                        animation_clip_asset_key: idle.clone(),
                        blend_duration_milliseconds: 0,
                        playback_speed_permille: 1000,
                        playback_repetition_policy: if variant.initial_animation_loops {
                            AnimationPlaybackRepetitionPolicy::Loop
                        } else {
                            AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy
                        },
                    });
                }
            }
        }
        let speed = if moving {
            if !task_animation
                && controller.animation_clip_asset_key == clip_key
                && controller.explicit_clip_request_id == ownership.move_request_id
                && controller.playback_state == AnimationPlaybackState::Playing
            {
                clip_motion_velocity(clip, parent, controller.playback_speed_permille)
                    .map_or(0.0, |velocity| velocity.length())
            } else {
                0.0
            }
        } else {
            default_speed
        };
        // Native active motion is the controller's current transformed vector,
        // not an independently accelerated velocity. Playback modulation owns
        // temporal speed changes; the existing navigation driver applies it.
        let projected = NavAgent {
            radius_m: radius,
            max_speed_mps: speed,
            acceleration_mps2: f32::MAX,
            capabilities: NavFlags::ANIMAL,
        };
        if let Some(mut agent) = agent {
            *agent = projected;
        } else {
            commands.entity(animal_entity).insert(projected);
        }
        if let Some(mut owner) = owner {
            *owner = ownership;
        } else {
            commands.entity(animal_entity).insert(ownership);
        }
    }
}
