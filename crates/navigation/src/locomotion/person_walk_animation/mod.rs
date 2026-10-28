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

#[derive(Component)]
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
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;

pub(super) fn project_person_motion_to_walk_animation(
    mut commands: Commands,
    mut assets: PersonAnimationAssets,
    mut controllers: Query<(
        Entity,
        &AnimationPlaybackController,
        Option<&AnimationGraphEnabled>,
        Option<&mut ConsumedLocomotionRootMotion>,
        Option<&mut PersonGaitPlayback>,
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
) {
    for (controller_entity, controller, enabled, mut consumed, mut gait) in &mut controllers {
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
        let speed = task.and_then(|task| assets.documents.as_ref()?.get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, task)))
            .and_then(|action| match action {
                BehaviorAction::Wander(action) => Some(action.locomotion_speed),
                BehaviorAction::Evasion(action) => Some(action.locomotion_speed),
                BehaviorAction::Pursuit(action) => Some(action.locomotion_speed),
                BehaviorAction::Move(action) => Some(match action.locomotion_speed {
                    Some(openzt2_game_data::behavior::action::movement::BehaviorMoveLocomotionSpeed::Medium) => BehaviorGroundLocomotionSpeed::Medium,
                    Some(openzt2_game_data::behavior::action::movement::BehaviorMoveLocomotionSpeed::Fast) => BehaviorGroundLocomotionSpeed::Fast,
                    _ => BehaviorGroundLocomotionSpeed::Slow,
                }),
                _ => None,
            }).unwrap_or_default();
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
        let speed_key = AssetId::from_key(if !moving {
            "stop"
        } else {
            match speed {
                BehaviorGroundLocomotionSpeed::Slow => "slow",
                BehaviorGroundLocomotionSpeed::Medium => "medium",
                BehaviorGroundLocomotionSpeed::Fast => "fast",
                BehaviorGroundLocomotionSpeed::Glide => "glide",
                BehaviorGroundLocomotionSpeed::Clean => "clean",
            }
        });
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
            if current.binder != table.binder
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
                let parent = std::iter::once(controller_entity)
                    .chain(children.iter_descendants_depth_first::<Children>(controller_entity))
                    .find_map(|entity| {
                        let (binding, parent) = joints.get(entity).ok()?;
                        (binding.animation_playback_controller_entity == controller_entity
                            && binding
                                .joint_asset_key
                                .eq_ignore_ascii_case(&root.joint_asset_key))
                        .then(|| transforms.get(parent.parent()).ok())
                        .flatten()
                    })?;
                let actor = transforms.get(person).ok()?;
                let speed = clip_motion_velocity(clip, parent, 1000)?.length();
                if speed <= 0.0 {
                    return None;
                }
                let rate = if let Some(rate) = sampled_rate {
                    rate
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
