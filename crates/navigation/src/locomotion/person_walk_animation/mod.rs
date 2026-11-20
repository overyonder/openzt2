//! Projects person navigation onto animation. Staff use their authored gait
//! tables and sampled clip speeds; guests retain their configured walk speed.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{
    behavior::{action::wander::BehaviorGroundLocomotionSpeed, action_record::BehaviorAction},
    AssetId,
};
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_zoo_state::behavior_task_execution_types::{
    find_current_behavior_task_action, BehaviorTaskExecutionState,
};
use openzt2_zoo_state::locomotion::locomotion_types::{LocomotionMode, SwimLayer};
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;

use openzt2_world_state::{
    animation_graph::{
        animation_graph_playback_message_types::AnimationClipPlaybackRequest,
        animation_graph_playback_state_types::AnimationGraphEnabled,
    },
    animation_playback::animation_playback_controller_types::{
        AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
    },
};
use openzt2_zoo_state::{
    behavior_task_execution_types::{
        BehaviorHeldAnimationClip, PendingBehaviorAnimationClipCompletion,
    },
    guests::guest_simulation_types::Guest,
    locomotion::locomotion_types::{Destination, DirectLocomotion},
    staff::staff_employment_types::Staff,
};

const WALK_CLIP: &str = "Walk_Ahead";
const STAND_CLIP: &str = "Stand_Idle";

#[derive(Component, Clone)]
pub(super) struct PersonGaitPlayback {
    binder: AssetId,
    row: AssetId,
    node: String,
    rate: i16,
    reset_seconds_remaining: f32,
    rates: [f32; 2],
}

#[derive(SystemParam)]
pub(super) struct PersonAnimationAssets<'w> {
    sets: ResMut<'w, Assets<AnimationSetAsset>>,
    server: Option<Res<'w, AssetServer>>,
    definitions: Option<Res<'w, WorldDefinitions>>,
    definition_assets: Option<Res<'w, Assets<WorldDefinitionAsset>>>,
    documents: Option<Res<'w, Assets<BehaviorDocumentAsset>>>,
    time: Option<Res<'w, Time<Fixed>>>,
}

use super::sampled_animation_motion_projection::LocomotionAnimationSource;
use openzt2_world_state::animation_playback::{
    animation_joint_target_types::AnimationJointTarget,
    locomotion_root_motion::{clip_motion_velocity, ConsumedLocomotionRootMotion},
};
use openzt2_zoo_state::locomotion::locomotion_types::{CaffeineMode, NavAgent};

/// How long a walking person may stand still before the walk becomes a stand.
const WALK_STOP_GRACE_S: f32 = 0.25;

/// The time left before a paused walk turns into a stand.
#[derive(Component, Clone, Copy)]
pub(super) struct PersonStopGrace(f32);

/// Slower than this, a behavior clip's root drift is posture, not a walk.
const BEHAVIOR_CLIP_MOTION_MIN_MPS: f32 = 0.05;

/// The person whose behavior clip on `controller` walks, so the clip's root
/// motion moves the person instead of drawing the body away from it.
#[derive(Component, Clone, Copy)]
pub(super) struct BehaviorClipRootMotion {
    controller: Entity,
}

fn root_parent<'a>(
    controller: Entity,
    root: &str,
    children: &Query<&Children>,
    joints: &Query<(&AnimationJointTarget, &ChildOf)>,
    transforms: &'a Query<&GlobalTransform>,
) -> Option<&'a GlobalTransform> {
    std::iter::once(controller)
        .chain(children.iter_descendants_depth_first::<Children>(controller))
        .find_map(|entity| {
            let (binding, parent) = joints.get(entity).ok()?;
            (binding.animation_playback_controller_entity == controller
                && binding.joint_asset_key.eq_ignore_ascii_case(root))
            .then(|| transforms.get(parent.parent()).ok())
            .flatten()
        })
}

fn root_parent_in_actor(
    controller: Entity,
    actor: Entity,
    root: &str,
    children: &Query<&Children>,
    joints: &Query<(&AnimationJointTarget, &ChildOf)>,
    transforms: &Query<&GlobalTransform>,
) -> Option<bevy::math::Affine3A> {
    let parent = root_parent(controller, root, children, joints, transforms)?;
    Some(transforms.get(actor).ok()?.affine().inverse() * parent.affine())
}

/// Moves a person by the root motion of the walking behavior clip it plays.
pub(super) fn apply_behavior_clip_root_motion(
    mut commands: Commands,
    mut people: Query<
        (Entity, &BehaviorClipRootMotion, &mut Transform),
        (Without<Destination>, Without<DirectLocomotion>),
    >,
    motions: Query<(&ConsumedLocomotionRootMotion, &AnimationPlaybackController)>,
) {
    for (person, source, mut transform) in &mut people {
        let Some((motion, _)) = motions
            .get(source.controller)
            .ok()
            .filter(|(motion, controller)| motion.owns_playback(controller))
        else {
            commands.entity(person).remove::<BehaviorClipRootMotion>();
            continue;
        };
        let step = transform.rotation * (transform.scale * motion.translation());
        transform.translation.x += step.x;
        transform.translation.z += step.z;
        transform.rotation = (transform.rotation * motion.rotation()).normalize();
    }
}

pub(super) fn project_person_motion_to_walk_animation(
    mut commands: Commands,
    mut assets: PersonAnimationAssets,
    mut controllers: Query<(
        Entity,
        &AnimationPlaybackController,
        Option<&AnimationGraphEnabled>,
        Option<&mut ConsumedLocomotionRootMotion>,
        Option<&mut PersonGaitPlayback>,
        Option<&PersonStopGrace>,
    )>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    joints: Query<(&AnimationJointTarget, &ChildOf)>,
    transforms: Query<&GlobalTransform>,
    mut people: Query<
        (
            Has<Destination>,
            Has<DirectLocomotion>,
            Has<PendingBehaviorAnimationClipCompletion>,
            Has<BehaviorHeldAnimationClip>,
            Option<&mut NavAgent>,
            Option<&LocomotionAnimationSource>,
            Option<&StaffRole>,
            Option<&LocomotionMode>,
            Option<&SwimLayer>,
            Option<&BehaviorTaskExecutionState>,
            Option<&mut BehaviorRandomStream>,
            Has<openzt2_zoo_state::staff::staff_employment_types::StaffClimbingTankGate>,
            Option<&openzt2_zoo_state::behavior_task_execution_types::BehaviorLocomotionModifier>,
        ),
        Or<(With<Guest>, With<Staff>)>,
    >,
    mut playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
    caffeine_modes: Query<&CaffeineMode>,
    mut shared_gaits: Local<bevy::platform::collections::HashMap<Entity, PersonGaitPlayback>>,
) {
    shared_gaits.clear();
    for (controller_entity, controller, enabled, mut consumed, mut gait, grace) in &mut controllers
    {
        let Some(person) = parents
            .iter_ancestors(controller_entity)
            .find(|ancestor| people.contains(*ancestor))
        else {
            continue;
        };
        let Ok((
            navigating,
            direct,
            awaited_clip,
            held_clip,
            mut agent,
            source,
            role,
            mode,
            layer,
            task,
            mut random,
            climbing,
            modifier,
        )) = people.get_mut(person)
        else {
            continue;
        };
        if climbing {
            continue;
        }
        let task_animation = awaited_clip || held_clip;
        let moving = navigating || direct;
        // A walk that pauses for a moment, such as between two tasks that
        // both walk, keeps its stride instead of flicking to a stand and back.
        if !moving
            && !task_animation
            && enabled.is_none_or(|enabled| enabled.0)
            && source.is_some_and(|source| source.controller == controller_entity)
        {
            let remaining = grace.map_or(WALK_STOP_GRACE_S, |grace| grace.0)
                - assets.time.as_ref().map_or(0.0, |time| time.delta_secs());
            if remaining > 0.0 {
                commands
                    .entity(controller_entity)
                    .insert(PersonStopGrace(remaining));
                continue;
            }
        }
        if grace.is_some() {
            commands
                .entity(controller_entity)
                .remove::<PersonStopGrace>();
        }
        if !moving || task_animation || enabled.is_some_and(|enabled| !enabled.0) {
            if source.is_some_and(|source| source.controller == controller_entity) {
                commands
                    .entity(person)
                    .remove::<LocomotionAnimationSource>();
                commands
                    .entity(controller_entity)
                    .remove::<ConsumedLocomotionRootMotion>();
            }
        }
        if task_animation && !moving && enabled.is_none_or(|enabled| enabled.0) {
            // A behavior clip that walks, such as stepping away from a
            // stand's counter with the food, moves the person itself.
            let parent = assets
                .sets
                .get(&controller.animation_set_asset)
                .and_then(|set| {
                    set.find_animation_clip_record_by_asset_key(
                        &controller.animation_clip_asset_key,
                    )
                })
                .filter(|clip| {
                    clip_motion_velocity(clip, &GlobalTransform::IDENTITY, 1000)
                        .is_some_and(|velocity| velocity.length() > BEHAVIOR_CLIP_MOTION_MIN_MPS)
                })
                .and_then(|clip| {
                    let root = clip
                        .authored_animation_clip_metadata
                        .as_ref()?
                        .root_motion_joint
                        .as_ref()?;
                    root_parent_in_actor(
                        controller_entity,
                        person,
                        &root.joint_asset_key,
                        &children,
                        &joints,
                        &transforms,
                    )
                });
            if let (Some(parent), Some(request)) = (parent, controller.explicit_clip_request_id) {
                match consumed.as_deref_mut() {
                    Some(motion) if motion.owns_playback(controller) => {
                        motion.update_root_parent_transform(parent);
                    }
                    _ => {
                        commands
                            .entity(controller_entity)
                            .insert(ConsumedLocomotionRootMotion::new(request, parent));
                    }
                }
                commands.entity(person).insert(BehaviorClipRootMotion {
                    controller: controller_entity,
                });
            }
        }
        if task_animation || enabled.is_some_and(|enabled| !enabled.0) {
            continue;
        }
        // Attached heads and props have independent controllers without these clips.
        let catalogue = assets
            .definitions
            .as_ref()
            .zip(assets.definition_assets.as_ref())
            .and_then(|(index, definitions)| index.get(definitions));
        let role_definition =
            role.and_then(|role| catalogue.and_then(|catalogue| catalogue.find_staff(role.0)));
        let speed = task
            .and_then(|task| {
                assets
                    .documents
                    .as_ref()?
                    .get(&task.document)
                    .and_then(|document| find_current_behavior_task_action(document, task))
            })
            .and_then(|action| match action {
                BehaviorAction::Wander(action) => Some(action.locomotion_speed),
                BehaviorAction::Evasion(action) => Some(action.locomotion_speed),
                BehaviorAction::Pursuit(action) => Some(action.locomotion_speed),
                BehaviorAction::Move(action) => Some(action.locomotion_speed.unwrap_or_default()),
                _ => None,
            })
            .unwrap_or_default();
        let binder = if mode == Some(&LocomotionMode::Swim) {
            if layer == Some(&SwimLayer::Submerged) {
                "underwater"
            } else {
                "water"
            }
        } else if modifier.is_some_and(|modifier| modifier.modifier == AssetId::from_key("carry")) {
            "groundcarry"
        } else {
            "ground"
        };
        let table = role_definition.and_then(|role| {
            role.locomotion_tables
                .iter()
                .find(|table| table.binder == AssetId::from_key(binder))
                .or_else(|| {
                    role.locomotion_tables
                        .iter()
                        .find(|table| table.binder == AssetId::from_key("ground"))
                })
        });
        let speed_key = if moving {
            speed.row_key()
        } else {
            BehaviorGroundLocomotionSpeed::Stop.row_key()
        };
        let row = table.and_then(|table| {
            table
                .rows
                .iter()
                .find(|row| row.key == speed_key)
                .or_else(|| table.rows.first())
        });
        let node_key =
            row.map(|row| row.node.as_str())
                .unwrap_or(if moving { WALK_CLIP } else { STAND_CLIP });
        let has_locomotion_node = assets
            .sets
            .get(&controller.animation_set_asset)
            .is_some_and(|set| {
                set.animation_graph_nodes
                    .iter()
                    .any(|node| node.animation_graph_node_asset_key == node_key)
                    || set
                        .find_animation_clip_source_reference_by_asset_key(node_key)
                        .is_some()
            });
        if !has_locomotion_node {
            continue;
        }
        if moving && row.is_some() {
            if let Some(agent) = agent.as_deref_mut() {
                agent.max_speed_mps = 0.0;
            }
        }
        let sampled_rate = if let (Some(table), Some(row)) = (table, row) {
            let mut new_gait = PersonGaitPlayback {
                binder: AssetId::default(),
                row: AssetId::default(),
                node: String::new(),
                rate: 1000,
                reset_seconds_remaining: 0.0,
                rates: [1.0; 2],
            };
            let newly_projected = gait.is_none();
            let current = gait.as_deref_mut().unwrap_or(&mut new_gait);
            current.reset_seconds_remaining -=
                assets.time.as_ref().map_or(0.0, |time| time.delta_secs());
            let shared = shared_gaits.get(&person).filter(|shared| {
                shared.binder == table.binder
                    && shared.row == row.key
                    && shared.node == row.node
                    && shared.rates == row.playback_speed_range
            });
            if let Some(shared) = shared {
                // Every body part of one person walks at the same rate, or
                // its legs fall out of step with its upper body.
                current.rate = shared.rate;
                current.reset_seconds_remaining = shared.reset_seconds_remaining;
                current.binder = shared.binder;
                current.row = shared.row;
                current.node.clone_from(&shared.node);
                current.rates = shared.rates;
            } else if current.binder != table.binder
                || current.row != row.key
                || current.node != row.node
                || current.rates != row.playback_speed_range
                || (row.reset_seconds_range[1] > 0.0 && current.reset_seconds_remaining <= 0.0)
            {
                let sample = random
                    .as_deref_mut()
                    .map_or(0.0, |random| random.0.unit_f32());
                let rate = row.playback_speed_range[0]
                    + sample * (row.playback_speed_range[1] - row.playback_speed_range[0]);
                current.rate = (rate * 1000.0).round().clamp(0.0, f32::from(i16::MAX)) as i16;
                let sample = random
                    .as_deref_mut()
                    .map_or(0.0, |random| random.0.unit_f32());
                current.reset_seconds_remaining = row.reset_seconds_range[0]
                    + sample * (row.reset_seconds_range[1] - row.reset_seconds_range[0]);
                current.binder = table.binder;
                current.row = row.key;
                current.node.clone_from(&row.node);
                current.rates = row.playback_speed_range;
            }
            let rate = current.rate;
            shared_gaits
                .entry(person)
                .or_insert_with(|| current.clone());
            if newly_projected {
                commands.entity(controller_entity).insert(new_gait);
            }
            Some(rate)
        } else {
            None
        };
        let pending = assets
            .sets
            .get(&controller.animation_set_asset)
            .map(|set| {
                let missing = |key: &str| {
                    set.find_animation_clip_record_by_asset_key(key).is_none()
                        && !set.animation_clip_asset_load_has_failed(key)
                        && !set.animation_clip_asset_load_is_pending(key)
                };
                set.animation_graph_nodes
                    .iter()
                    .find(|node| node.animation_graph_node_asset_key == node_key)
                    .map_or_else(
                        || {
                            if missing(node_key) {
                                vec![node_key.to_owned()]
                            } else {
                                Vec::new()
                            }
                        },
                        |node| {
                            node.animation_clip_asset_keys
                                .iter()
                                .filter(|key| missing(key))
                                .cloned()
                                .collect()
                        },
                    )
            })
            .unwrap_or_default();
        if !pending.is_empty() {
            if let Some(server) = assets.server.as_ref() {
                if let Some(mut set) = assets.sets.get_mut(&controller.animation_set_asset) {
                    for key in &pending {
                        set.request_animation_clip_asset_load(server, key);
                    }
                }
            }
        }
        let Some(set) = assets.sets.get(&controller.animation_set_asset) else {
            continue;
        };
        let selected = set.animation_graph_nodes.iter().find(|node| node.animation_graph_node_asset_key == node_key)
            .and_then(|node| openzt2_world_state::animation_graph::animation_graph_transition_operations::select_preferred_animation_clip_from_graph_node(set, node));
        let wanted = if !moving {
            table
                .and_then(|table| table.stationary_clip.as_deref())
                .or(selected.as_deref())
                .unwrap_or(node_key)
        } else {
            selected.as_deref().unwrap_or(node_key)
        };
        if set
            .find_animation_clip_source_reference_by_asset_key(wanted)
            .is_none()
            || set.animation_clip_asset_load_has_failed(wanted)
            || set.animation_clip_asset_load_is_pending(wanted)
        {
            continue;
        }
        let binding = moving
            .then(|| {
                let clip = set.find_animation_clip_record_by_asset_key(wanted)?;
                let root = clip
                    .authored_animation_clip_metadata
                    .as_ref()?
                    .root_motion_joint
                    .as_ref()?;
                let parent = root_parent(
                    controller_entity,
                    &root.joint_asset_key,
                    &children,
                    &joints,
                    &transforms,
                )?;
                let actor = transforms.get(person).ok()?;
                let speed = clip_motion_velocity(clip, parent, 1000)?.length();
                if speed <= 0.0 {
                    return None;
                }
                // A gait row's rate also sets the agent's speed below, so
                // scaling it speeds the person up or slows them down.
                // People without a gait row move at their
                // NavAgent speed and keep it; scale that too if one appears.
                let rate = if let Some(rate) = sampled_rate {
                    (f32::from(rate) * CaffeineMode::speed_factor(caffeine_modes.get(person).ok()))
                        .round()
                        .clamp(1.0, f32::from(i16::MAX)) as i16
                } else {
                    (agent.as_deref()?.max_speed_mps / speed * 1000.0)
                        .round()
                        .clamp(1.0, f32::from(i16::MAX)) as i16
                };
                Some((
                    actor.affine().inverse() * parent.affine(),
                    rate,
                    speed * f32::from(rate) / 1000.0,
                ))
            })
            .flatten();
        let rate = binding.map_or(1000, |(_, rate, _)| rate);
        if row.is_some() {
            if let Some(agent) = agent.as_deref_mut() {
                let active = controller.animation_clip_asset_key == wanted
                    && controller.playback_speed_permille == rate
                    && controller.playback_state == openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackState::Playing
                    && consumed.as_ref().is_some_and(|motion| motion.owns_playback(controller));
                agent.max_speed_mps = if active {
                    binding.map_or(0.0, |(_, _, speed)| speed)
                } else {
                    0.0
                };
            }
        }
        let needs_request = controller.animation_clip_asset_key != wanted
            || (moving
                && binding.is_some()
                && (controller.playback_speed_permille != rate
                    || consumed
                        .as_ref()
                        .is_none_or(|motion| !motion.owns_playback(controller))));
        if needs_request {
            let request = playback_requests.write(AnimationClipPlaybackRequest {
                animation_subject_entity: controller_entity,
                animation_clip_asset_key: wanted.to_owned(),
                blend_duration_milliseconds: 0,
                playback_speed_permille: rate,
                playback_repetition_policy: AnimationPlaybackRepetitionPolicy::Loop,
            });
            if let Some((parent, _, mean_speed_mps)) = binding {
                commands
                    .entity(controller_entity)
                    .insert(ConsumedLocomotionRootMotion::new(request.id, parent));
                commands.entity(person).insert(LocomotionAnimationSource {
                    controller: controller_entity,
                    mean_speed_mps,
                });
            }
        } else if let (Some((parent, _, mean_speed_mps)), Some(motion)) =
            (binding, consumed.as_deref_mut())
        {
            motion.update_root_parent_transform(parent);
            if source.is_none_or(|source| {
                source.controller != controller_entity || source.mean_speed_mps != mean_speed_mps
            }) {
                commands.entity(person).insert(LocomotionAnimationSource {
                    controller: controller_entity,
                    mean_speed_mps,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::animation::animation_set::AuthoredAnimationSetClipReference;
    use openzt2_world_state::animation_playback::animation_playback_controller_types::{
        AnimationPlaybackClock, AnimationPlaybackState,
    };

    #[test]
    fn a_walking_behavior_clip_moves_the_person_along_its_facing() {
        use bevy::animation::{animated_field, AnimationTargetId};
        let mut clip = AnimationClip::default();
        clip.add_curve_to_target(
            AnimationTargetId::from_name(&Name::new("bip01")),
            AnimatableCurve::new(
                animated_field!(Transform::translation),
                AnimatableKeyframeCurve::new([(0.0, Vec3::ZERO), (1.0, Vec3::Z * 2.0)]).unwrap(),
            ),
        );
        let mut motion = ConsumedLocomotionRootMotion::new(7, bevy::math::Affine3A::IDENTITY);
        motion.sample_interval(&clip, "Bip01", 0.0..0.5, 0, 1.0);
        let mut world = World::new();
        let controller = world
            .spawn((
                motion,
                AnimationPlaybackController {
                    playback_generation: 0,
                    explicit_clip_request_id: Some(7),
                    animation_set_asset: Handle::default(),
                    animation_clip_asset_key: "WalkItem_Ahead".to_owned(),
                    elapsed_milliseconds: 0,
                    fractional_milliseconds: default(),
                    playback_speed_permille: 1000,
                    playback_state: AnimationPlaybackState::Playing,
                    playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
                    playback_clock: AnimationPlaybackClock::Simulation,
                },
            ))
            .id();
        let person = world
            .spawn((
                Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                BehaviorClipRootMotion { controller },
            ))
            .id();
        let mut system = IntoSystem::into_system(apply_behavior_clip_root_motion);
        system.initialize(&mut world);
        system.run((), &mut world).unwrap();
        let moved = world.get::<Transform>(person).unwrap().translation;
        assert!(moved.abs_diff_eq(Vec3::X, 1e-5), "{moved:?}");
    }

    #[test]
    fn navigating_guest_walks_and_stopped_guest_stands() {
        let mut app = App::new();
        let mut sets = Assets::<AnimationSetAsset>::default();
        let set = sets.add(AnimationSetAsset {
            model_asset_path: String::new(),
            skeleton_asset_path: String::new(),
            animation_clips: Vec::new(),
            animation_graph_nodes: Vec::new(),
            animation_graph_order: Vec::new(),
            animation_graph_edges: Vec::new(),
            animation_graph_metadata: None,
            animation_clip_source_references: [WALK_CLIP, STAND_CLIP]
                .map(|key| AuthoredAnimationSetClipReference {
                    animation_clip_asset_key: key.to_owned(),
                    animation_clip_asset_path: String::new(),
                    playback_policy: default(),
                })
                .into(),
            pending_animation_clip_source_asset_handles: Vec::new(),
            failed_animation_clip_asset_keys: Vec::new(),
        });
        app.insert_resource(sets)
            .add_message::<AnimationClipPlaybackRequest>()
            .add_systems(Update, project_person_motion_to_walk_animation);
        let guest = app
            .world_mut()
            .spawn((
                Guest,
                Destination {
                    request_id: 1,
                    world: Vec3::X,
                    arrival_radius_m: 0.1,
                },
            ))
            .id();
        let controller = app
            .world_mut()
            .spawn((
                ChildOf(guest),
                AnimationPlaybackController {
                    playback_generation: 0,
                    explicit_clip_request_id: None,
                    animation_set_asset: set,
                    animation_clip_asset_key: STAND_CLIP.to_owned(),
                    elapsed_milliseconds: 0,
                    fractional_milliseconds: default(),
                    playback_speed_permille: 1000,
                    playback_state: AnimationPlaybackState::Playing,
                    playback_repetition_policy: AnimationPlaybackRepetitionPolicy::Loop,
                    playback_clock: AnimationPlaybackClock::Simulation,
                },
            ))
            .id();
        let requested = |app: &mut App| {
            app.update();
            app.world_mut()
                .resource_mut::<Messages<AnimationClipPlaybackRequest>>()
                .drain()
                .map(|request| request.animation_clip_asset_key)
                .collect::<Vec<_>>()
        };
        assert_eq!(requested(&mut app), [WALK_CLIP]);
        app.world_mut()
            .get_mut::<AnimationPlaybackController>(controller)
            .unwrap()
            .animation_clip_asset_key = WALK_CLIP.to_owned();
        assert!(requested(&mut app).is_empty());
        app.world_mut().entity_mut(guest).remove::<Destination>();
        assert_eq!(requested(&mut app), [STAND_CLIP]);
    }
}
