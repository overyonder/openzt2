use bevy::{ecs::system::SystemParam, prelude::*, transform::helper::TransformHelper};
use openzt2_game_assets::{
    animation::animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
    world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{
    world_definitions::animation_binder_rig::{
        GroundFitRig, GroundFitSupport, QuadrupedLegFit, SpineBendRig,
    },
    AssetId,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::{
    animation_graph::{
        animation_graph_playback_message_types::ActiveAnimationClipOptions,
        animation_presentation_relationship_types::AnimationPresentationOwner,
    },
    animation_playback::{
        animation_joint_target_types::{AnimationJointTarget, AnimationRigJoints},
        animation_playback_controller_types::{
            AnimationGroundFitControls, AnimationPlaybackController, RetainedAnimationClipSource,
        },
    },
};
use openzt2_zoo_state::{
    locomotion::locomotion_types::{LocomotionMode, SwimLayer},
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
};

use super::authored_binder_rig_binding::BoundAnimationBinderRig;

#[derive(Component, Default)]
pub(super) struct GroundFitPose {
    offsets: Vec<JointPoseOffset>,
    correction: Vec3,
    height_offset_disabled_seconds: Option<f32>,
    spine: SpineBendPose,
}

struct JointPoseOffset {
    joint: Entity,
    translation: Vec3,
    rotation: Quat,
}

#[derive(Default)]
struct SpineBendPose {
    sample: Option<Vec3>,
    previous_sample: Option<Vec3>,
    sample_elapsed: f32,
    curve: Vec3,
}

pub(super) fn clear_procedural_ground_fit_pose(
    mut poses: Query<&mut GroundFitPose>,
    mut joints: Query<&mut Transform>,
) {
    for mut pose in &mut poses {
        for offset in pose.offsets.drain(..).rev() {
            if let Ok(mut joint) = joints.get_mut(offset.joint) {
                joint.translation -= offset.translation;
                joint.rotation = offset.rotation.inverse() * joint.rotation;
            }
        }
    }
}

#[derive(SystemParam)]
pub(super) struct GroundFitAssets<'w> {
    rigs: Res<'w, Assets<WorldDefinitionAsset>>,
    sets: Res<'w, Assets<AnimationSetAsset>>,
    clips: Res<'w, Assets<AnimationClipSourceAsset>>,
}

#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
pub(super) struct GroundFitController {
    owner: &'static AnimationPresentationOwner,
    playback: &'static AnimationPlaybackController,
    controls: &'static AnimationGroundFitControls,
    options: Option<&'static ActiveAnimationClipOptions>,
    clip: Option<&'static RetainedAnimationClipSource>,
    binding: &'static BoundAnimationBinderRig,
    joints: &'static AnimationRigJoints,
    pose: &'static mut GroundFitPose,
}

pub(super) fn apply_procedural_ground_fit_pose(
    mut controllers: Query<GroundFitController>,
    time: Res<Time>,
    assets: GroundFitAssets,
    terrain: DrySurfaceEligibility,
    locomotion: Query<(&LocomotionMode, Option<&SwimLayer>)>,
    members: Query<&WorldMember>,
    joints: Query<(&AnimationJointTarget, Option<&ChildOf>)>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    for mut controller in &mut controllers {
        let Some(rig) = controller
            .binding
            .source
            .and_then(|source| source.get(&assets.rigs))
        else {
            continue;
        };
        let enabled = controller
            .options
            .filter(|options| {
                Some(options.0.request_id) == controller.playback.explicit_clip_request_id
            })
            .and_then(|options| options.0.ground_fit)
            .unwrap_or(true);
        let active_policy = assets
            .sets
            .get(&controller.playback.animation_set_asset)
            .and_then(|set| {
                set.find_animation_clip_source_reference_by_asset_key(
                    &controller.playback.animation_clip_asset_key,
                )
            })
            .map(|clip| &clip.playback_policy);
        let lookup = |key| find_rig_joint(controller.joints, &joints, key);
        let mut pose = controller.pose.reborrow();
        {
            let snapshot = transforms.p0();
            let position = |entity| {
                snapshot
                    .compute_global_transform(entity)
                    .ok()
                    .map(|pose| pose.translation())
            };
            let Some(actor) = snapshot
                .compute_global_transform(controller.owner.gameplay_entity)
                .ok()
            else {
                continue;
            };
            let gameplay = controller.owner.gameplay_entity;
            let swim = locomotion.get(gameplay).ok().and_then(|(mode, layer)| {
                (*mode == LocomotionMode::Swim).then(|| layer.copied().unwrap_or_default())
            });
            let local_parent = |joint| {
                joints
                    .get(joint)
                    .ok()
                    .and_then(|(_, parent)| parent)
                    .and_then(|parent| snapshot.compute_global_transform(parent.parent()).ok())
            };
            let root = controller
                .clip
                .and_then(|clip| assets.clips.get(&clip.0))
                .and_then(|clip| {
                    clip.authored_animation_clip_metadata
                        .root_motion_joint
                        .as_ref()
                })
                .and_then(|root| {
                    lookup(AssetId::from_key(
                        &root.joint_asset_key.to_ascii_lowercase(),
                    ))
                })
                .or_else(|| {
                    controller.joints.joints().iter().copied().find(|&joint| {
                        joints.get(joint).is_ok_and(|(_, parent)| {
                            parent.is_none_or(|parent| {
                                !controller.joints.joints().contains(&parent.parent())
                            })
                        })
                    })
                });

            let quadruped = rig.quadruped_ground_fit.as_ref();
            let biped = rig.biped_ground_fit.as_ref();
            let selected = match (quadruped, biped) {
                (Some(_), Some(biped)) if !controller.controls.prefer_quadruped => Some(biped),
                (Some(quadruped), _) => Some(quadruped),
                (None, biped) => biped,
            };
            if let Some(((ground, root), member)) = selected
                .zip(root)
                .zip(members.get(gameplay).ok())
                .filter(|_| enabled)
            {
                let rotate = active_policy
                    .and_then(|policy| policy.ground_fit_rotation_is_enabled)
                    .unwrap_or(true);
                fit_ground_support(
                    ground,
                    root,
                    actor,
                    &terrain,
                    member.root,
                    &position,
                    lookup,
                    rotate,
                    controller.controls.quadruped_tilt_off,
                    controller.controls.leg_fit_enabled,
                    swim,
                    time.delta_secs(),
                    &mut pose,
                );
                if let Some(first) = pose
                    .offsets
                    .first_mut()
                    .filter(|offset| offset.joint == root)
                {
                    if let Some(parent) = local_parent(root) {
                        first.translation = parent
                            .affine()
                            .inverse()
                            .transform_vector3(first.translation);
                        first.rotation =
                            parent.rotation().inverse() * first.rotation * parent.rotation();
                    }
                }
            } else {
                pose.correction = Vec3::ZERO;
            }
            if let Some(spine) = rig.spine_bend.as_ref() {
                let bend_enabled = enabled
                    && active_policy
                        .and_then(|policy| policy.spine_bending_is_allowed)
                        .unwrap_or(true);
                fit_spine_bend(
                    spine,
                    actor,
                    &position,
                    lookup,
                    local_parent,
                    bend_enabled,
                    time.delta_secs(),
                    &mut pose,
                );
            }
        }
        let mut locals = transforms.p1();
        for offset in &pose.offsets {
            if let Ok(mut joint) = locals.get_mut(offset.joint) {
                joint.translation += offset.translation;
                joint.rotation = offset.rotation * joint.rotation;
            }
        }
    }
}

fn find_rig_joint(
    rig: &AnimationRigJoints,
    joints: &Query<(&AnimationJointTarget, Option<&ChildOf>)>,
    name: AssetId,
) -> Option<Entity> {
    rig.joints().iter().copied().find(|&joint| {
        joints
            .get(joint)
            .is_ok_and(|(target, _)| target.authored_joint_id == name)
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "the support solve borrows the sampled pose and writes one rig"
)]
fn fit_ground_support(
    rig: &GroundFitRig,
    root: Entity,
    actor: GlobalTransform,
    terrain: &DrySurfaceEligibility,
    world_root: Entity,
    position: &impl Fn(Entity) -> Option<Vec3>,
    joint: impl Fn(AssetId) -> Option<Entity>,
    rotate: bool,
    quadruped_tilt_off: Option<bool>,
    legs_enabled: Option<bool>,
    swim: Option<SwimLayer>,
    delta: f32,
    pose: &mut GroundFitPose,
) {
    if swim == Some(SwimLayer::Submerged) {
        pose.correction = Vec3::ZERO;
        return;
    }
    let center = actor.translation();
    let forward = (actor.rotation() * Vec3::Z).with_y(0.0).normalize_or_zero();
    let right = (actor.rotation() * Vec3::X).with_y(0.0).normalize_or_zero();
    let sample = |point: Vec3| {
        terrain
            .ground_height(world_root, point.xz())
            .map(|height| point.with_y(height))
    };
    let sample_water = |point: Vec3| {
        terrain
            .water_surface_height(world_root, point.xz())
            .map(|height| point.with_y(height))
    };
    let at = |name: Option<AssetId>| name.and_then(&joint).and_then(position);
    let supports = |points: [Option<AssetId>; 2]| {
        let [front, back] = points;
        at(back).zip(at(front))
    };
    let mid = |point: Vec3, radius: f32| {
        sample(point - right * radius).zip(sample(point + right * radius))
    };
    let tilt_off = match &rig.support {
        GroundFitSupport::Quadruped { .. } => quadruped_tilt_off.unwrap_or(rig.tilt_off),
        GroundFitSupport::Biped { .. } => rig.tilt_off,
    };
    let mut requested = Vec3::ZERO;
    let support = match (&rig.support, swim) {
        (_, Some(_)) => supports(rig.buoyancy)
            .and_then(|(back, front)| sample_water(back).zip(sample_water(front))),
        (
            GroundFitSupport::Quadruped {
                bicycle_points,
                foot_spacing,
                legs,
            },
            None,
        ) => supports(*bicycle_points).and_then(|(back, front)| {
            let side = |left: usize, right: usize| {
                let legs = &legs.as_ref()?.legs;
                at(legs[left].ankle)
                    .zip(at(legs[right].ankle))
                    .map(|(left, right)| left.xz().distance(right.xz()))
            };
            let radius = if *foot_spacing < 0.0 {
                side(0, 1).unwrap_or(0.0).max(side(2, 3).unwrap_or(0.0)) * 0.5
            } else {
                *foot_spacing * 0.5
            };
            let ((back_left, back_right), (front_left, front_right)) =
                mid(back, radius).zip(mid(front, radius))?;
            requested.z = slope(
                back_left.midpoint(front_left),
                back_right.midpoint(front_right),
            )
            .clamp(-rig.allowed_roll_radians, rig.allowed_roll_radians);
            Some((
                back_left
                    .midpoint(back_right)
                    .with_y(back_left.y.max(back_right.y)),
                front_left
                    .midpoint(front_right)
                    .with_y(front_left.y.max(front_right.y)),
            ))
        }),
        (GroundFitSupport::Biped { .. }, None) => None,
    };
    let Some(height) = support
        .map(|(back, front)| back.midpoint(front).y)
        .or_else(|| {
            if swim.is_none() {
                sample(center)
            } else {
                sample_water(center)
            }
            .map(|point| point.y)
        })
    else {
        pose.correction = Vec3::ZERO;
        return;
    };
    requested.x = height - center.y;
    let countdown = pose
        .height_offset_disabled_seconds
        .get_or_insert(rig.height_offset_disabled_seconds);
    *countdown = if tilt_off {
        0.5
    } else {
        (*countdown - delta).max(0.0)
    };
    if *countdown == 0.0 {
        requested.x += rig.height_offset * rig.height_offset_scale;
    }
    if let Some((back, front)) = support {
        requested.y = slope(back, front);
    }
    if !rotate {
        requested.y = 0.0;
    }
    if !rotate || tilt_off {
        requested.z = 0.0;
    }

    let limit = Vec3::from_array(rig.error_limits);
    let gain = Vec3::from_array(rig.gains).max(Vec3::ZERO);
    let steps = (delta * 60.0).ceil().clamp(1.0, 30.0) as u32;
    let gain = (gain * (delta / steps as f32)).min(Vec3::ONE);
    for _ in 0..steps {
        pose.correction += (requested - pose.correction).clamp(-limit, limit) * gain;
    }
    let translation = Vec3::Y * pose.correction.x;
    let rotation = Quat::from_scaled_axis(forward * pose.correction.z)
        * Quat::from_scaled_axis(-right * pose.correction.y);
    pose.offsets.push(JointPoseOffset {
        joint: root,
        translation,
        rotation,
    });

    if swim.is_some() {
        return;
    }
    match &rig.support {
        GroundFitSupport::Biped {
            ankles,
            ankle_axes,
            fit_radius,
        } if rotate => {
            let radius = fit_radius.max(0.01);
            for (ankle, axis) in ankles.iter().zip(ankle_axes) {
                let Some(ankle) = ankle.and_then(&joint) else {
                    continue;
                };
                let Some(point) = position(ankle) else {
                    continue;
                };
                let Some((low, high)) =
                    sample(point - forward * radius).zip(sample(point + forward * radius))
                else {
                    continue;
                };
                add_joint_rotation(
                    Some(ankle),
                    *axis,
                    slope(low, high) - pose.correction.y,
                    &mut pose.offsets,
                );
            }
        }
        GroundFitSupport::Quadruped {
            legs: Some(legs), ..
        } if legs_enabled.unwrap_or(legs.initially_enabled) => {
            let pivot = position(root).unwrap_or(center);
            let fitted = |point| pivot + translation + rotation * (point - pivot);
            fit_quadruped_legs(
                legs,
                &joint,
                position,
                fitted,
                sample,
                forward,
                &mut pose.offsets,
            );
        }
        _ => {}
    }
}

fn slope(low: Vec3, high: Vec3) -> f32 {
    (high.y - low.y).atan2(high.xz().distance(low.xz()))
}

fn fit_quadruped_legs(
    rig: &QuadrupedLegFit,
    joint: &impl Fn(AssetId) -> Option<Entity>,
    position: &impl Fn(Entity) -> Option<Vec3>,
    fitted: impl Fn(Vec3) -> Vec3,
    sample: impl Fn(Vec3) -> Option<Vec3>,
    forward: Vec3,
    offsets: &mut Vec<JointPoseOffset>,
) {
    for leg in &rig.legs {
        let Some(ankle) = leg.ankle.and_then(joint) else {
            continue;
        };
        let Some(point) = position(ankle).map(&fitted) else {
            continue;
        };
        let Some(contact) = sample(point) else {
            continue;
        };
        let hip = leg.hip.and_then(joint);
        let knee = leg.knee.and_then(joint);
        let offset = (contact.y - point.y).clamp(0.0, rig.maximum_raise);
        if let Some(knee_point) = knee.and_then(position).map(&fitted) {
            let lever = knee_point.distance(point).max(0.001);
            let angle = (offset / lever).min(1.0).asin();
            match leg.fit_mode {
                0 => {
                    add_joint_rotation(hip, leg.hip_axis, angle, offsets);
                    add_joint_rotation(knee, leg.knee_axis, -angle, offsets);
                }
                1 => add_joint_rotation(knee, leg.knee_axis, angle, offsets),
                _ => {}
            }
        }
        let span = 0.05;
        if let Some((low, high)) =
            sample(point - forward * span).zip(sample(point + forward * span))
        {
            add_joint_rotation(Some(ankle), leg.ankle_axis, slope(low, high), offsets);
        }
    }
}

fn add_joint_rotation(
    joint: Option<Entity>,
    axis: [f32; 3],
    angle: f32,
    offsets: &mut Vec<JointPoseOffset>,
) {
    if let Some(joint) = joint {
        let axis = Vec3::from_array(axis).normalize_or_zero();
        if axis != Vec3::ZERO && angle != 0.0 {
            offsets.push(JointPoseOffset {
                joint,
                translation: Vec3::ZERO,
                rotation: Quat::from_axis_angle(axis, angle),
            });
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "each spine needs its sampled root, joint transforms and accumulated pose"
)]
fn fit_spine_bend(
    rig: &SpineBendRig,
    actor: GlobalTransform,
    position: &impl Fn(Entity) -> Option<Vec3>,
    joint: impl Fn(AssetId) -> Option<Entity>,
    parent: impl Fn(Entity) -> Option<GlobalTransform>,
    enabled: bool,
    delta: f32,
    pose: &mut GroundFitPose,
) {
    const SAMPLE_SECONDS: f32 = 0.25;
    let now = actor.translation();
    let forward = (actor.rotation() * Vec3::Z).with_y(0.0).normalize_or_zero();
    let bend = &mut pose.spine;
    bend.sample_elapsed += delta;
    if bend.sample.is_none() || bend.sample_elapsed >= SAMPLE_SECONDS {
        bend.previous_sample = bend.sample.replace(now);
        bend.sample_elapsed = 0.0;
    }
    let Some(previous) = bend.previous_sample else {
        return;
    };
    let Some(base) = joint(rig.base).and_then(position) else {
        return;
    };
    let body_length = rig
        .spine
        .iter()
        .chain(&rig.tail)
        .filter_map(|&name| joint(name).and_then(position))
        .map(|point| point.distance(base))
        .fold(0.0_f32, f32::max);
    if body_length <= f32::EPSILON {
        return;
    }
    let radius = rig
        .bend_radius
        .unwrap_or_else(|| 2.0 * body_length / rig.maximum_bend_radians.max(0.001));
    let travel = (now - previous).with_y(0.0);
    let forward_distance = travel.dot(forward);
    let goal = if enabled && forward_distance > 0.0 {
        let distance_squared = travel.length_squared();
        if distance_squared > (radius * 2.0).powi(2) {
            -2.0 * (travel - forward * forward_distance) / distance_squared
        } else {
            bend.curve
        }
    } else {
        Vec3::ZERO
    };
    let change = goal - bend.curve;
    let blend = delta / (rig.blend_seconds * radius).max(0.001);
    bend.curve += change.clamp_length_max(blend);
    let curvature = bend.curve.length().min(radius.recip());
    let axis = forward.cross(bend.curve).normalize_or_zero();
    if axis == Vec3::ZERO || curvature <= 0.0 {
        return;
    }
    for chain in [&rig.spine, &rig.tail] {
        let mut previous = base;
        for &name in chain {
            let Some(joint) = joint(name) else {
                continue;
            };
            let Some(point) = position(joint) else {
                continue;
            };
            let distance = point.distance(previous);
            previous = point;
            let parent_axis =
                parent(joint).map_or(axis, |parent| parent.rotation().inverse() * axis);
            add_joint_rotation(
                Some(joint),
                parent_axis.to_array(),
                curvature * distance,
                &mut pose.offsets,
            );
        }
    }
}
