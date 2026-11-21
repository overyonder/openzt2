use bevy::{
    animation::{
        animated_field, animation_curves::AnimatableCurve, graph::AnimationNodeIndex,
        AnimationTargetId,
    },
    asset::{LoadContext, VisitAssetDependencies},
    curve::{cubic_splines::CubicSegment, FunctionCurve, Interval},
    prelude::*,
};
use openzt2_game_data::scene_prefab::{
    PrefabControllerClock, PrefabEntity, PrefabRotationCurve, PrefabScalarCurve,
};
use std::sync::Arc;

/// The graph nodes whose clocks use elapsed game time.
#[derive(Component, Clone)]
pub struct PrefabTransformClocks(pub Arc<[(AnimationNodeIndex, bool)]>);

#[derive(Debug, VisitAssetDependencies)]
pub struct PrefabMotion {
    pub(super) row: usize,
    #[dependency]
    pub graph: Handle<AnimationGraph>,
    pub target: AnimationTargetId,
    pub transform_clocks: Arc<[(AnimationNodeIndex, bool)]>,
    pub spin: Option<AnimationNodeIndex>,
    pub has_rotation: bool,
}

#[derive(Clone, Component, Reflect)]
pub struct PrefabMotionRotation {
    pub sample: Quat,
    pub relative_to: Option<Quat>,
}

pub(super) fn lower_prefab_motion(
    row: usize,
    source: &mut PrefabEntity,
    context: &mut LoadContext<'_>,
) -> Option<PrefabMotion> {
    let mut animations = std::mem::take(&mut source.transform_animations);
    let spin = std::mem::take(&mut source.rotation_cycles).pop();
    if animations.is_empty() && spin.is_none() {
        return None;
    }
    let target = AnimationTargetId::from_iter(["prefab_motion"]);
    let mut graph = AnimationGraph::new();
    let mut has_rotation = false;
    // Later writes in the controller chain win independently per channel.
    let mut occupied = [false; 3];
    for animation in animations.iter_mut().rev() {
        if animation.translation_m.is_some() && std::mem::replace(&mut occupied[0], true) {
            animation.translation_m = None;
        }
        if animation.rotation.is_some() && std::mem::replace(&mut occupied[1], true) {
            animation.rotation = None;
        }
        if animation.uniform_scale.is_some() && std::mem::replace(&mut occupied[2], true) {
            animation.uniform_scale = None;
        }
    }
    let mut lower_transform =
        |animation: openzt2_game_data::scene_prefab::PrefabTransformAnimation| {
            let clock = animation.clock;
            let mut clip = AnimationClip::default();
            if let Some([x, y, z]) = animation.translation_m {
                clip.add_curve_to_target(
                    target,
                    AnimatableCurve::new(
                        animated_field!(Transform::translation),
                        FunctionCurve::new(Interval::EVERYWHERE, move |elapsed| {
                            let time = controller_time(clock, elapsed);
                            Vec3::new(
                                scalar_at(&x, time, 0.0),
                                scalar_at(&y, time, 0.0),
                                scalar_at(&z, time, 0.0),
                            )
                        }),
                    ),
                );
            }
            if let Some(scale) = animation.uniform_scale {
                clip.add_curve_to_target(
                    target,
                    AnimatableCurve::new(
                        animated_field!(Transform::scale),
                        FunctionCurve::new(Interval::EVERYWHERE, move |elapsed| {
                            Vec3::splat(scalar_at(&scale, controller_time(clock, elapsed), 1.0))
                        }),
                    ),
                );
            }
            if let Some(rotation) = animation.rotation {
                has_rotation = true;
                clip.add_curve_to_target(
                    target,
                    AnimatableCurve::new(
                        animated_field!(PrefabMotionRotation::sample),
                        FunctionCurve::new(Interval::EVERYWHERE, move |elapsed| {
                            rotation_at(&rotation, controller_time(clock, elapsed))
                        }),
                    ),
                );
            }
            clip.set_duration(f32::MAX);
            (clip, clock.flags & 1 != 0)
        };
    let transform_clocks = animations
        .into_iter()
        .enumerate()
        .filter_map(|(index, animation)| {
            if animation.translation_m.is_none()
                && animation.rotation.is_none()
                && animation.uniform_scale.is_none()
            {
                return None;
            }
            let (clip, instance_origin) = lower_transform(animation);
            let clip = context.add_labeled_asset(format!("PrefabTransform/{row}/{index}"), clip);
            Some((graph.add_clip(clip, 1.0, graph.root), instance_origin))
        })
        .collect::<Arc<[_]>>();
    let spin = spin.filter(|_| !has_rotation).map(|spin| {
        let mut clip = AnimationClip::default();
        let axis_rate = Vec3::from_array(spin.radians_per_second);
        clip.add_curve_to_target(
            target,
            AnimatableCurve::new(
                animated_field!(PrefabMotionRotation::sample),
                FunctionCurve::new(Interval::EVERYWHERE, move |seconds| {
                    Quat::from_scaled_axis(axis_rate * seconds)
                }),
            ),
        );
        clip.set_duration(f32::MAX);
        let clip = context.add_labeled_asset(format!("PrefabSpin/{row}"), clip);
        graph.add_clip(clip, 1.0, graph.root)
    });
    Some(PrefabMotion {
        row,
        graph: context.add_labeled_asset(format!("PrefabMotion/{row}"), graph),
        target,
        transform_clocks,
        spin,
        has_rotation: has_rotation || spin.is_some(),
    })
}

fn controller_time(clock: PrefabControllerClock, elapsed: f32) -> f32 {
    let start = clock.start_time_s;
    let stop = clock.stop_time_s;
    let duration = stop - start;
    if duration <= 0.0 {
        return start;
    }
    let time = elapsed.mul_add(clock.frequency, clock.phase);
    match (clock.flags >> 1) & 3 {
        0 => (start + (time - start).rem_euclid(duration)).clamp(start, stop),
        1 => {
            let position = (time - start).rem_euclid(2.0 * duration);
            start
                + if position <= duration {
                    position
                } else {
                    2.0_f32.mul_add(duration, -position)
                }
        }
        _ => time.clamp(start, stop),
    }
}

fn scalar_at(curve: &PrefabScalarCurve, time: f32, default: f32) -> f32 {
    let segments = &curve.segments;
    let Some(first) = segments.first() else {
        return default;
    };
    if time <= first.start_time_s {
        return first.coefficients[0];
    }
    let index = segments
        .partition_point(|segment| segment.start_time_s <= time)
        .saturating_sub(1);
    let segment = &segments[index];
    let fraction = segments.get(index + 1).map_or(0.0, |next| {
        (time - segment.start_time_s) / (next.start_time_s - segment.start_time_s)
    });
    CubicSegment {
        coeff: segment.coefficients,
    }
    .position(fraction)
}

fn rotation_at(curve: &PrefabRotationCurve, time: f32) -> Quat {
    match curve {
        PrefabRotationCurve::EulerAnglesXzy([x, z, y]) => {
            Quat::from_rotation_x(scalar_at(x, time, 0.0))
                * Quat::from_rotation_z(scalar_at(z, time, 0.0))
                * Quat::from_rotation_y(scalar_at(y, time, 0.0))
        }
        PrefabRotationCurve::Quaternions { keys, stepped } => {
            let Some(first) = keys.first() else {
                return Quat::IDENTITY;
            };
            if time <= first.time_s {
                return Quat::from_array(first.rotation_xyzw);
            }
            let index = keys
                .partition_point(|key| key.time_s <= time)
                .saturating_sub(1);
            let key = keys[index];
            let rotation = Quat::from_array(key.rotation_xyzw);
            let Some(next) = keys.get(index + 1).filter(|_| !stepped) else {
                return rotation;
            };
            let span = next.time_s - key.time_s;
            if span <= 0.0 {
                return Quat::from_array(next.rotation_xyzw);
            }
            rotation.slerp(
                Quat::from_array(next.rotation_xyzw),
                (time - key.time_s) / span,
            )
        }
    }
}
