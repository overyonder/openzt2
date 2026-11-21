//! Source-backed default ground locomotion through the existing animation owner.

use super::locomotion_clip_dependencies::LocomotionClipDependencies;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use openzt2_game_assets::animation::animation_clip_source_queries::AnimationClipSources;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_data::behavior::action::wander::BehaviorGroundLocomotionSpeed;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_transition_operations::select_preferred_animation_clip_from_graph_node;
use openzt2_world_state::animation_playback::animation_joint_target_types::{
    AnimationJointTarget, AnimationRigJoints,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_world_state::animation_playback::locomotion_root_motion::clip_motion_velocity;
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::{
    running_actions::BehaviorLocomotionIntent, PendingBehaviorTaskFailure,
};
use openzt2_zoo_state::locomotion::locomotion_types::CaffeineMode;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;

/// Identifies only navigation created here; other agent owners are untouched.
#[derive(Component, Clone, Copy)]
pub(super) struct AnimalNavigationAnimationOwner {
    move_request_id: Option<usize>,
    requested_generation: u64,
    rate_source: Option<openzt2_game_data::AssetId>,
    playback_speed_permille: i16,
    rate_seconds_remaining: f32,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AnimalLocomotionRuntime<'w, 's> {
    fixed_time: Res<'w, Time<Fixed>>,
    random_streams: Query<'w, 's, &'static mut openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream>,
    unprojected_reasons: Local<'s, HashMap<Entity, &'static str>>,
    caffeine_modes: Query<'w, 's, &'static CaffeineMode>,
}

pub(super) fn project_animal_animation_motion_to_navigation(
    mut commands: Commands,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    locomotion_intents: Query<&BehaviorLocomotionIntent, Without<PendingBehaviorTaskFailure>>,
    docking_poses: Query<
        (),
        With<openzt2_zoo_state::behavior_task_execution_types::SamplingBehaviorDockPose>,
    >,
    animations: Res<Assets<AnimationSetAsset>>,
    clip_sources: AnimationClipSources,
    mut controllers: Query<
        (
            Entity,
            &AnimationPlaybackController,
            &AnimationPlayer,
            Option<&AnimationRigJoints>,
            Option<&mut ConsumedLocomotionRootMotion>,
            Option<&mut LocomotionClipDependencies>,
        ),
        With<Animal>,
    >,
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
            Has<openzt2_zoo_state::behavior_task_execution_types::BehaviorHeldAnimationClip>,
        ),
        (
            With<Animal>,
            Without<openzt2_zoo_state::animal_health::types::Dead>,
            Without<openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated>,
        ),
    >,
    mut playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
    runtime: AnimalLocomotionRuntime,
) {
    let AnimalLocomotionRuntime {
        fixed_time,
        mut random_streams,
        mut unprojected_reasons,
        caffeine_modes,
    } = runtime;
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
    for (_, _, agent, owner, _, _, _, _, _) in &mut animals {
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
        if !controllers.contains(animal) {
            wait(
                animal,
                agent.is_some(),
                "no animation controller is attached to its model",
            );
        }
    }
    for (controller_entity, controller, player, rig, consumed, dependencies) in &mut controllers {
        let Ok((
            animal_entity,
            selected_variant,
            agent,
            owner,
            mode,
            navigating,
            direct,
            task_animation,
            held_animation,
        )) = animals.get_mut(controller_entity)
        else {
            continue;
        };
        let has_agent = agent.is_some();
        let task_animation =
            task_animation || held_animation || docking_poses.contains(animal_entity);
        if agent.is_some() && owner.is_none() {
            continue;
        }
        if mode.is_some_and(|mode| !matches!(*mode, LocomotionMode::Ground | LocomotionMode::Swim))
        {
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
        let movement = locomotion_intents
            .get(animal_entity)
            .ok()
            .map(|intent| intent.0)
            .filter(|speed| *speed != BehaviorGroundLocomotionSpeed::Stop);
        let water = variant
            .surface_water_locomotion
            .as_ref()
            .filter(|_| mode == Some(&LocomotionMode::Swim));
        let water_row = water.map(|water| water.row(movement.unwrap_or_default()));
        let locomotion_key = if let Some(row) = water_row {
            Some(&row.node)
        } else if mode == Some(&LocomotionMode::Swim) {
            None
        } else {
            // A table without the row moves at its `slow` clip.
            let slow = variant.ground_navigation_clip_asset_key.as_ref();
            match movement.unwrap_or_default() {
                BehaviorGroundLocomotionSpeed::Medium => {
                    variant.medium_ground_navigation_clip_asset_key.as_ref()
                }
                BehaviorGroundLocomotionSpeed::Fast => {
                    variant.fast_ground_navigation_clip_asset_key.as_ref()
                }
                speed => variant
                    .named_ground_navigation_clip_asset_keys
                    .iter()
                    .find(|(key, _)| *key == speed.row_key())
                    .map(|(_, clip)| clip),
            }
            .or(slow)
        };
        if movement.is_some() && locomotion_key.is_none() {
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
        let Some(set) = animations.get(&controller.animation_set_asset) else {
            wait(animal_entity, has_agent, "its animation set is not loaded");
            continue;
        };
        LocomotionClipDependencies::retain(
            &mut commands,
            controller_entity,
            dependencies,
            controller.animation_set_asset.id(),
            locomotion_key,
            set,
            &clip_sources,
        );
        let resolve = |key: &str| clip_sources.get(set, key);
        let pending = |key: &str| {
            resolve(key).is_none()
                && clip_sources
                    .retained_handle(set, key)
                    .is_some_and(|handle| !clip_sources.failed(&handle))
        };
        let clip_key = if let Some(node) = set
            .animation_graph_nodes
            .iter()
            .find(|node| node.animation_graph_node_asset_key == *locomotion_key)
        {
            let selected = select_preferred_animation_clip_from_graph_node(
                set,
                |key| resolve(key).is_some(),
                node,
            );
            let settled = selected.as_deref().is_some_and(|key| {
                key.ends_with("_Idle")
                    || (key.ends_with("_Ahead")
                        && !node
                            .animation_clip_asset_keys
                            .iter()
                            .any(|key| key.ends_with("_Idle") && pending(key)))
            });
            if !settled
                && node
                    .animation_clip_asset_keys
                    .iter()
                    .any(|key| pending(key))
            {
                wait(
                    animal_entity,
                    has_agent,
                    "its ground clips are still loading",
                );
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
        let Some(clip) = resolve(&clip_key) else {
            wait(animal_entity, has_agent, "its ground clip is unavailable");
            continue;
        };
        let Some(root) = clip
            .metadata
            .and_then(|metadata| metadata.root_motion_joint.as_ref())
        else {
            wait(
                animal_entity,
                has_agent,
                "its ground clip has no root motion joint",
            );
            continue;
        };
        let joint = openzt2_game_data::AssetId::from_key(&root.joint_asset_key);
        let parent = rig
            .into_iter()
            .flat_map(|rig| joints.iter_many(rig.joints()).matched())
            .find_map(|(binding, parent)| {
                (binding.authored_joint_id == joint)
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
        let actor_from_root_parent = transforms
            .get(animal_entity)
            .map_or(bevy::math::Affine3A::IDENTITY, |actor| {
                actor.affine().inverse()
            })
            * parent.affine();
        if let Some(mut consumed) = consumed {
            consumed.update_root_parent_transform(actor_from_root_parent);
        }
        let Some(default_velocity) = clip_motion_velocity(&clip, parent, 1000) else {
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
            .unwrap_or(AnimalNavigationAnimationOwner {
                move_request_id: None,
                requested_generation: controller.playback_generation,
                rate_source: None,
                playback_speed_permille: 1000,
                rate_seconds_remaining: 0.0,
            });
        let rate_source = water_row.map(|row| {
            openzt2_game_data::AssetId::from_key(&format!(
                "{}:{:?}",
                row.node,
                movement.unwrap_or_default()
            ))
        });
        ownership.rate_seconds_remaining -= fixed_time.delta_secs();
        if rate_source != ownership.rate_source
            || (rate_source.is_some() && ownership.rate_seconds_remaining <= 0.0)
        {
            if let Some(row) = water_row {
                let Ok(mut random) = random_streams.get_mut(animal_entity) else {
                    continue;
                };
                let rate = row.playback_speed_range[0]
                    + random.0.unit_f32()
                        * (row.playback_speed_range[1] - row.playback_speed_range[0]);
                ownership.playback_speed_permille =
                    (rate * 1000.0).round().clamp(1.0, f32::from(i16::MAX)) as i16;
                ownership.rate_seconds_remaining = row.reset_seconds_range[0]
                    + random.0.unit_f32()
                        * (row.reset_seconds_range[1] - row.reset_seconds_range[0]);
            } else {
                ownership.playback_speed_permille = 1000;
            }
            ownership.rate_source = rate_source;
        }
        let moving = navigating || direct;
        // Movement follows the clip's root motion, so scaling its rate speeds
        // the animal up or slows it down.
        let rate = (f32::from(ownership.playback_speed_permille)
            * CaffeineMode::speed_factor(caffeine_modes.get(animal_entity).ok()))
        .round()
        .clamp(1.0, f32::from(i16::MAX)) as i16;
        if !task_animation
            && ownership.move_request_id.is_some()
            && ((controller.playback_generation != ownership.requested_generation
                && controller.explicit_clip_request_id != ownership.move_request_id)
                || (controller.explicit_clip_request_id == ownership.move_request_id
                    && (controller.animation_clip_asset_key != clip_key
                        || controller.playback_speed_permille != rate)))
        {
            ownership.move_request_id = None;
        }
        if moving && !task_animation && ownership.move_request_id.is_none() {
            let request = playback_requests.write(AnimationClipPlaybackRequest {
                animation_subject_entity: controller_entity,
                animation_clip_asset_key: clip_key.clone(),
                blend_duration_milliseconds: 0,
                playback_speed_permille: rate,
                playback_repetition_policy: AnimationPlaybackRepetitionPolicy::Loop,
            });
            ownership.move_request_id = Some(request.id);
            ownership.requested_generation = controller.playback_generation;
            commands
                .entity(controller_entity)
                .insert(ConsumedLocomotionRootMotion::new(
                    request.id,
                    actor_from_root_parent,
                ));
        } else if !moving && ownership.move_request_id.take().is_some() {
            commands
                .entity(controller_entity)
                .remove::<ConsumedLocomotionRootMotion>();
            if !task_animation {
                let idle = water
                    .map(|water| &water.stop)
                    .or(variant.initial_animation_clip_asset_key.as_ref());
                if let Some(idle) = idle {
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
                && player
                    .playing_animations()
                    .any(|(_, active)| !active.is_paused() && !active.is_finished())
            {
                default_speed * f32::from(controller.playback_speed_permille) / 1000.0
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
            capabilities: NavFlags(
                NavFlags::ANIMAL.0
                    | if variant.surface_water_locomotion.is_some() {
                        NavFlags::AMPHIBIOUS.0
                    } else {
                        0
                    },
            ),
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
