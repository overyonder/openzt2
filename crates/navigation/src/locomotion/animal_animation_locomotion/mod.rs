//! Source-backed default ground locomotion through the existing animation owner.

use super::locomotion_clip_dependencies::{
    GaitClipBinding, GaitRigJoints, LocomotionClipDependencies,
};
use bevy::prelude::*;
use openzt2_game_assets::animation::animation_clip_source_queries::AnimationClipSources;
use openzt2_game_assets::animation::animation_set_asset_types::{
    AnimationClipSourceAsset, AnimationSetAsset,
};
use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::behavior::action::wander::BehaviorGroundLocomotionSpeed;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest;
use openzt2_world_state::animation_graph::animation_graph_transition_operations::select_preferred_animation_clip_from_graph_node;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::{
    running_actions::BehaviorLocomotionIntent, PendingBehaviorTaskFailure,
};
use openzt2_zoo_state::locomotion::locomotion_types::CaffeineFlags;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::DirectLocomotion;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;

/// Identifies only navigation created here; other agent owners are untouched.
#[derive(Component, Clone)]
pub(super) struct AnimalNavigationAnimationOwner {
    move_request_id: Option<usize>,
    requested_generation: u64,
    rate_source: Option<openzt2_game_data::AssetId>,
    playback_speed_permille: i16,
    rate_reset: Timer,
}

#[derive(Component)]
struct AnimalAnimationProjectionPending(&'static str);

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AnimalLocomotionRuntime<'w, 's> {
    fixed_time: Res<'w, Time<Fixed>>,
    clip_assets: Res<'w, Assets<AnimationClipSourceAsset>>,
    random_streams: Query<'w, 's, &'static mut openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream>,
    unprojected: Query<'w, 's, &'static mut AnimalAnimationProjectionPending>,
    caffeine_flags: Query<'w, 's, &'static CaffeineFlags>,
    binders: Query<'w, 's, &'static DefinitionId>,
    definitions: Option<Res<'w, WorldDefinitions>>,
    definition_assets: Option<Res<'w, Assets<WorldDefinitionAsset>>>,
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
            Option<&GaitRigJoints>,
            Option<&mut ConsumedLocomotionRootMotion>,
            Option<&mut LocomotionClipDependencies>,
            Option<&mut GaitClipBinding>,
        ),
        With<Animal>,
    >,
    parents: Query<&ChildOf>,
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
            openzt2_world_state::world_spawn::world_entity_crating::UncratedWorldEntity,
        ),
    >,
    mut playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
    runtime: AnimalLocomotionRuntime,
) {
    let AnimalLocomotionRuntime {
        fixed_time,
        clip_assets,
        mut random_streams,
        mut unprojected,
        caffeine_flags,
        binders,
        definitions,
        definition_assets,
    } = runtime;
    let catalogue = definitions
        .as_deref()
        .zip(definition_assets.as_deref())
        .and_then(|(index, assets)| index.get(assets));
    let diagnostics = bevy::log::tracing::enabled!(bevy::log::Level::DEBUG);
    let mut wait = |commands: &mut Commands,
                    animal: Entity,
                    agent: Option<&mut Mut<NavAgent>>,
                    reason: &'static str| {
        let has_agent = agent.is_some();
        if let Some(agent) = agent {
            agent
                .reborrow()
                .map_unchanged(|agent| &mut agent.max_speed_mps)
                .set_if_neq(0.0);
        }
        if !diagnostics || has_agent {
            return;
        }
        if let Ok(mut current) = unprojected.get_mut(animal) {
            if !current
                .reborrow()
                .map_unchanged(|current| &mut current.0)
                .set_if_neq(reason)
            {
                return;
            }
        } else {
            commands
                .entity(animal)
                .insert(AnimalAnimationProjectionPending(reason));
        }
        debug!(?animal, reason, "animal has no navigation agent yet");
    };
    let species = species_index.get(&species_assets);
    for (animal, _, mut agent, owner, ..) in &mut animals {
        if agent.is_some() && owner.is_none() {
            continue;
        }
        if species.is_none() {
            wait(
                &mut commands,
                animal,
                agent.as_mut(),
                "species definitions are not loaded",
            );
        } else if !controllers.contains(animal) {
            wait(
                &mut commands,
                animal,
                agent.as_mut(),
                "no animation controller is attached to its model",
            );
        }
    }
    let Some(species) = species else {
        return;
    };
    for (controller_entity, controller, player, rig, mut consumed, dependencies, binding) in
        &mut controllers
    {
        let Ok((
            animal_entity,
            selected_variant,
            mut agent,
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
        let task_animation =
            task_animation || held_animation || docking_poses.contains(animal_entity);
        if agent.is_some() && owner.is_none() {
            continue;
        }
        if mode.is_some_and(|mode| !matches!(*mode, LocomotionMode::Ground | LocomotionMode::Swim))
        {
            wait(
                &mut commands,
                animal_entity,
                agent.as_mut(),
                "its locomotion mode is not ground",
            );
            continue;
        }
        let Some(variant) = species.find_variant(selected_variant.0) else {
            wait(
                &mut commands,
                animal_entity,
                agent.as_mut(),
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
            openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation(animal_entity, &mut commands);
        }
        let (Some(radius), Some(locomotion_key)) = (
            variant
                .navigation_collision_policy
                .as_ref()
                .map(|policy| policy.radius_m),
            locomotion_key,
        ) else {
            wait(
                &mut commands,
                animal_entity,
                agent.as_mut(),
                "its variant has no collision radius or ground clip",
            );
            continue;
        };
        if !radius.is_finite() || radius < 0.0 {
            wait(
                &mut commands,
                animal_entity,
                agent.as_mut(),
                "its collision radius is invalid",
            );
            continue;
        }
        let Some(set) = animations.get(&controller.animation_set_asset) else {
            wait(
                &mut commands,
                animal_entity,
                agent.as_mut(),
                "its animation set is not loaded",
            );
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
            animations.is_changed(),
        );
        let resolved = GaitClipBinding::resolve(
            &mut commands,
            controller_entity,
            binding,
            controller.animation_set_asset.id(),
            locomotion_key,
            false,
            animations.is_changed() || clip_assets.is_changed(),
            set,
            &clip_sources,
            || {
                let ready = |key: &str| clip_sources.get(set, key).is_some();
                let pending = |key: &str| {
                    !ready(key)
                        && clip_sources
                            .retained_handle(set, key)
                            .is_some_and(|handle| !clip_sources.failed(&handle))
                };
                let Some(node) = set
                    .animation_graph_nodes
                    .iter()
                    .find(|node| node.animation_graph_node_asset_key == *locomotion_key)
                else {
                    return Ok(locomotion_key.clone());
                };
                let selected = select_preferred_animation_clip_from_graph_node(set, ready, node);
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
                    return Err("its ground clips are still loading");
                }
                selected.ok_or("its ground graph node selects no loaded clip")
            },
        );
        let clip = match resolved {
            Ok(clip) => clip,
            Err(reason) => {
                wait(&mut commands, animal_entity, agent.as_mut(), reason);
                continue;
            }
        };
        let clip_key = clip.key.as_ref();
        let Some(joint) = clip.joint else {
            wait(
                &mut commands,
                animal_entity,
                agent.as_mut(),
                "its ground clip has no root motion joint",
            );
            continue;
        };
        let parent = rig.and_then(|rig| rig.root_parent(joint, &parents, &transforms));
        let Some(parent) = parent else {
            wait(
                &mut commands,
                animal_entity,
                agent.as_mut(),
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
        if let Some(consumed) = consumed.as_deref_mut() {
            consumed.update_root_parent_transform(actor_from_root_parent);
        }
        let Some(default_velocity) = clip.velocity(parent) else {
            wait(
                &mut commands,
                animal_entity,
                agent.as_mut(),
                "its ground clip motion cannot be measured",
            );
            continue;
        };
        let default_speed = default_velocity.length();
        if !default_speed.is_finite() || default_speed <= 0.0 {
            wait(
                &mut commands,
                animal_entity,
                agent.as_mut(),
                "its ground clip does not move",
            );
            continue;
        }
        let mut ownership = owner
            .as_deref()
            .cloned()
            .unwrap_or(AnimalNavigationAnimationOwner {
                move_request_id: None,
                requested_generation: controller.playback_generation,
                rate_source: None,
                playback_speed_permille: 1000,
                rate_reset: Timer::default(),
            });
        let rate_source = water_row.map(|row| {
            openzt2_game_data::AssetId::from_key(&format!(
                "{}:{:?}",
                row.node,
                movement.unwrap_or_default()
            ))
        });
        let rate_due =
            rate_source.is_some() && ownership.rate_reset.tick(fixed_time.delta()).is_finished();
        if rate_source != ownership.rate_source || rate_due {
            if let Some(row) = water_row {
                let Ok(mut random) = random_streams.get_mut(animal_entity) else {
                    wait(
                        &mut commands,
                        animal_entity,
                        agent.as_mut(),
                        "its locomotion random stream is unavailable",
                    );
                    continue;
                };
                let rate = row.playback_speed_range[0]
                    + random.0.unit_f32()
                        * (row.playback_speed_range[1] - row.playback_speed_range[0]);
                ownership.playback_speed_permille =
                    (rate * 1000.0).round().clamp(1.0, f32::from(i16::MAX)) as i16;
                let reset_seconds = row.reset_seconds_range[0]
                    + random.0.unit_f32()
                        * (row.reset_seconds_range[1] - row.reset_seconds_range[0]);
                ownership.rate_reset = super::fixed_interval_timer(reset_seconds);
            } else {
                ownership.playback_speed_permille = 1000;
            }
            ownership.rate_source = rate_source;
        }
        let moving = navigating || direct;
        // Movement follows the clip's root motion, so scaling its rate speeds
        // the animal up or slows it down.
        let rate = caffeine_flags
            .get(animal_entity)
            .copied()
            .unwrap_or_default()
            .apply_to_rate(
                f32::from(ownership.playback_speed_permille),
                super::authored_caffeine_factors(
                    binders
                        .get(animal_entity)
                        .ok()
                        .and_then(|binder| catalogue?.find_object(binder.0)),
                ),
            )
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
                animation_clip_asset_key: clip_key.to_owned(),
                blend_duration_milliseconds: 0,
                playback_speed_permille: rate,
                playback_repetition_policy: AnimationPlaybackRepetitionPolicy::Loop,
                action_settings: None,
            });
            ownership.move_request_id = Some(request.id);
            ownership.requested_generation = controller.playback_generation;
            let motion = ConsumedLocomotionRootMotion::new(request.id, actor_from_root_parent);
            if let Some(mut current) = consumed {
                *current = motion;
            } else {
                commands.entity(controller_entity).insert(motion);
            }
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
                        action_settings: None,
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
            agent.set_if_neq(projected);
        } else {
            commands
                .entity(animal_entity)
                .insert(projected)
                .remove::<AnimalAnimationProjectionPending>();
        }
        if let Some(mut owner) = owner {
            *owner = ownership;
        } else {
            commands.entity(animal_entity).insert(ownership);
        }
    }
}
