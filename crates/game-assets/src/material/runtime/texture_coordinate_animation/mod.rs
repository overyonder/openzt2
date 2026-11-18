//! Immutable UV curves and the animated material buffer update.

#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, HashSet},
    ops::RangeInclusive,
};

use bevy::{
    asset::AssetId, curve::cubic_splines::CubicSegment, prelude::*, render::storage::ShaderBuffer,
};

use super::effect_pass_gpu_data::EffectPassMaterial;

/// One native controller lowered to per-segment polynomial coefficients.
/// Curves are shared by material instances; only their clock origins differ.
#[derive(Clone, Debug)]
pub struct TextureCoordinateAnimation {
    pub uv_set: u16,
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub interval: RangeInclusive<f32>,
    pub tracks: [Box<[(f32, CubicSegment<f32>)]>; 4],
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

/// Playback begins with the geometry and survives material changes.
#[derive(Component)]
pub struct GeometryTextureCoordinateAnimationClock {
    start_seconds: f32,
}

impl GeometryTextureCoordinateAnimationClock {
    pub const fn new(start_seconds: f32) -> Self {
        Self { start_seconds }
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureCoordinateAnimationSet;

#[derive(Default)]
pub(super) struct TextureCoordinateAnimationSamples {
    animated: HashSet<AssetId<EffectPassMaterial>>,
    geometry_samples: HashMap<(Option<Entity>, usize), Vec<(u16, Mat4)>>,
}

pub(super) fn advance_texture_coordinate_animations(
    mut events: MessageReader<AssetEvent<EffectPassMaterial>>,
    mut samples: Local<TextureCoordinateAnimationSamples>,
    time: Res<Time>,
    clocks: Query<&GeometryTextureCoordinateAnimationClock>,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let elapsed = time.elapsed_secs();
    let TextureCoordinateAnimationSamples {
        animated,
        geometry_samples,
    } = &mut *samples;
    for event in events.read() {
        match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => {
                if materials
                    .get(id)
                    .is_some_and(EffectPassMaterial::has_texture_coordinate_animation)
                {
                    animated.insert(id);
                } else {
                    animated.remove(&id);
                }
            }
            AssetEvent::Removed { id } => {
                animated.remove(&id);
            }
            _ => {}
        }
    }
    geometry_samples.clear();
    for &id in animated.iter() {
        let Some(material) = materials.get_mut_untracked(id) else {
            continue;
        };
        let geometry = material.texture_coordinate_animation_geometry;
        let started = match geometry {
            Some(geometry) => {
                let Ok(clock) = clocks.get(geometry) else {
                    continue;
                };
                clock.start_seconds
            }
            None => elapsed,
        };
        let curves = &material.texture_coordinate_animations;
        let matrices = geometry_samples
            .entry((geometry, curves.as_ptr() as usize))
            .or_insert_with(|| {
                let mut values = Vec::<(u16, Mat4)>::new();
                for animation in curves.iter() {
                    let transform = animation.transform(elapsed, started);
                    if let Some((_, value)) = values
                        .iter_mut()
                        .find(|(uv_set, _)| *uv_set == animation.uv_set)
                    {
                        *value = transform * *value;
                    } else {
                        values.push((animation.uv_set, transform));
                    }
                }
                values
            });
        let mut changed = false;
        for stage in 0..8 {
            if material.fixed_function.values[stage * 5 + 4].x.to_bits() != 2.0_f32.to_bits() {
                continue;
            }
            let uv_set = material.fixed_function.values[stage * 5 + 3].y.to_bits();
            if let Some((_, next)) = matrices
                .iter()
                .find(|(set, _)| f32::from(*set).to_bits() == uv_set)
            {
                if material.fixed_transforms.values[stage] != *next {
                    material.fixed_transforms.values[stage] = *next;
                    changed = true;
                }
            }
        }
        if changed {
            material.write_fixed_function_transforms_to_persistent_buffer(&mut buffers);
        }
    }
}
