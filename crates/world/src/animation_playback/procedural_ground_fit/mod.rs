use bevy::{
    ecs::{
        entity::{EntityHashMap, EntityHashSet},
        system::SystemParam,
    },
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_game_assets::{
    animation::animation_set_asset_types::AnimationSetAsset,
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
            AnimationGroundFitControls, AnimationPlaybackController,
        },
    },
};
use openzt2_zoo_state::{
    locomotion::locomotion_types::{LocomotionMode, SwimLayer},
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
};
use std::cell::RefCell;

use super::authored_binder_rig_binding::BoundAnimationBinderRig;
use super::managed_clip_binding::{FixedAnimationPlayer, ManagedAnimationClip};
use openzt2_zoo_state::terrain::terrain_edit_types::{TerrainChanged, TerrainWaterChanged};

#[derive(Component, Default)]
pub(super) struct GroundFitPose {
    offsets: Vec<JointPoseOffset>,
    correction: Vec3,
    height_offset_delay: Option<Timer>,
    spine: SpineBendPose,
}

struct JointPoseOffset {
    joint: Entity,
    translation: Vec3,
    rotation: Quat,
}

struct SpineBendPose {
    sample: Option<Vec3>,
    previous_sample: Option<Vec3>,
    sample_timer: Timer,
    curve: Vec3,
}

impl Default for SpineBendPose {
    fn default() -> Self {
        Self {
            sample: None,
            previous_sample: None,
            sample_timer: Timer::from_seconds(0.25, TimerMode::Once),
            curve: Vec3::ZERO,
        }
    }
}

#[derive(Component, PartialEq, Eq)]
pub(super) struct GroundFitJointIndex {
    named: HashMap<AssetId, Entity>,
    root: Option<Entity>,
}

pub(super) fn index_ground_fit_joints(
    mut commands: Commands,
    mut controllers: Query<
        (
            Entity,
            Ref<AnimationRigJoints>,
            Option<&mut GroundFitJointIndex>,
        ),
        With<GroundFitPose>,
    >,
    targets: Query<(&AnimationJointTarget, Option<&ChildOf>)>,
    reparented: Query<&AnimationJointTarget, Or<(Changed<ChildOf>, Changed<AnimationJointTarget>)>>,
    mut detached: RemovedComponents<ChildOf>,
) {
    let affected = reparented
        .iter()
        .chain(
            detached
                .read()
                .filter_map(|entity| targets.get(entity).ok().map(|(target, _)| target)),
        )
        .map(|target| target.animation_playback_controller_entity)
        .collect::<EntityHashSet>();
    for (controller, joints, existing) in &mut controllers {
        if existing.is_some() && !joints.is_changed() && !affected.contains(&controller) {
            continue;
        }
        let entities = joints.joints().iter().copied().collect::<EntityHashSet>();
        let mut index = GroundFitJointIndex {
            named: HashMap::new(),
            root: None,
        };
        for &entity in joints.joints() {
            if let Ok((target, parent)) = targets.get(entity) {
                index
                    .named
                    .entry(target.authored_joint_id)
                    .or_insert(entity);
                if index.root.is_none()
                    && parent.is_none_or(|parent| !entities.contains(&parent.parent()))
                {
                    index.root = Some(entity);
                }
            }
        }
        if let Some(mut existing) = existing {
            existing.set_if_neq(index);
        } else {
            commands.entity(controller).insert(index);
        }
    }
}

pub(super) fn clear_procedural_ground_fit_pose(
    mut poses: Query<(Entity, &mut GroundFitPose)>,
    playable: Query<
        (),
        (
            With<AnimationPresentationOwner>,
            With<AnimationPlaybackController>,
            With<AnimationPlayer>,
            With<BoundAnimationBinderRig>,
            With<GroundFitJointIndex>,
        ),
    >,
    mut joints: Query<&mut Transform>,
) {
    for (entity, mut pose) in &mut poses {
        for offset in pose.offsets.iter().rev() {
            if let Ok(mut joint) = joints.get_mut(offset.joint) {
                let base = Transform {
                    translation: joint.translation - offset.translation,
                    rotation: offset.rotation.inverse() * joint.rotation,
                    scale: joint.scale,
                };
                joint.set_if_neq(base);
            }
        }
        if !pose.offsets.is_empty() && !playable.contains(entity) {
            pose.offsets.clear();
        }
    }
}

#[derive(SystemParam)]
pub(super) struct GroundFitAssets<'w> {
    rigs: Res<'w, Assets<WorldDefinitionAsset>>,
    sets: Res<'w, Assets<AnimationSetAsset>>,
}

#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
pub(super) struct GroundFitController {
    owner: Ref<'static, AnimationPresentationOwner>,
    playback: Ref<'static, AnimationPlaybackController>,
    controls: Ref<'static, AnimationGroundFitControls>,
    options: Option<Ref<'static, ActiveAnimationClipOptions>>,
    clip: Option<Ref<'static, ManagedAnimationClip>>,
    binding: Ref<'static, BoundAnimationBinderRig>,
    joints: Ref<'static, GroundFitJointIndex>,
    player: &'static AnimationPlayer,
    fixed: Has<FixedAnimationPlayer>,
    pose: &'static mut GroundFitPose,
}

pub(super) fn apply_procedural_ground_fit_pose(
    mut controllers: Query<GroundFitController>,
    time: Res<Time>,
    assets: GroundFitAssets,
    terrain: DrySurfaceEligibility,
    locomotion: Query<(Ref<LocomotionMode>, Option<Ref<SwimLayer>>)>,
    mut ground_edits: MessageReader<TerrainChanged>,
    mut water_edits: MessageReader<TerrainWaterChanged>,
    simulation: Query<
        &openzt2_world_state::simulation_time::simulation_control_types::SimulationPaused,
        With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>,
    >,
    members: Query<&WorldMember>,
    mut transforms: ParamSet<(
        Query<(Ref<Transform>, Option<Ref<ChildOf>>)>,
        Query<&mut Transform>,
    )>,
) {
    let terrain_changed = (ground_edits.read().count() != 0) | (water_edits.read().count() != 0);
    let simulation_running =
        openzt2_world_state::simulation_time::simulation_control_queries::simulation_is_running(
            simulation,
        );
    let locals = transforms.p0();
    controllers.par_iter_mut().for_each(|mut controller| {
        let gameplay = controller.owner.gameplay_entity;
        let animation_paused = controller.fixed && !simulation_running
            || controller.clip.as_ref().is_some_and(|clip| {
                controller
                    .player
                    .animation(clip.node)
                    .is_some_and(|active| active.is_paused())
            });
        let swim_input = locomotion.get(gameplay).ok();
        if animation_paused
            && !terrain_changed
            && !controller.owner.is_changed()
            && !controller.playback.is_changed()
            && !controller.controls.is_changed()
            && !controller.joints.is_changed()
            && !controller.binding.is_changed()
            && !controller.options.as_ref().is_some_and(Ref::is_changed)
            && !controller.clip.as_ref().is_some_and(Ref::is_changed)
            && !swim_input.as_ref().is_some_and(|(mode, layer)| {
                mode.is_changed() || layer.as_ref().is_some_and(Ref::is_changed)
            })
            && !locals.get(gameplay).is_ok_and(|(transform, parent)| {
                transform.is_changed() || parent.as_ref().is_some_and(Ref::is_changed)
            })
        {
            return;
        }
        controller.pose.offsets.clear();
        let Some(rig) = controller
            .binding
            .source
            .and_then(|source| source.get(&assets.rigs))
        else {
            return;
        };
        let enabled = controller
            .options
            .as_ref()
            .filter(|options| {
                Some(options.request_id) == controller.playback.explicit_clip_request_id
            })
            .and_then(|options| options.settings.ground_fit)
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
        let lookup = |key| controller.joints.named.get(&key).copied();
        let mut pose = controller.pose.reborrow();
        {
            let snapshots = RefCell::new(EntityHashMap::new());
            let world_pose =
                |entity| sample_ground_fit_world_pose(entity, &locals, &mut snapshots.borrow_mut());
            // Fit and bend share the unmodified animation pose.
            let position = |entity| world_pose(entity).map(|pose| pose.translation());
            let Some(actor) = world_pose(controller.owner.gameplay_entity) else {
                return;
            };
            let swim = swim_input.and_then(|(mode, layer)| {
                (*mode == LocomotionMode::Swim)
                    .then(|| layer.map_or_else(SwimLayer::default, |layer| *layer))
            });
            let local_parent = |joint| {
                locals
                    .get(joint)
                    .ok()
                    .and_then(|(_, parent)| world_pose(parent?.parent()))
            };
            let root = controller
                .clip
                .as_ref()
                .and_then(|clip| clip.root_motion.as_ref())
                .and_then(|root| lookup(root.joint))
                .or(controller.joints.root);
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
    });
    let mut locals = transforms.p1();
    for controller in &controllers {
        for offset in &controller.pose.offsets {
            if let Ok(mut joint) = locals.get_mut(offset.joint) {
                let next = Transform {
                    translation: joint.translation + offset.translation,
                    rotation: offset.rotation * joint.rotation,
                    scale: joint.scale,
                };
                joint.set_if_neq(next);
            }
        }
    }
}

fn sample_ground_fit_world_pose(
    entity: Entity,
    locals: &Query<(Ref<Transform>, Option<Ref<ChildOf>>)>,
    snapshots: &mut EntityHashMap<GlobalTransform>,
) -> Option<GlobalTransform> {
    if let Some(&pose) = snapshots.get(&entity) {
        return Some(pose);
    }
    let (local, parent) = locals.get(entity).ok()?;
    let pose = if let Some(parent) = parent {
        sample_ground_fit_world_pose(parent.parent(), locals, snapshots)?.mul_transform(*local)
    } else {
        GlobalTransform::from(*local)
    };
    snapshots.insert(entity, pose);
    Some(pose)
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
    let delay = pose.height_offset_delay.get_or_insert_with(|| {
        Timer::new(
            visual_fit_duration(rig.height_offset_disabled_seconds),
            TimerMode::Once,
        )
    });
    if tilt_off {
        delay.set_duration(visual_fit_duration(0.5));
        delay.reset();
    } else {
        delay.tick(visual_fit_duration(delta));
    }
    if delay.is_finished() {
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
    let now = actor.translation();
    let forward = (actor.rotation() * Vec3::Z).with_y(0.0).normalize_or_zero();
    let bend = &mut pose.spine;
    if bend
        .sample_timer
        .tick(visual_fit_duration(delta))
        .is_finished()
        || bend.sample.is_none()
    {
        bend.previous_sample = bend.sample.replace(now);
        bend.sample_timer.reset();
    }
    let Some(previous) = bend.previous_sample else {
        return;
    };
    let Some(base) = joint(rig.base).and_then(position) else {
        return;
    };
    let chains = [&rig.spine, &rig.tail].map(|chain| {
        chain
            .iter()
            .filter_map(|&name| {
                let joint = joint(name)?;
                Some((joint, position(joint)?))
            })
            .collect::<Vec<_>>()
    });
    let body_length = chains
        .iter()
        .flatten()
        .map(|&(_, point)| point.distance(base))
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
    for chain in chains {
        let mut previous = base;
        for (joint, point) in chain {
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

fn visual_fit_duration(seconds: f32) -> std::time::Duration {
    std::time::Duration::try_from_secs_f32(seconds).unwrap_or_default()
}
