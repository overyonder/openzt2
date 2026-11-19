use bevy::prelude::*;
use openzt2_game_data::world_definitions::environment::{
    EnvironmentDefinition, EnvironmentFogQuality, EnvironmentFogSample, EnvironmentLightKind,
    EnvironmentLightSample, EnvironmentLightTarget, EnvironmentVisualFlags, EnvironmentVisualKind,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use super::{
    daylight_curve_sampling::{
        encode_daylight_fraction_as_u16, interpolate_unsigned_normalized_u16,
        select_cyclic_daylight_curve_sample,
    },
    weather_types::{Weather, WeatherTransition},
};

#[derive(Clone, Copy, PartialEq)]
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

#[derive(Component)]
pub(super) struct EnvironmentVisualCurves {
    groups: Vec<Vec<(usize, u16)>>,
}

#[derive(Component, PartialEq)]
pub(super) struct EnvironmentVisualProjections(pub(super) Box<[EnvironmentVisualSampleProjection]>);

impl EnvironmentVisualCurves {
    pub(super) fn from_definition(definition: &EnvironmentDefinition) -> Self {
        let mut groups = std::collections::BTreeMap::new();
        for (index, row) in definition.visual_samples.iter().enumerate() {
            groups
                .entry((
                    environment_visual_kind_rank(&row.kind),
                    row.layer,
                    row.order,
                ))
                .or_insert_with(Vec::new)
                .push((index, encode_daylight_fraction_as_u16(row.day_fraction)));
        }
        Self {
            groups: groups.into_values().collect(),
        }
    }

    pub(super) fn sample(
        &self,
        definition: &EnvironmentDefinition,
        fraction: f32,
    ) -> EnvironmentVisualProjections {
        let rows = &definition.visual_samples;
        let mut projections = vec![
            EnvironmentVisualSampleProjection {
                active: false,
                color: Color::WHITE,
                scale: 1.0
            };
            rows.len()
        ];
        for group in &self.groups {
            let Some(curve) =
                select_cyclic_daylight_curve_sample(fraction, group.len(), |index| group[index].1)
            else {
                continue;
            };
            let stepped = if curve.interpolation_fraction < 0.5 {
                0.0
            } else {
                1.0
            };
            let selected = if stepped == 0.0 {
                curve.left_keyframe_index
            } else {
                curve.right_keyframe_index
            };
            let left = &rows[group[curve.left_keyframe_index].0];
            let right = &rows[group[curve.right_keyframe_index].0];
            let sample = |t| {
                (
                    Color::srgb(
                        interpolate_unsigned_normalized_u16(
                            left.color_unorm[0],
                            right.color_unorm[0],
                            t,
                        ),
                        interpolate_unsigned_normalized_u16(
                            left.color_unorm[1],
                            right.color_unorm[1],
                            t,
                        ),
                        interpolate_unsigned_normalized_u16(
                            left.color_unorm[2],
                            right.color_unorm[2],
                            t,
                        ),
                    ),
                    interpolate_f32(left.scale, right.scale, t),
                )
            };
            let smooth = sample(curve.interpolation_fraction);
            let stepped = sample(stepped);
            for (keyframe, &(index, _)) in group.iter().enumerate() {
                let flags = rows[index].flags;
                let selected =
                    if flags.contains_all(EnvironmentVisualFlags::INTERPOLATE_VERTEX_COLORS) {
                        0
                    } else {
                        selected
                    };
                let (color, scale) = if flags.contains_all(EnvironmentVisualFlags::SMOOTH_UPDATE) {
                    smooth
                } else {
                    stepped
                };
                projections[index] = EnvironmentVisualSampleProjection {
                    active: keyframe == selected,
                    color,
                    scale,
                };
            }
        }
        EnvironmentVisualProjections(projections.into_boxed_slice())
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

/// Environment-local light and fog programs with inherited rows expanded
/// in authored order. Lighting and fog sample these programs independently.
#[derive(Component)]
pub(super) struct EnvironmentLightCurves {
    lights: [[Vec<EnvironmentLightSample>; 4]; 4],
    first_object: Option<(u8, u16)>,
    own_water_direction: Vec<EnvironmentLightSample>,
    fog: [Vec<EnvironmentFogSample>; 3],
}

impl EnvironmentLightCurves {
    pub(super) fn new(
        catalogue: WorldDefinitionsView<'_>,
        definition: &EnvironmentDefinition,
    ) -> Self {
        let mut curves = Self {
            lights: std::array::from_fn(|_| std::array::from_fn(|_| Vec::new())),
            first_object: None,
            own_water_direction: Vec::new(),
            fog: std::array::from_fn(|_| Vec::new()),
        };
        for row in iterate_environment_and_referenced_light_samples(catalogue, definition) {
            let kind = environment_light_kind_rank(&row.kind);
            if row.target == EnvironmentLightTarget::Object
                && kind != 0
                && curves.first_object.is_none()
            {
                curves.first_object = Some((kind, row.child_index));
            }
            curves.lights[light_target_index(row.target)][kind as usize].push(*row);
        }
        let own_water_direction = definition.light_samples.iter().find(|row| {
            row.target == EnvironmentLightTarget::Water && row.kind != EnvironmentLightKind::Ambient
        });
        if let Some(first) = own_water_direction {
            curves
                .own_water_direction
                .extend(definition.light_samples.iter().copied().filter(|row| {
                    row.target == first.target
                        && row.kind == first.kind
                        && row.child_index == first.child_index
                }));
        }
        for row in iterate_environment_and_inherited_family_fog_samples(catalogue, definition) {
            curves.fog[environment_fog_quality_rank(&row.quality) as usize].push(*row);
        }
        curves
    }

    pub(super) fn directional(
        &self,
        target: EnvironmentLightTarget,
        kind: u8,
        child: Option<u16>,
        fraction: f32,
    ) -> Option<SampledEnvironmentDirectionalLight<'_>> {
        let rows = self.lights[light_target_index(target)].get(kind as usize)?;
        sample_directional_light_curve(
            rows.iter()
                .filter(|row| child.is_none_or(|child| row.child_index == child)),
            fraction,
        )
    }

    pub(super) fn first_object_light_weighted_rgb(&self, fraction: f32) -> Option<Vec3> {
        let Some((kind, child)) = self.first_object else {
            return Some(Vec3::ZERO);
        };
        let sample =
            self.directional(EnvironmentLightTarget::Object, kind, Some(child), fraction)?;
        let diffuse = Vec3::from_array(sample.left_keyframe.diffuse).lerp(
            Vec3::from_array(sample.right_keyframe.diffuse),
            sample.interpolation_fraction,
        );
        let intensity = interpolate_f32(
            sample.left_keyframe.intensity,
            sample.right_keyframe.intensity,
            sample.interpolation_fraction,
        );
        Some(diffuse * intensity)
    }

    pub(super) fn water_direction(&self, fraction: f32) -> Option<Vec3> {
        if self.own_water_direction.is_empty() {
            return Some(Vec3::NEG_Y);
        }
        let sample = sample_directional_light_curve(self.own_water_direction.iter(), fraction)?;
        Some(
            Vec3::from_array(sample.left_keyframe.direction)
                .lerp(
                    Vec3::from_array(sample.right_keyframe.direction),
                    sample.interpolation_fraction,
                )
                .normalize_or_zero(),
        )
    }

    pub(super) fn ambient(&self, target: EnvironmentLightTarget, fraction: f32) -> Vec3 {
        self.lights[light_target_index(target)]
            .iter()
            .fold(Vec3::ZERO, |ambient, rows| {
                let Some(sample) =
                    select_cyclic_daylight_curve_sample(fraction, rows.len(), |index| {
                        encode_daylight_fraction_as_u16(rows[index].day_fraction)
                    })
                else {
                    return ambient;
                };
                let left = rows[sample.left_keyframe_index];
                let right = rows[sample.right_keyframe_index];
                ambient
                    + Vec3::from_array(std::array::from_fn(|channel| {
                        interpolate_f32(
                            left.ambient[channel],
                            right.ambient[channel],
                            sample.interpolation_fraction,
                        )
                    }))
            })
    }

    pub(super) fn fog(&self, fraction: f32) -> Option<SampledEnvironmentFog<'_>> {
        self.fog.iter().rev().find_map(|rows| {
            let sample = select_cyclic_daylight_curve_sample(fraction, rows.len(), |index| {
                encode_daylight_fraction_as_u16(rows[index].day_fraction)
            })?;
            Some(SampledEnvironmentFog {
                left_keyframe: &rows[sample.left_keyframe_index],
                right_keyframe: &rows[sample.right_keyframe_index],
                interpolation_fraction: sample.interpolation_fraction,
            })
        })
    }
}

fn light_target_index(target: EnvironmentLightTarget) -> usize {
    match target {
        EnvironmentLightTarget::Terrain => 0,
        EnvironmentLightTarget::Object => 1,
        EnvironmentLightTarget::Water => 2,
        EnvironmentLightTarget::Cloud => 3,
    }
}

pub(super) fn sample_directional_light_curve<'a>(
    rows: impl Iterator<Item = &'a EnvironmentLightSample>,
    fraction: f32,
) -> Option<SampledEnvironmentDirectionalLight<'a>> {
    if !fraction.is_finite() {
        return None;
    }
    let mut before: Option<(usize, &EnvironmentLightSample)> = None;
    let mut after: Option<(usize, &EnvironmentLightSample)> = None;
    let mut first: Option<(usize, &EnvironmentLightSample)> = None;
    let mut last: Option<(usize, &EnvironmentLightSample)> = None;
    for (index, row) in rows.enumerate() {
        if first.is_none_or(|(_, old)| row.day_fraction < old.day_fraction) {
            first = Some((index, row));
        }
        if last.is_none_or(|(_, old)| row.day_fraction > old.day_fraction) {
            last = Some((index, row));
        }
        if row.day_fraction <= fraction
            && before.is_none_or(|(_, old)| row.day_fraction > old.day_fraction)
        {
            before = Some((index, row));
        }
        if row.day_fraction > fraction
            && after.is_none_or(|(_, old)| row.day_fraction < old.day_fraction)
        {
            after = Some((index, row));
        }
    }
    let (left_keyframe_index, left_keyframe) = before.or(first)?;
    let (_, right_keyframe) = after.or(last)?;
    let duration = right_keyframe.day_fraction - left_keyframe.day_fraction;
    Some(SampledEnvironmentDirectionalLight {
        left_keyframe_index,
        left_keyframe,
        right_keyframe,
        interpolation_fraction: if duration > 0.0 {
            ((fraction - left_keyframe.day_fraction) / duration).clamp(0.0, 1.0)
        } else {
            0.0
        },
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
        let present = iterate_environment_and_referenced_light_samples(catalogue, definition)
            .any(|row| row.target == target && environment_light_kind_rank(&row.kind) == rank);
        if present {
            ranks[count] = rank;
            count += 1;
        }
    }
    (ranks, count)
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

pub(super) fn iterate_environment_and_referenced_light_samples<'a>(
    catalogue: WorldDefinitionsView<'a>,
    definition: &'a EnvironmentDefinition,
) -> impl Clone
       + Iterator<
    Item = &'a openzt2_game_data::world_definitions::environment::EnvironmentLightSample,
> {
    let light_program = definition.light_program.unwrap_or(definition.family);
    let family = (light_program != definition.id)
        .then(|| catalogue.find_environment(light_program))
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
