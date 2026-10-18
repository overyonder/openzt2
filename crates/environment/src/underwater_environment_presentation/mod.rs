use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use openzt2_game_assets::{
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};

pub(crate) mod current;

pub(crate) fn update_water_particle_wave_fog(
    mut commands: Commands,
    cameras: Query<
        (&GlobalTransform, &Projection, &DistanceFog),
        With<openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera>,
    >,
    effects: Query<
        Entity,
        With<openzt2_game_assets::effect::particle_wave_surface::ParticleEffectWaveSurface>,
    >,
) {
    use openzt2_game_assets::effect::particle_wave_surface::ParticleEffectWaveSurfaceFog;
    let sample = cameras
        .single()
        .ok()
        .and_then(|(transform, projection, fog)| {
            ParticleEffectWaveSurfaceFog::from_camera(transform, projection, fog)
        });
    for entity in &effects {
        if let Some(sample) = sample {
            commands.entity(entity).insert((sample, bevy::camera::visibility::RenderLayers::layer(openzt2_zoo_state::terrain::terrain_water_renderer_types::TERRAIN_WATER_RENDER_LAYER)));
        } else {
            commands
                .entity(entity)
                .remove::<ParticleEffectWaveSurfaceFog>();
        }
    }
}

pub(crate) fn sample_water_environment_map(
    clock: Res<ZooClock>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    images: Res<Assets<Image>>,
    active: Res<WorldDefinitions>,
    environments: Query<&WorldEnvironment>,
    mut sample: ResMut<
        openzt2_zoo_state::environment::environment_state_types::WaterEnvironmentMapSample,
    >,
) {
    use openzt2_zoo_state::environment::environment_state_types::WaterEnvironmentMapSample;
    *sample = WaterEnvironmentMapSample::Unresolved;
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    let Ok(environment) = environments.single() else {
        return;
    };
    let Some(definition) = definitions.find_environment(environment.definition) else {
        return;
    };
    let Some(time) = super::daylight_curve_sampling::calculate_environment_month_fraction(
        &clock,
        definitions.timing(),
    ) else {
        return;
    };
    let maps = definition.map_samples.as_slice();
    if maps.is_empty() {
        *sample = WaterEnvironmentMapSample::Unauthored;
        return;
    }
    let selected = if maps.len() == 1 {
        maps.first()
    } else {
        maps.iter()
            .filter(|map| map.day_fraction <= time)
            .max_by(|left, right| left.day_fraction.total_cmp(&right.day_fraction))
    };
    let Some(selected) = selected else {
        *sample = WaterEnvironmentMapSample::Empty;
        return;
    };
    if selected.water_texture == openzt2_game_data::AssetId::default() {
        *sample = WaterEnvironmentMapSample::Empty;
    } else if let Some(texture) = definitions
        .texture_image(selected.water_texture)
        .filter(|texture| images.contains(texture.id()))
    {
        *sample = WaterEnvironmentMapSample::Texture(texture);
    }
}

#[derive(Component)]
pub(crate) struct UnderwaterFogOverride {
    daylight: DistanceFog,
    colour: Color,
    start: f32,
    end: f32,
}

pub(crate) fn apply_camera_underwater_fog(
    mut commands: Commands,
    clock: Res<ZooClock>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    settings: Res<openzt2_game_assets::graphics_settings_types::GraphicsSettings>,
    view: Res<openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterViewState>,
    terrains: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    mut cameras: Query<
        (
            Entity,
            &Projection,
            &mut DistanceFog,
            Option<&mut UnderwaterFogOverride>,
        ),
        With<openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera>,
    >,
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
        let definitions = active.get(&assets)?;
        let time = super::daylight_curve_sampling::calculate_environment_month_fraction(
            &clock,
            definitions.timing(),
        )?;
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
    for (entity, projection, mut fog, previous) in &mut cameras {
        let far = match projection {
            Projection::Perspective(projection) => projection.far,
            Projection::Orthographic(projection) => projection.far,
            _ => continue,
        };
        let mut daylight = fog.clone();
        if let Some(previous) = previous {
            let unchanged = fog.color == previous.colour
                && matches!(fog.falloff, FogFalloff::Linear { start, end } if start == previous.start && end == previous.end);
            if unchanged {
                daylight = previous.daylight.clone();
            }
        }
        if let Some(sample) = sample {
            let start = sample.start * far;
            let end = sample.end * far;
            let colour = Color::srgb(
                sample.colour_srgb.x,
                sample.colour_srgb.y,
                sample.colour_srgb.z,
            );
            fog.color = colour;
            fog.falloff = FogFalloff::Linear { start, end };
            commands.entity(entity).insert(UnderwaterFogOverride {
                daylight,
                colour,
                start,
                end,
            });
        } else {
            *fog = daylight;
            commands.entity(entity).remove::<UnderwaterFogOverride>();
        }
    }
}
use openzt2_game_data::{
    scene_prefab::PrefabEnvironmentKey,
    terrain::{TerrainWaterFogDetail, TerrainWaterFogDirtiness, TerrainWaterFogKeyframe},
    world_definitions::environment::EnvironmentFogQuality,
};
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::{
    environment::environment_state_types::{EnvironmentWaterRayLight, WorldEnvironment},
    terrain::{
        terrain_water_presentation_types::TerrainWaterRegionQuery,
        terrain_water_underwater_particle_types::TerrainWaterRegionLight,
    },
};

pub(crate) fn update_water_ray_environment_light_samples(
    mut commands: Commands,
    clock: Res<ZooClock>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    environments: Query<(Entity, &WorldEnvironment)>,
    views: Res<openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterViewState>,
    visuals: Query<
        (
            Entity,
            &super::environment_presentation_types::EnvironmentVisualOwner,
            &super::environment_presentation_types::EnvironmentVisualSampleIndex,
        ),
        Without<super::environment_presentation_types::EnvironmentSunPosition>,
    >,
    transforms: bevy::transform::helper::TransformHelper,
) {
    use openzt2_game_data::world_definitions::environment::{
        EnvironmentLightTarget, EnvironmentVisualKind,
    };
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    let Some(time) = super::daylight_curve_sampling::calculate_environment_month_fraction(
        &clock,
        definitions.timing(),
    ) else {
        return;
    };
    for (entity, environment) in &environments {
        let Some(definition) = definitions.find_environment(environment.definition) else {
            commands.entity(entity).remove::<EnvironmentWaterRayLight>();
            continue;
        };
        let ambient = super::daylight_environment_sampling::sample_environment_ambient_light_at_daylight_fraction(definitions, definition, EnvironmentLightTarget::Object, time);
        let family = (definition.family != definition.id)
            .then(|| definitions.find_environment(definition.family))
            .flatten();
        let first_kind = definition.light_samples.iter()
            .chain(family.into_iter().flat_map(|family| family.light_samples.iter()))
            .find(|sample| sample.target == EnvironmentLightTarget::Object && sample.kind != openzt2_game_data::world_definitions::environment::EnvironmentLightKind::Ambient)
            .map(|sample| (match sample.kind {
                openzt2_game_data::world_definitions::environment::EnvironmentLightKind::Sun => 1,
                openzt2_game_data::world_definitions::environment::EnvironmentLightKind::Side => 2,
                openzt2_game_data::world_definitions::environment::EnvironmentLightKind::Back => 3,
                _ => unreachable!(),
            }, sample.child_index));
        let first_object_light_weighted_rgb = first_kind.map_or(Some(Vec3::ZERO), |(kind, child_index)| {
            let sample = super::daylight_environment_sampling::sample_environment_directional_light_at_daylight_fraction(definitions, definition, EnvironmentLightTarget::Object, kind, Some(child_index), time)?;
            let diffuse = Vec3::from_array(sample.left_keyframe.diffuse).lerp(Vec3::from_array(sample.right_keyframe.diffuse), sample.interpolation_fraction);
            let intensity = sample.left_keyframe.intensity + (sample.right_keyframe.intensity - sample.left_keyframe.intensity) * sample.interpolation_fraction;
            Some(diffuse * intensity)
        });
        let sun_direction = views.camera_position().and_then(|camera| {
            visuals.iter().find_map(|(visual, owner, index)| {
                if owner.0 != entity
                    || !matches!(
                        definition.visual_samples.get(index.0 as usize)?.kind,
                        EnvironmentVisualKind::Sun
                    )
                {
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
        commands.entity(entity).insert(EnvironmentWaterRayLight {
            raw_object_ambient_rgba: ambient.extend(1.0),
            first_object_light_weighted_rgb,
            sun_direction,
        });
    }
}

pub(crate) fn update_region_water_light_samples(
    mut commands: Commands,
    clock: Res<ZooClock>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    terrains: Res<Assets<TerrainAsset>>,
    environments: Query<&WorldEnvironment>,
    regions: TerrainWaterRegionQuery,
) {
    let definitions = active.get(&assets);
    let fraction = definitions.as_ref().and_then(|definitions| {
        super::daylight_curve_sampling::calculate_environment_month_fraction(
            &clock,
            definitions.timing(),
        )
    });
    let water_ambient = definitions.as_ref().zip(fraction).and_then(|(definitions, fraction)| {
        let environment = environments.single().ok()?;
        let definition = definitions.find_environment(environment.definition)?;
        Some(super::daylight_environment_sampling::sample_environment_ambient_light_at_daylight_fraction(*definitions, definition, openzt2_game_data::world_definitions::environment::EnvironmentLightTarget::Water, fraction))
    });
    for (entity, source) in regions.iter() {
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
                    lights.iter().filter(|sample| sample.child_index == first.child_index), fraction?)?;
                Vec3::from_array(sampled.left_keyframe.direction).lerp(
                    Vec3::from_array(sampled.right_keyframe.direction), sampled.interpolation_fraction).normalize_or_zero()
            } else if keys.is_empty() {
                let definitions = definitions.as_ref()?;
                let environment = definitions.find_environment(environments.single().ok()?.definition)?;
                let first = environment.light_samples.iter().find(|sample|
                    sample.target == openzt2_game_data::world_definitions::environment::EnvironmentLightTarget::Water
                    && sample.kind != openzt2_game_data::world_definitions::environment::EnvironmentLightKind::Ambient);
                if let Some(first) = first {
                    let sampled = super::daylight_environment_sampling::sample_directional_light_curve(
                        environment.light_samples.iter().filter(|sample| sample.target == first.target
                            && sample.kind == first.kind && sample.child_index == first.child_index), fraction?)?;
                    Vec3::from_array(sampled.left_keyframe.direction).lerp(
                        Vec3::from_array(sampled.right_keyframe.direction), sampled.interpolation_fraction).normalize_or_zero()
                } else { Vec3::NEG_Y }
            } else { Vec3::NEG_Y };
            Some((raw, Vec3::new(direction.x, -direction.z, direction.y)))
        });
        if let Some((raw, direction)) = sample {
            commands
                .entity(entity)
                .insert(TerrainWaterRegionLight::from_sample(raw, direction));
        } else {
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
