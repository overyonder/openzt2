//! Immutable UV curves and the animated material buffer update.

#[cfg(test)]
mod tests;

use std::{collections::HashMap, ops::RangeInclusive};

use bevy::{
    asset::AssetId, math::cubic_splines::CubicSegment, prelude::*, render::storage::ShaderBuffer,
};

use super::effect_pass_gpu_data::EffectPassMaterial;

/// One native controller lowered to per-segment polynomial coefficients.
/// Curves are shared by material instances; only their clock origins differ.
#[derive(Clone, Debug)]
pub(in crate::assets) struct TextureCoordinateAnimation {
    pub(in crate::assets) uv_set: u16,
    pub(in crate::assets) flags: u16,
    pub(in crate::assets) frequency: f32,
    pub(in crate::assets) phase: f32,
    pub(in crate::assets) interval: RangeInclusive<f32>,
    pub(in crate::assets) tracks: [Box<[(f32, CubicSegment<f32>)]>; 4],
}

impl TextureCoordinateAnimation {
    fn transform(&self, elapsed: f32, started: f32) -> Mat4 {
        if self.flags & 8 == 0 {
            return Mat4::IDENTITY;
        }
        let elapsed = if self.flags & 1 != 0 {
            elapsed - started
        } else {
            elapsed
        };
        let time = self.sample_time(elapsed);
        let [offset_u, offset_v, scale_u, scale_v] = std::array::from_fn(|index| {
            sample_track(&self.tracks[index], time, if index < 2 { 0.0 } else { 1.0 })
        });
        Mat4::from_cols(
            Vec4::new(scale_u, 0.0, 0.0, 0.0),
            Vec4::new(0.0, scale_v, 0.0, 0.0),
            Vec4::Z,
            Vec4::new(
                (1.0 - scale_u).mul_add(0.5, -offset_u),
                (1.0 - scale_v).mul_add(0.5, offset_v),
                0.0,
                1.0,
            ),
        )
    }

    fn sample_time(&self, elapsed: f32) -> f32 {
        let start = *self.interval.start();
        let stop = *self.interval.end();
        let duration = stop - start;
        if duration <= 0.0 {
            return start;
        }
        let time = elapsed.mul_add(self.frequency, self.phase);
        match (self.flags >> 1) & 3 {
            0 => {
                let wrapped = start + time % duration;
                (if wrapped < 0.0 {
                    wrapped + duration
                } else {
                    wrapped
                })
                .clamp(start, stop)
            }
            1 => {
                let phase = time.rem_euclid(2.0 * duration);
                start
                    + if phase <= duration {
                        phase
                    } else {
                        2.0_f32.mul_add(duration, -phase)
                    }
            }
            _ => time.clamp(start, stop),
        }
    }
}

fn sample_track(keys: &[(f32, CubicSegment<f32>)], time: f32, default: f32) -> f32 {
    let Some(first) = keys.first() else {
        return default;
    };
    if time <= first.0 {
        return first.1.position(0.0);
    }
    let index = keys.partition_point(|key| key.0 <= time).saturating_sub(1);
    let (start, segment) = &keys[index];
    let fraction = keys
        .get(index + 1)
        .map_or(0.0, |next| (time - start) / (next.0 - start));
    segment.position(fraction)
}

/// Animation start times keyed by material asset.
#[derive(Resource, Default)]
pub(super) struct AnimatedTextureCoordinateMaterials(HashMap<AssetId<EffectPassMaterial>, f32>);

#[allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems receive Res system parameters by value"
)]
pub(super) fn advance_texture_coordinate_animations(
    mut events: MessageReader<AssetEvent<EffectPassMaterial>>,
    mut animated: ResMut<AnimatedTextureCoordinateMaterials>,
    time: Res<Time>,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let elapsed = time.elapsed_secs();
    for event in events.read() {
        match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => {
                if materials
                    .get(*id)
                    .is_some_and(|material| !material.texture_coordinate_animations.is_empty())
                {
                    animated.0.entry(*id).or_insert(elapsed);
                } else {
                    animated.0.remove(id);
                }
            }
            AssetEvent::Removed { id } => {
                animated.0.remove(id);
            }
            _ => {}
        }
    }
    for (id, started) in &animated.0 {
        let Some(material) = materials.get_mut_untracked(*id) else {
            continue;
        };
        let mut changed = false;
        for stage in 0..8 {
            if material.fixed_function.values[stage * 5 + 4].x.to_bits() != 2.0_f32.to_bits() {
                continue;
            }
            let uv_set = material.fixed_function.values[stage * 5 + 3].y.to_bits();
            let mut next = Mat4::IDENTITY;
            let mut matched = false;
            for animation in material
                .texture_coordinate_animations
                .iter()
                .filter(|animation| f32::from(animation.uv_set).to_bits() == uv_set)
            {
                next = animation.transform(elapsed, *started) * next;
                matched = true;
            }
            if matched && material.fixed_transforms.values[stage] != next {
                material.fixed_transforms.values[stage] = next;
                changed = true;
            }
        }
        if changed {
            material.write_fixed_function_transforms_to_persistent_buffer(&mut buffers);
        }
    }
}
