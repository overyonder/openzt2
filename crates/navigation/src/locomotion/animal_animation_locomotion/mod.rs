//! Source-backed default ground locomotion through the existing animation owner.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_data::behavior::{
    action::wander::BehaviorGroundLocomotionSpeed, action_record::BehaviorAction,
};
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_transition_operations::select_preferred_animation_clip_from_graph_node;
use openzt2_world_state::animation_playback::animation_joint_target_types::AnimationJointTarget;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackState;
use openzt2_world_state::animation_playback::locomotion_root_motion::clip_motion_velocity;
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::AnimalPresentationOwner;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::{
    find_current_behavior_task_action, BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
};
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;

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
    behavior_documents: Res<Assets<BehaviorDocumentAsset>>,
    behavior_tasks: Query<&BehaviorTaskExecutionState, Without<PendingBehaviorTaskFailure>>,
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
        (
            With<Animal>,
            Without<openzt2_zoo_state::animal_health::types::Dead>,
            Without<openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated>,
        ),
    >,
    mut playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
    mut unprojected_reasons: Local<HashMap<Entity, &'static str>>,
    mut unprojected_clip_states: Local<HashMap<Entity, String>>,
) {
    // Logs why an animal has no navigation agent yet, once per change of reason.
    let mut wait = |animal: Entity, has_agent: bool, reason: &'static str| {
        if has_agent {
            return;
        }
        if unprojected_reasons.insert(animal, reason) != Some(reason) {
            debug!(?animal, reason, "animal has no navigation agent yet");
        }
    };
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
        for (animal, _, agent, ..) in &animals {
            wait(
                animal,
                agent.is_some(),
                "species definitions are not loaded",
            );
        }
        return;
    };
    for (animal, _, agent, ..) in &animals {
        if !controllers
            .iter()
            .any(|(_, presentation, _)| presentation.0 == animal)
        {
            wait(
                animal,
                agent.is_some(),
                "no animation controller is attached to its model",
            );
        }
    }
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
        let has_agent = agent.is_some();
        if agent.is_some() && owner.is_none() {
            continue;
        }
        if owner.as_ref().is_some_and(|owner| {
            owner.controller != controller_entity && controllers.get(owner.controller).is_ok()
        }) {
            continue;
        }
        if mode.is_some_and(|mode| *mode != LocomotionMode::Ground) {
            wait(
                animal_entity,
                has_agent,
                "its locomotion mode is not ground",
            );
            continue;
        }
        let Some(variant) = species.find_variant(selected_variant.0) else {
            wait(
                animal_entity,
                has_agent,
                "its variant is not in the species definitions",
            );
            continue;
        };
        let wander = behavior_tasks
            .get(animal_entity)
            .ok()
            .and_then(|task| {
                behavior_documents
                    .get(&task.document)
                    .and_then(|document| find_current_behavior_task_action(document, task))
            })
            .and_then(|action| match action {
                BehaviorAction::Wander(wander) => Some(wander),
                _ => None,
            });
        let locomotion_key = match wander
            .map(|wander| wander.locomotion_speed)
            .unwrap_or_default()
        {
            BehaviorGroundLocomotionSpeed::Slow => {
                variant.ground_navigation_clip_asset_key.as_ref()
            }
            BehaviorGroundLocomotionSpeed::Medium => {
                variant.medium_ground_navigation_clip_asset_key.as_ref()
            }
            BehaviorGroundLocomotionSpeed::Fast => {
                variant.fast_ground_navigation_clip_asset_key.as_ref()
            }
        };
        if wander.is_some() && locomotion_key.is_none() {
            openzt2_zoo_state::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(animal_entity, &mut commands);
        }
        let (Some(radius), Some(locomotion_key)) = (
            variant
                .navigation_collision_policy
                .as_ref()
                .map(|policy| policy.radius_m),
            locomotion_key,
        ) else {
            wait(
                animal_entity,
                has_agent,
                "its variant has no collision radius or ground clip",
            );
            continue;
        };
        if !radius.is_finite() || radius < 0.0 {
            wait(animal_entity, has_agent, "its collision radius is invalid");
            continue;
        }
        // Request every ground clip once. Touching the set mutably every tick
        // would mark it changed and invalidate its playback graph each time.
        let unrequested = animations
            .get(&controller.animation_set_asset)
            .and_then(|set| {
                let node = set
                    .animation_graph_nodes
                    .iter()
                    .find(|node| node.animation_graph_node_asset_key == *locomotion_key)?;
                Some(
                    node.animation_clip_asset_keys
                        .iter()
                        .filter(|key| {
                            set.find_animation_clip_record_by_asset_key(key).is_none()
                                && !set.animation_clip_asset_load_has_failed(key)
                                && !set.animation_clip_asset_load_is_pending(key)
                        })
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or_default();
        if !unrequested.is_empty() {
            if let Some(mut set) = animations.get_mut(&controller.animation_set_asset) {
                for key in unrequested {
                    set.request_animation_clip_asset_load(&asset_server, &key);
                }
            }
        }
        let Some(set) = animations.get(&controller.animation_set_asset) else {
            wait(animal_entity, has_agent, "its animation set is not loaded");
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
            // Only the preferred clip is measured. It is settled once loaded,
            // unless a clip still loading would be preferred over it.
            let selected = select_preferred_animation_clip_from_graph_node(set, node);
            let selected_is_settled = selected.as_deref().is_some_and(|key| {
                key.ends_with("_Idle")
                    || (key.ends_with("_Ahead")
                        && !pending_keys
                            .iter()
                            .any(|pending| pending.ends_with("_Idle")))
            });
            if !pending_keys.is_empty() && !selected_is_settled {
                wait(
                    animal_entity,
                    has_agent,
                    "its ground clips are still loading",
                );
                if !has_agent {
                    let states = pending_keys
                        .iter()
                        .map(|key| {
                            let state = set
                                .pending_animation_clip_source_asset_handles
                                .iter()
                                .find(|(pending, _)| pending == key)
                                .map(|(_, handle)| asset_server.get_load_state(handle.id()));
                            format!("{key}={state:?}")
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    if unprojected_clip_states
                        .insert(animal_entity, states.clone())
                        .as_ref()
                        != Some(&states)
                    {
                        debug!(animal = ?animal_entity, clips = %states,
                            "animal ground clips waiting to load");
                    }
                }
                continue;
            }
            let Some(selected) = selected else {
                wait(
                    animal_entity,
                    has_agent,
                    "its ground graph node selects no loaded clip",
                );
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
            let failed = set.animation_clip_asset_load_has_failed(&clip_key);
            wait(
                animal_entity,
                has_agent,
                if failed {
                    "its ground clip failed to load"
                } else {
                    "its ground clip is loading"
                },
            );
            if !set.animation_clip_asset_load_is_pending(&clip_key) && !failed {
                if let Some(mut set) = animations.get_mut(&controller.animation_set_asset) {
                    set.request_animation_clip_asset_load(&asset_server, &clip_key);
                }
            }
            continue;
        }
        let Some(clip) = set.find_animation_clip_record_by_asset_key(&clip_key) else {
            wait(animal_entity, has_agent, "its ground clip has no record");
            continue;
        };
        let Some(root) = clip
            .authored_animation_clip_metadata
            .as_ref()
            .and_then(|metadata| metadata.root_motion_joint.as_ref())
        else {
            wait(
                animal_entity,
                has_agent,
                "its ground clip has no root motion joint",
            );
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
            wait(
                animal_entity,
                has_agent,
                "its root motion joint is not bound to the model",
            );
            continue;
        };
        let Some(default_velocity) = clip_motion_velocity(clip, parent, 1000) else {
            wait(
                animal_entity,
                has_agent,
                "its ground clip motion cannot be measured",
            );
            continue;
        };
        let default_speed = default_velocity.length();
        if !default_speed.is_finite() || default_speed <= 0.0 {
            wait(animal_entity, has_agent, "its ground clip does not move");
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
