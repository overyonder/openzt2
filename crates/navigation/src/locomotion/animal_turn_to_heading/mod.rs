//! Turns an animal on the spot with its turning clips, as `zt.exe`'s
//! locomotion does for a dock or a target to face (`FUN_0061ded7`), rather
//! than setting its heading at once.

use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_world_state::animation_graph::animation_graph_playback_message_types::{
    AnimationClipPlaybackRequest, AnimationClipPlaybackRequestRejected,
};
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_event_message_types::AnimationCompleted;
use openzt2_world_state::animation_playback::animation_joint_target_types::AnimationJointTarget;
use openzt2_world_state::animation_playback::animation_playback_controller_types::{
    AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
};
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;
use openzt2_zoo_state::locomotion::locomotion_types::TurnToHeading;

#[cfg(test)]
mod tests;

/// One turning clip being played towards a `TurnToHeading`.
#[derive(Component, Debug, Clone, Copy)]
pub(super) struct TurnStep {
    request_id: usize,
    /// The animal could not play it, as when it has no ready presentation;
    /// it turns at a steady rate instead of asking again each step.
    refused: bool,
}

/// Larger turns take a quarter turn clip, smaller ones the shuffle.
const QUARTER_TURN_THRESHOLD_RADIANS: f32 = std::f32::consts::FRAC_PI_4;

/// How fast an animal without turning clips turns, so it never snaps round.
const FALLBACK_TURN_RATE_RADIANS_PER_SECOND: f32 = std::f32::consts::FRAC_PI_2;

/// The clip that turns an animal by `error` radians, positive to its left,
/// or none once it is within `tolerance`.
fn turn_clip(error: f32, tolerance: f32) -> Option<&'static str> {
    if error.abs() <= tolerance {
        None
    } else if error > QUARTER_TURN_THRESHOLD_RADIANS {
        Some("Stand_TurnLeft90")
    } else if error < -QUARTER_TURN_THRESHOLD_RADIANS {
        Some("Stand_TurnRight90")
    } else {
        Some("Stand_ShuffleGeneric")
    }
}

/// The signed angle from a heading of `rotation` to `heading` about +Y, from
/// +Z, which actor models face.
fn heading_error(rotation: Quat, heading: f32) -> f32 {
    let current = rotation * Vec3::Z;
    let current = current.x.atan2(current.z);
    (heading - current + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI
}

/// Plays turning clips until each turning animal faces its heading within
/// its tolerance, then removes its `TurnToHeading`. Each clip's root motion
/// turns the animal; one without the clips turns at a steady rate.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn turn_animals_to_their_headings(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut animation_sets: ResMut<Assets<AnimationSetAsset>>,
    asset_server: Res<AssetServer>,
    mut playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
    mut animals: Query<(
        Entity,
        &TurnToHeading,
        &mut Transform,
        Option<&AnimationPlaybackController>,
        Option<&TurnStep>,
    )>,
    transforms: Query<&GlobalTransform>,
    children: Query<&Children>,
    joints: Query<(&AnimationJointTarget, &ChildOf)>,
) {
    for (animal, turn, mut transform, controller, step) in &mut animals {
        let refused = step.is_some_and(|step| step.refused);
        // A step still turning goes on; one another clip replaced is over.
        if step.is_some_and(|step| {
            !step.refused
                && controller.is_some_and(|controller| {
                    controller.explicit_clip_request_id == Some(step.request_id)
                })
        }) {
            continue;
        }
        let error = heading_error(transform.rotation, turn.heading_radians);
        let Some(clip_key) = turn_clip(error, turn.tolerance_radians) else {
            commands
                .entity(animal)
                .remove::<(TurnToHeading, TurnStep)>();
            continue;
        };
        let root = controller.filter(|_| !refused).and_then(|controller| {
            Some(
                animation_sets
                    .get(&controller.animation_set_asset)?
                    .find_animation_clip_record_by_asset_key(clip_key)?
                    .authored_animation_clip_metadata
                    .as_ref()?
                    .root_motion_joint
                    .as_ref()?
                    .joint_asset_key
                    .clone(),
            )
        });
        let Some(root) = root else {
            // Start loading the clip a later step can play; touching the set
            // mutably marks it changed, so only to load. Meanwhile, and for an
            // animal without it, turn at a steady rate.
            let unrequested = controller.filter(|controller| {
                animation_sets
                    .get(&controller.animation_set_asset)
                    .is_some_and(|set| {
                        set.find_animation_clip_record_by_asset_key(clip_key)
                            .is_none()
                            && !set.animation_clip_asset_load_is_pending(clip_key)
                            && !set.animation_clip_asset_load_has_failed(clip_key)
                    })
            });
            if let Some(mut set) = unrequested
                .and_then(|controller| animation_sets.get_mut(&controller.animation_set_asset))
            {
                set.request_animation_clip_asset_load(&asset_server, clip_key);
            }
            let step = FALLBACK_TURN_RATE_RADIANS_PER_SECOND * time.delta_secs();
            transform.rotation =
                Quat::from_rotation_y(error.clamp(-step, step)) * transform.rotation;
            continue;
        };
        // The root joint's parent places its motion in the animal's space, as
        // for walking.
        let actor_from_root_parent = std::iter::once(animal)
            .chain(children.iter_descendants_depth_first::<Children>(animal))
            .find_map(|entity| {
                let (binding, parent) = joints.get(entity).ok()?;
                (binding.animation_playback_controller_entity == animal
                    && binding.joint_asset_key.eq_ignore_ascii_case(&root))
                .then(|| transforms.get(parent.parent()).ok())
                .flatten()
            })
            .zip(transforms.get(animal).ok())
            .map_or(bevy::math::Affine3A::IDENTITY, |(parent, actor)| {
                actor.affine().inverse() * parent.affine()
            });
        let request = playback_requests.write(AnimationClipPlaybackRequest {
            animation_subject_entity: animal,
            animation_clip_asset_key: clip_key.to_owned(),
            blend_duration_milliseconds: 0,
            playback_speed_permille: 1000,
            playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
        });
        commands.entity(animal).insert((
            TurnStep {
                request_id: request.id,
                refused: false,
            },
            ConsumedLocomotionRootMotion::new(request.id, actor_from_root_parent),
        ));
    }
}

/// Moves a turning animal by its turning clip's root motion, which the
/// animation keeps off its root joint.
pub(super) fn apply_turn_step_motion(
    mut animals: Query<
        (
            &mut Transform,
            &AnimationPlaybackController,
            &mut ConsumedLocomotionRootMotion,
        ),
        With<TurnStep>,
    >,
) {
    for (mut transform, controller, mut motion) in &mut animals {
        if !motion.owns_playback(controller) {
            continue;
        }
        let (yaw, _, _) = motion.rotation().to_euler(EulerRot::YXZ);
        let step = transform.rotation * (transform.scale * motion.translation());
        transform.translation += Vec3::new(step.x, 0.0, step.z);
        transform.rotation *= Quat::from_rotation_y(yaw);
        motion.clear_sample();
    }
}

/// Ends a turning step when its clip completes, so the next one measures the
/// heading again, or marks it refused.
pub(super) fn end_turn_steps_after_their_clips(
    mut commands: Commands,
    mut completed: MessageReader<AnimationCompleted>,
    mut rejected: MessageReader<AnimationClipPlaybackRequestRejected>,
    owners: Query<&AnimationPresentationOwner>,
    mut steps: Query<&mut TurnStep>,
) {
    for completed in completed.read() {
        let animal = owners
            .get(completed.animation_playback_controller_entity)
            .map_or(completed.animation_playback_controller_entity, |owner| {
                owner.gameplay_entity
            });
        if steps
            .get(animal)
            .is_ok_and(|step| Some(step.request_id) == completed.explicit_clip_request_id)
        {
            commands.entity(animal).remove::<TurnStep>();
        }
    }
    for rejected in rejected.read() {
        if let Ok(mut step) = steps.get_mut(rejected.animation_subject_entity) {
            if step.request_id == rejected.request_id {
                step.refused = true;
            }
        }
    }
}
