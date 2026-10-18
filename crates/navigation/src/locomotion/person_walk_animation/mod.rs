//! Guests and staff move kinematically at their authored speed, so nothing in
//! their motion selects a clip. Play the walk clip while they navigate and
//! return to standing when they stop, unless a behavior owns the clip.

use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;

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
    behavior_task_execution_types::PendingBehaviorAnimationClipCompletion,
    guests::guest_simulation_types::Guest,
    locomotion::locomotion_types::{Destination, DirectLocomotion},
    staff::staff_employment_types::Staff,
};

const WALK_CLIP: &str = "Walk_Ahead";
const STAND_CLIP: &str = "Stand_Idle";

use super::sampled_animation_motion_projection::LocomotionAnimationSource;
use openzt2_world_state::animation_playback::{
    animation_joint_target_types::AnimationJointTarget,
    locomotion_root_motion::{clip_motion_velocity, ConsumedLocomotionRootMotion},
};
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;

pub(super) fn project_person_motion_to_walk_animation(
    mut commands: Commands,
    animation_sets: Res<Assets<AnimationSetAsset>>,
    mut controllers: Query<(
        Entity,
        &AnimationPlaybackController,
        Option<&AnimationGraphEnabled>,
        Option<&mut ConsumedLocomotionRootMotion>,
    )>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    joints: Query<(&AnimationJointTarget, &ChildOf)>,
    transforms: Query<&GlobalTransform>,
    people: Query<
        (
            Has<Destination>,
            Has<DirectLocomotion>,
            Has<PendingBehaviorAnimationClipCompletion>,
            Option<&NavAgent>,
            Option<&LocomotionAnimationSource>,
        ),
        Or<(With<Guest>, With<Staff>)>,
    >,
    mut playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
) {
    for (controller_entity, controller, enabled, mut consumed) in &mut controllers {
        let Some(person) = parents
            .iter_ancestors(controller_entity)
            .find(|ancestor| people.contains(*ancestor))
        else {
            continue;
        };
        let Ok((navigating, direct, task_animation, agent, source)) = people.get(person) else {
            continue;
        };
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
        let Some(set) = animation_sets.get(&controller.animation_set_asset) else {
            continue;
        };
        let wanted = if moving { WALK_CLIP } else { STAND_CLIP };
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
                let clip = set.find_animation_clip_record_by_asset_key(WALK_CLIP)?;
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
                let rate = (agent?.max_speed_mps / speed * 1000.0)
                    .round()
                    .clamp(1.0, f32::from(i16::MAX)) as i16;
                Some((
                    actor.affine().inverse() * parent.affine(),
                    rate,
                    speed * f32::from(rate) / 1000.0,
                ))
            })
            .flatten();
        let rate = binding.map_or(1000, |(_, rate, _)| rate);
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
