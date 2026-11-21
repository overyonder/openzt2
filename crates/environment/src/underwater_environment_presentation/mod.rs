use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use openzt2_game_assets::{
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};

use super::environment_presentation_types::{AboveWaterFog, EnvironmentFog};
use super::environment_state_types::{EnvironmentDaylightFraction, ObjectAmbientLight};

pub(crate) mod current;

pub(crate) fn sample_water_environment_map(
    daylight_fraction: Res<EnvironmentDaylightFraction>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    images: Res<Assets<Image>>,
    active: Res<WorldDefinitions>,
    environments: Query<&WorldEnvironment>,
    mut sample: ResMut<
        openzt2_zoo_state::environment::environment_state_types::WaterEnvironmentMapSample,
    >,
) {
    use openzt2_zoo_state::environment::environment_state_types::WaterEnvironmentMapSample;
    let next = (|| {
        let Some(definitions) = active.get(&assets) else {
            return WaterEnvironmentMapSample::Unresolved;
        };
        let Ok(environment) = environments.single() else {
            return WaterEnvironmentMapSample::Unresolved;
        };
        let Some(definition) = definitions.find_environment(environment.definition) else {
            return WaterEnvironmentMapSample::Unresolved;
        };
        let Some(time) = daylight_fraction.0 else {
            return WaterEnvironmentMapSample::Unresolved;
        };
        let maps = definition.map_samples.as_slice();
        if maps.is_empty() {
            return WaterEnvironmentMapSample::Unauthored;
        }
        let selected = if maps.len() == 1 {
            maps.first()
        } else {
            maps.iter()
                .filter(|map| map.day_fraction <= time)
                .max_by(|left, right| left.day_fraction.total_cmp(&right.day_fraction))
        };
        let Some(selected) = selected else {
            return WaterEnvironmentMapSample::Empty;
        };
        if selected.water_texture == openzt2_game_data::AssetId::default() {
            WaterEnvironmentMapSample::Empty
        } else if let Some(texture) = definitions
            .texture_image(selected.water_texture)
            .filter(|texture| images.contains(texture.id()))
        {
            WaterEnvironmentMapSample::Texture(texture)
        } else {
            WaterEnvironmentMapSample::Unresolved
        }
    })();
    sample.set_if_neq(next);
}

pub(crate) fn project_resolved_camera_distance_fog(
    daylight_fraction: Res<EnvironmentDaylightFraction>,
    settings: Res<openzt2_game_assets::graphics_settings_types::GraphicsSettings>,
    view: Res<openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterViewState>,
    terrains: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    daylight: Query<&AboveWaterFog>,
    mut cameras: Query<(&EnvironmentFog, &Projection, &mut DistanceFog)>,
) {
    use openzt2_game_assets::graphics_settings_types::ThreeLevelGraphicsDetail;
    let sample = (|| {
        let source = regions.get(view.camera_region()?)?;
        let terrain = terrains.get(source.terrain())?;
        let depth_ratio = source.depth_ratio(terrain)?;
        let configuration = &source
            .biome(terrain)?
            .water_presentation
            .as_ref()?
            .surface_material
            .configuration;
        let time = daylight_fraction.0?;
        let quality = match settings.view_distance {
            ThreeLevelGraphicsDetail::Low => EnvironmentFogQuality::Low,
            ThreeLevelGraphicsDetail::Medium => EnvironmentFogQuality::Medium,
            ThreeLevelGraphicsDetail::High => EnvironmentFogQuality::High,
        };
        Some(sample_authored_underwater_fog(
            &configuration.fog,
            quality,
            !configuration.light.is_empty() || !configuration.directional_lights.is_empty(),
            depth_ratio.clamp(0.0, 1.0),
            time,
        ))
    })();
    for (owner, projection, mut fog) in &mut cameras {
        let above = daylight.get(owner.0).copied().unwrap_or_default();
        let selected = sample
            .filter(|_| {
                matches!(
                    projection,
                    Projection::Perspective(_) | Projection::Orthographic(_)
                )
            })
            .map_or(above, |sample| AboveWaterFog {
                color: Color::srgb(
                    sample.colour_srgb.x,
                    sample.colour_srgb.y,
                    sample.colour_srgb.z,
                ),
                start: sample.start,
                end: sample.end,
                relative_to_far_plane: true,
            });
        let far = if selected.relative_to_far_plane {
            projection.far()
        } else {
            1.0
        };
        let start = selected.start * far;
        let end = selected.end * far;
        if fog.color != selected.color
            || !matches!(fog.falloff, FogFalloff::Linear { start: current_start, end: current_end } if current_start == start && current_end == end)
        {
            fog.color = selected.color;
            fog.falloff = FogFalloff::Linear { start, end };
        }
    }
}
use super::daylight_environment_sampling::EnvironmentLightCurves;
use super::environment_presentation_types::{EnvironmentVisualSampleIndex, EnvironmentVisuals};
use bevy::ecs::relationship::RelationshipTarget;
use openzt2_game_data::{
    scene_prefab::PrefabEnvironmentKey,
    terrain::{TerrainWaterFogDetail, TerrainWaterFogDirtiness, TerrainWaterFogKeyframe},
    world_definitions::environment::EnvironmentFogQuality,
};

use openzt2_zoo_state::{
    environment::environment_state_types::{EnvironmentWaterRayLight, WorldEnvironment},
    terrain::{
        terrain_water_presentation_types::{
            TerrainWaterRegionEdit, TerrainWaterRegionQuery, TerrainWaterRegionRetired,
            TerrainWaterRegionSource,
        },
        terrain_water_underwater_particle_types::TerrainWaterRegionLight,
    },
};

pub(crate) fn update_water_ray_environment_light_samples(
    mut commands: Commands,
    daylight_fraction: Res<EnvironmentDaylightFraction>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    mut environments: Query<(
        Entity,
        &WorldEnvironment,
        Option<&ObjectAmbientLight>,
        Option<&EnvironmentVisuals>,
        Option<&mut EnvironmentWaterRayLight>,
    )>,
    views: Res<openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterViewState>,
    visuals: Query<
        (Entity, &EnvironmentVisualSampleIndex),
        Without<super::environment_presentation_types::EnvironmentSunPosition>,
    >,
    transforms: bevy::transform::helper::TransformHelper,
) {
    use openzt2_game_data::world_definitions::environment::EnvironmentVisualKind;
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    if daylight_fraction.0.is_none() {
        return;
    }
    for (entity, environment, ambient, children, current) in &mut environments {
        let Some((definition, ambient)) = definitions
            .find_environment(environment.definition)
            .zip(ambient)
        else {
            if current.is_some() {
                commands.entity(entity).remove::<EnvironmentWaterRayLight>();
            }
            continue;
        };
        let first_object_light_weighted_rgb = ambient.1;
        let sun_direction = views.camera_position().and_then(|camera| {
            visuals
                .iter_many(children.into_iter().flat_map(RelationshipTarget::iter))
                .flatten()
                .find_map(|(visual, index)| {
                    if !matches!(
                        definition.visual_samples.get(index.0 as usize)?.kind,
                        EnvironmentVisualKind::Sun
                    ) {
                        return None;
                    }
                    (transforms
                        .compute_global_transform(visual)
                        .ok()?
                        .translation()
                        - camera)
                        .try_normalize()
                })
        });
        let next = EnvironmentWaterRayLight {
            raw_object_ambient_rgba: ambient.0.extend(1.0),
            first_object_light_weighted_rgb,
            sun_direction,
        };
        if let Some(mut current) = current {
            current.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

pub(crate) fn update_region_water_light_samples(
    mut commands: Commands,
    daylight_fraction: Res<EnvironmentDaylightFraction>,
    terrains: Res<Assets<TerrainAsset>>,
    environments: Query<Ref<EnvironmentLightCurves>>,
    regions: TerrainWaterRegionQuery,
    mut lights: Query<&mut TerrainWaterRegionLight>,
    changed: Query<
        (),
        Or<(
            Changed<TerrainWaterRegionEdit>,
            Changed<TerrainWaterRegionSource>,
        )>,
    >,
    mut restored: RemovedComponents<TerrainWaterRegionRetired>,
    mut terrain_events: MessageReader<AssetEvent<TerrainAsset>>,
) {
    let changed_terrains: bevy::platform::collections::HashSet<_> = terrain_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id }
            | AssetEvent::Removed { id } => Some(id),
            _ => None,
        })
        .collect();
    let restored: bevy::ecs::entity::EntityHashSet = restored.read().collect();
    let fraction = daylight_fraction.0;
    let curves = environments.single().ok();
    let water_ambient = curves.as_ref().zip(fraction).map(|(curves, fraction)| {
        curves.ambient(
            openzt2_game_data::world_definitions::environment::EnvironmentLightTarget::Water,
            fraction,
        )
    });
    let water_direction = curves
        .as_ref()
        .zip(fraction)
        .and_then(|(curves, fraction)| curves.water_direction(fraction));
    let daylight_changed =
        daylight_fraction.is_changed() || curves.as_ref().is_none_or(|curves| curves.is_changed());
    for (entity, source) in regions.iter() {
        if !daylight_changed
            && !changed.contains(entity)
            && !restored.contains(&entity)
            && !changed_terrains.contains(&source.terrain().id())
            && lights.contains(entity)
        {
            continue;
        }
        let sample = terrains.get(source.terrain()).and_then(|terrain| {
            let water = source.biome(terrain)?.water_presentation.as_ref()?;
            let keys = &water.surface_material.configuration.light;
            let raw = if keys.is_empty() {
                water_ambient.map(|ambient| ambient.extend(1.0))
            } else {
                fraction.and_then(|fraction| sample_water_ambient_keys(keys, fraction))
            }?;
            let lights = &water.surface_material.configuration.directional_lights;
            let direction = if let Some(first) = lights.first() {
                let sampled = super::daylight_environment_sampling::sample_directional_light_curve(
                    lights
                        .iter()
                        .filter(|sample| sample.child_index == first.child_index),
                    fraction?,
                )?;
                Vec3::from_array(sampled.left_keyframe.direction)
                    .lerp(
                        Vec3::from_array(sampled.right_keyframe.direction),
                        sampled.interpolation_fraction,
                    )
                    .normalize_or_zero()
            } else if keys.is_empty() {
                water_direction?
            } else {
                Vec3::NEG_Y
            };
            Some((raw, Vec3::new(direction.x, -direction.z, direction.y)))
        });
        if let Some((raw, direction)) = sample {
            let next = TerrainWaterRegionLight::from_sample(raw, direction);
            if let Ok(mut current) = lights.get_mut(entity) {
                current.set_if_neq(next);
            } else {
                commands.entity(entity).insert(next);
            }
        } else if lights.contains(entity) {
            commands.entity(entity).remove::<TerrainWaterRegionLight>();
        }
    }
}

fn sample_water_ambient_keys(keys: &[PrefabEnvironmentKey], time: f32) -> Option<Vec4> {
    let first = keys.first()?;
    let to_rgba = |key: &PrefabEnvironmentKey| Vec3::from_array(key.ambient_srgb).extend(1.0);
    if time < first.time {
        return Some(to_rgba(first));
    }
    for pair in keys.windows(2) {
        if time < pair[1].time {
            let fraction = (time - pair[0].time) / (pair[1].time - pair[0].time);
            return Some(to_rgba(&pair[0]).lerp(to_rgba(&pair[1]), fraction));
        }
    }
    keys.last().map(to_rgba)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UnderwaterFogSample {
    pub start: f32,
    pub end: f32,
    pub colour_srgb: Vec3,
}

impl Default for UnderwaterFogSample {
    fn default() -> Self {
        Self {
            start: -0.5,
            end: 1.0,
            colour_srgb: Vec3::ZERO,
        }
    }
}

pub(crate) fn sample_authored_underwater_fog(
    details: &[TerrainWaterFogDetail],
    quality: EnvironmentFogQuality,
    lit: bool,
    depth_ratio: f32,
    daylight_fraction: f32,
) -> UnderwaterFogSample {
    let Some(detail) = details.iter().find(|detail| detail.quality == quality) else {
        return UnderwaterFogSample::default();
    };
    let levels = if lit { &detail.lit } else { &detail.unlit };
    let Some(first) = levels.first() else {
        return UnderwaterFogSample::default();
    };
    if levels.len() == 1 || depth_ratio <= first.level {
        return sample_fog_time_curve(first, daylight_fraction);
    }
    for pair in levels.windows(2) {
        if depth_ratio <= pair[1].level {
            let fraction = (depth_ratio - pair[0].level) / (pair[1].level - pair[0].level);
            return interpolate_fog_samples(
                sample_fog_time_curve(&pair[0], daylight_fraction),
                sample_fog_time_curve(&pair[1], daylight_fraction),
                fraction,
            );
        }
    }
    sample_fog_time_curve(
        levels.last().expect("nonempty fog levels"),
        daylight_fraction,
    )
}

fn sample_fog_time_curve(level: &TerrainWaterFogDirtiness, time: f32) -> UnderwaterFogSample {
    let Some(first) = level.keyframes.first() else {
        return UnderwaterFogSample::default();
    };
    let time = time.clamp(0.0, 1.0);
    if time <= first.time {
        return fog_keyframe_sample(first);
    }
    for pair in level.keyframes.windows(2) {
        if time <= pair[1].time {
            if pair[1].time <= pair[0].time {
                return fog_keyframe_sample(&pair[0]);
            }
            return interpolate_fog_samples(
                fog_keyframe_sample(&pair[0]),
                fog_keyframe_sample(&pair[1]),
                (time - pair[0].time) / (pair[1].time - pair[0].time),
            );
        }
    }
    fog_keyframe_sample(level.keyframes.last().expect("nonempty fog keyframes"))
}

fn fog_keyframe_sample(key: &TerrainWaterFogKeyframe) -> UnderwaterFogSample {
    UnderwaterFogSample {
        start: key.start,
        end: key.end,
        colour_srgb: Vec3::from_array(key.colour_rgb255.map(|value| f32::from(value) / 255.0)),
    }
}

fn interpolate_fog_samples(
    left: UnderwaterFogSample,
    right: UnderwaterFogSample,
    fraction: f32,
) -> UnderwaterFogSample {
    UnderwaterFogSample {
        start: left.start + (right.start - left.start) * fraction,
        end: left.end + (right.end - left.end) * fraction,
        colour_srgb: left.colour_srgb.lerp(right.colour_srgb, fraction),
    }
}
