use bevy::{
    animation::{
        animated_field, animation_curves::AnimatableCurve, graph::AnimationNodeIndex,
        morph::WeightsCurve, AnimationTargetId,
    },
    asset::{LoadContext, VisitAssetDependencies},
    curve::{cubic_splines::CubicSegment, iterable::IterableCurve, FunctionCurve, Interval},
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
    pub has_visibility: bool,
    pub morph_target_count: usize,
    pub refresh_morph_bound: bool,
}

#[derive(Clone, Component, Reflect)]
pub struct PrefabMotionRotation {
    pub sample: Quat,
    pub relative_to: Option<Quat>,
}

#[derive(Clone, Component, Reflect)]
pub struct PrefabMotionVisibility {
    pub sample: f32,
}

#[derive(Clone, Debug, Reflect)]
#[reflect(opaque)]
struct PrefabMorphWeightsCurve {
    clock: PrefabControllerClock,
    weights: Vec<PrefabScalarCurve>,
}

impl IterableCurve<f32> for PrefabMorphWeightsCurve {
    fn domain(&self) -> Interval {
        Interval::EVERYWHERE
    }

    fn sample_iter_unchecked(&self, elapsed: f32) -> impl Iterator<Item = f32> {
        let time = controller_time(self.clock, elapsed);
        self.weights
            .iter()
            .map(move |weight| scalar_at(weight, time, 0.0))
    }
}

pub(super) fn lower_prefab_motion(
    row: usize,
    source: &mut PrefabEntity,
    context: &mut LoadContext<'_>,
) -> Option<PrefabMotion> {
    let mut animations = std::mem::take(&mut source.transform_animations);
    let spin = std::mem::take(&mut source.rotation_cycles).pop();
    let visibility = source
        .visibility_animation
        .take()
        .filter(|value| !value.keys.is_empty());
    let morph = source.morph_animation.take();
    if animations.is_empty() && spin.is_none() && visibility.is_none() && morph.is_none() {
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
    let mut transform_clocks = animations
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
        .collect::<Vec<_>>();
    let has_visibility = visibility.is_some();
    if let Some(visibility) = visibility {
        let mut clip = AnimationClip::default();
        let clock = visibility.clock;
        let keys = visibility.keys;
        clip.add_curve_to_target(
            target,
            AnimatableCurve::new(
                animated_field!(PrefabMotionVisibility::sample),
                FunctionCurve::new(Interval::EVERYWHERE, move |elapsed| {
                    let time = controller_time(clock, elapsed);
                    let next = keys.partition_point(|(key_time, _)| *key_time <= time);
                    let visible =
                        keys.len() > 1 && next == keys.len() || keys[next.saturating_sub(1)].1;
                    if visible {
                        1.0
                    } else {
                        0.0
                    }
                }),
            ),
        );
        clip.set_duration(f32::MAX);
        let clip = context.add_labeled_asset(format!("PrefabVisibility/{row}"), clip);
        transform_clocks.push((graph.add_clip(clip, 1.0, graph.root), clock.flags & 1 != 0));
    }
    let (morph_target_count, refresh_morph_bound) = if let Some(morph) = morph {
        let mut clip = AnimationClip::default();
        let clock = morph.clock;
        let count = morph.weight_curves.len();
        clip.add_curve_to_target(
            target,
            WeightsCurve(PrefabMorphWeightsCurve {
                clock,
                weights: morph.weight_curves,
            }),
        );
        clip.set_duration(f32::MAX);
        let clip = context.add_labeled_asset(format!("PrefabMorph/{row}"), clip);
        transform_clocks.push((graph.add_clip(clip, 1.0, graph.root), clock.flags & 1 != 0));
        (count, morph.refresh_bound)
    } else {
        (0, false)
    };
    let spin = spin
        .filter(|spin| !has_rotation && spin.clock.stop_time_s > spin.key_time_s)
        .map(|spin| {
            let mut clip = AnimationClip::default();
            let clock = spin.clock;
            let key_time = spin.key_time_s;
            let duration = clock.stop_time_s - key_time;
            let [x, z, y] = spin.angles_xzy;
            clip.add_curve_to_target(
                target,
                AnimatableCurve::new(
                    animated_field!(PrefabMotionRotation::sample),
                    FunctionCurve::new(Interval::EVERYWHERE, move |seconds| {
                        let progress = ((controller_time(clock, seconds) - key_time) / duration)
                            .clamp(0.0, 1.0);
                        Quat::from_rotation_x(x * progress)
                            * Quat::from_rotation_z(z * progress)
                            * Quat::from_rotation_y(y * progress)
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
        transform_clocks: transform_clocks.into(),
        spin,
        has_rotation: has_rotation || spin.is_some(),
        has_visibility,
        morph_target_count,
        refresh_morph_bound,
    })
}

pub fn controller_time(clock: PrefabControllerClock, elapsed: f32) -> f32 {
    let start = clock.start_time_s;
    let stop = clock.stop_time_s;
    let duration = stop - start;
    if duration <= 0.0 {
        return start;
    }
    let time = elapsed * clock.frequency + clock.phase;
    match (clock.flags >> 1) & 3 {
        0 => {
            let wrapped = start + time % duration;
            let wrapped = if wrapped < 0.0 {
                wrapped + duration
            } else {
                wrapped
            };
            wrapped.clamp(start, stop)
        }
        1 => {
            let position = time.rem_euclid(2.0 * duration);
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

pub fn scalar_at(curve: &PrefabScalarCurve, time: f32, default: f32) -> f32 {
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
