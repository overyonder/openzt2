use bevy::prelude::*;
use openzt2_game_data::world_definitions::environment::{
    EnvironmentDefinition, EnvironmentFogQuality, EnvironmentLightKind, EnvironmentLightTarget,
    EnvironmentVisualFlags, EnvironmentVisualKind,
};

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use super::{
    daylight_curve_sampling::{
        encode_daylight_fraction_as_u16, interpolate_unsigned_normalized_u16,
        select_cyclic_daylight_curve_sample,
    },
    weather_types::{Weather, WeatherTransition},
};

#[derive(Clone, Copy)]
pub(super) struct EnvironmentVisualSampleProjection {
    pub(super) active: bool,
    pub(super) color: Color,
    pub(super) scale: f32,
}

pub(super) struct SampledEnvironmentDirectionalLight<'a> {
    pub(super) left_keyframe_index: usize,
    pub(super) left_keyframe:
        &'a openzt2_game_data::world_definitions::environment::EnvironmentLightSample,
    pub(super) right_keyframe:
        &'a openzt2_game_data::world_definitions::environment::EnvironmentLightSample,
    pub(super) interpolation_fraction: f32,
}

pub(super) struct SampledEnvironmentFog<'a> {
    pub(super) left_keyframe:
        &'a openzt2_game_data::world_definitions::environment::EnvironmentFogSample,
    pub(super) right_keyframe:
        &'a openzt2_game_data::world_definitions::environment::EnvironmentFogSample,
    pub(super) interpolation_fraction: f32,
}

pub(super) fn calculate_environment_visual_sample_projection(
    definition: &EnvironmentDefinition,
    sample_index: usize,
    fraction: f32,
) -> EnvironmentVisualSampleProjection {
    let rows = definition.visual_samples.as_slice();
    let Some(current) = rows.get(sample_index) else {
        return EnvironmentVisualSampleProjection {
            active: false,
            color: Color::WHITE,
            scale: 1.0,
        };
    };
    let group = rows.iter().enumerate().filter(|(_, sample)| {
        environment_visual_kind_rank(&sample.kind) == environment_visual_kind_rank(&current.kind)
            && sample.layer == current.layer
            && sample.order == current.order
    });
    let count = group.clone().count();
    let Some(curve) = select_cyclic_daylight_curve_sample(fraction, count, |index| {
        group
            .clone()
            .nth(index)
            .map(|(_, sample)| encode_daylight_fraction_as_u16(sample.day_fraction))
            .unwrap_or(0)
    }) else {
        return EnvironmentVisualSampleProjection {
            active: false,
            color: Color::WHITE,
            scale: 1.0,
        };
    };
    let selected = if curve.interpolation_fraction < 0.5 {
        curve.left_keyframe_index
    } else {
        curve.right_keyframe_index
    };
    let active = group
        .clone()
        .nth(selected)
        .is_some_and(|(index, _)| index == sample_index);
    let endpoints = group
        .clone()
        .nth(curve.left_keyframe_index)
        .zip(group.clone().nth(curve.right_keyframe_index));
    let (color, scale) = endpoints.map_or((Color::WHITE, 1.0), |(left, right)| {
        let left = left.1;
        let right = right.1;
        let t = if current
            .flags
            .contains_all(EnvironmentVisualFlags::SMOOTH_UPDATE)
        {
            curve.interpolation_fraction
        } else if curve.interpolation_fraction < 0.5 {
            0.0
        } else {
            1.0
        };
        (
            Color::srgb(
                interpolate_unsigned_normalized_u16(left.color_unorm[0], right.color_unorm[0], t),
                interpolate_unsigned_normalized_u16(left.color_unorm[1], right.color_unorm[1], t),
                interpolate_unsigned_normalized_u16(left.color_unorm[2], right.color_unorm[2], t),
            ),
            interpolate_f32(left.scale, right.scale, t),
        )
    });
    EnvironmentVisualSampleProjection {
        active,
        color,
        scale,
    }
}

pub(super) fn environment_visual_kind_rank(kind: &EnvironmentVisualKind) -> u8 {
    match kind {
        EnvironmentVisualKind::Sky => 0,
        EnvironmentVisualKind::Sun => 1,
        EnvironmentVisualKind::Skirt => 2,
    }
}

#[inline]
pub(super) fn interpolate_f32(left: f32, right: f32, t: f32) -> f32 {
    left + (right - left) * t
}

#[inline]
pub(super) fn interpolate_wrapped_angle_radians(left: f32, right: f32, t: f32) -> f32 {
    let delta = (right - left + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    left + delta * t
}

pub(super) fn sample_environment_directional_light_at_daylight_fraction<'a>(
    catalogue: WorldDefinitionsView<'a>,
    definition: &'a EnvironmentDefinition,
    target: EnvironmentLightTarget,
    kind_rank: u8,
    fraction: f32,
) -> Option<SampledEnvironmentDirectionalLight<'a>> {
    let rows = iterate_environment_and_inherited_family_light_samples(catalogue, definition)
        .filter(|row| row.target == target && environment_light_kind_rank(&row.kind) == kind_rank);
    let count = rows.clone().count();
    let sample = select_cyclic_daylight_curve_sample(fraction, count, |index| {
        rows.clone()
            .nth(index)
            .map(|row| encode_daylight_fraction_as_u16(row.day_fraction))
            .unwrap_or(0)
    })?;
    Some(SampledEnvironmentDirectionalLight {
        left_keyframe_index: sample.left_keyframe_index,
        left_keyframe: rows.clone().nth(sample.left_keyframe_index)?,
        right_keyframe: rows.clone().nth(sample.right_keyframe_index)?,
        interpolation_fraction: sample.interpolation_fraction,
    })
}

pub(super) fn collect_authored_environment_light_kind_ranks(
    catalogue: WorldDefinitionsView<'_>,
    definition: &EnvironmentDefinition,
    target: EnvironmentLightTarget,
) -> ([u8; 3], usize) {
    let mut ranks = [0; 3];
    let mut count = 0;
    for rank in 1..=3 {
        let present = iterate_environment_and_inherited_family_light_samples(catalogue, definition)
            .any(|row| row.target == target && environment_light_kind_rank(&row.kind) == rank);
        if present {
            ranks[count] = rank;
            count += 1;
        }
    }
    (ranks, count)
}

pub(super) fn sample_environment_ambient_light_at_daylight_fraction(
    catalogue: WorldDefinitionsView<'_>,
    definition: &EnvironmentDefinition,
    target: EnvironmentLightTarget,
    fraction: f32,
) -> Vec3 {
    (0..4).fold(Vec3::ZERO, |ambient, kind| {
        let rows = iterate_environment_and_inherited_family_light_samples(catalogue, definition)
            .filter(|row| row.target == target && environment_light_kind_rank(&row.kind) == kind);
        let count = rows.clone().count();
        let Some(sample) = select_cyclic_daylight_curve_sample(fraction, count, |index| {
            rows.clone()
                .nth(index)
                .map(|row| encode_daylight_fraction_as_u16(row.day_fraction))
                .unwrap_or(0)
        }) else {
            return ambient;
        };
        let Some((left, right)) = rows
            .clone()
            .nth(sample.left_keyframe_index)
            .zip(rows.clone().nth(sample.right_keyframe_index))
        else {
            return ambient;
        };
        ambient
            + Vec3::new(
                interpolate_f32(
                    left.ambient[0],
                    right.ambient[0],
                    sample.interpolation_fraction,
                ),
                interpolate_f32(
                    left.ambient[1],
                    right.ambient[1],
                    sample.interpolation_fraction,
                ),
                interpolate_f32(
                    left.ambient[2],
                    right.ambient[2],
                    sample.interpolation_fraction,
                ),
            )
    })
}

#[inline]
fn environment_light_kind_rank(kind: &EnvironmentLightKind) -> u8 {
    match kind {
        EnvironmentLightKind::Ambient => 0,
        EnvironmentLightKind::Sun => 1,
        EnvironmentLightKind::Side => 2,
        EnvironmentLightKind::Back => 3,
    }
}

pub(super) fn sample_environment_fog_at_daylight_fraction<'a>(
    catalogue: WorldDefinitionsView<'a>,
    definition: &'a EnvironmentDefinition,
    fraction: f32,
) -> Option<SampledEnvironmentFog<'a>> {
    fn sample_environment_fog_for_authored_quality<'a>(
        catalogue: WorldDefinitionsView<'a>,
        definition: &'a EnvironmentDefinition,
        fraction: f32,
        quality: u8,
    ) -> Option<SampledEnvironmentFog<'a>> {
        let rows = iterate_environment_and_inherited_family_fog_samples(catalogue, definition)
            .filter(|row| environment_fog_quality_rank(&row.quality) == quality);
        let count = rows.clone().count();
        let sample = select_cyclic_daylight_curve_sample(fraction, count, |index| {
            rows.clone()
                .nth(index)
                .map(|row| encode_daylight_fraction_as_u16(row.day_fraction))
                .unwrap_or(0)
        })?;
        Some(SampledEnvironmentFog {
            left_keyframe: rows.clone().nth(sample.left_keyframe_index)?,
            right_keyframe: rows.clone().nth(sample.right_keyframe_index)?,
            interpolation_fraction: sample.interpolation_fraction,
        })
    }
    sample_environment_fog_for_authored_quality(
        catalogue,
        definition,
        fraction,
        environment_fog_quality_rank(&EnvironmentFogQuality::High),
    )
    .or_else(|| {
        sample_environment_fog_for_authored_quality(
            catalogue,
            definition,
            fraction,
            environment_fog_quality_rank(&EnvironmentFogQuality::Medium),
        )
    })
    .or_else(|| {
        sample_environment_fog_for_authored_quality(
            catalogue,
            definition,
            fraction,
            environment_fog_quality_rank(&EnvironmentFogQuality::Low),
        )
    })
}

fn iterate_environment_and_inherited_family_light_samples<'a>(
    catalogue: WorldDefinitionsView<'a>,
    definition: &'a EnvironmentDefinition,
) -> impl Clone
       + Iterator<
    Item = &'a openzt2_game_data::world_definitions::environment::EnvironmentLightSample,
> {
    let family = (definition.family != definition.id)
        .then(|| catalogue.find_environment(definition.family))
        .flatten()
        .map_or(&[][..], |family| family.light_samples.as_slice());
    definition.light_samples.iter().chain(family)
}

fn iterate_environment_and_inherited_family_fog_samples<'a>(
    catalogue: WorldDefinitionsView<'a>,
    definition: &'a EnvironmentDefinition,
) -> impl Clone
       + Iterator<Item = &'a openzt2_game_data::world_definitions::environment::EnvironmentFogSample>
{
    let family = (definition.family != definition.id)
        .then(|| catalogue.find_environment(definition.family))
        .flatten()
        .map_or(&[][..], |family| family.fog_samples.as_slice());
    definition.fog_samples.iter().chain(family)
}

#[inline]
fn environment_fog_quality_rank(quality: &EnvironmentFogQuality) -> u8 {
    match quality {
        EnvironmentFogQuality::Low => 0,
        EnvironmentFogQuality::Medium => 1,
        EnvironmentFogQuality::High => 2,
    }
}

pub(super) fn calculate_weather_light_and_fog_multipliers(
    catalogue: WorldDefinitionsView<'_>,
    weather: &Weather,
    transition: Option<&WeatherTransition>,
    tick: u64,
) -> (f32, f32) {
    let authored_weather_multipliers = |weather_definition_identifier| {
        catalogue
            .find_weather(weather_definition_identifier)
            .map(|definition| {
                (
                    f32::from(definition.light_multiplier) / f32::from(u16::MAX),
                    f32::from(definition.fog_multiplier) / f32::from(u16::MAX),
                )
            })
    };
    let current = authored_weather_multipliers(weather.definition).unwrap_or((1.0, 1.0));
    let Some(transition) = transition else {
        return current;
    };
    let Some(target) = authored_weather_multipliers(transition.to) else {
        return current;
    };
    let elapsed = tick.saturating_sub(transition.start_tick) as f32;
    let duration = transition.duration_ticks.max(1) as f32;
    let t = (elapsed / duration).clamp(0.0, 1.0);
    (
        current.0 + (target.0 - current.0) * t,
        current.1 + (target.1 - current.1) * t,
    )
}
