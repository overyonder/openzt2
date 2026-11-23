use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use openzt2_game_assets::graphics_settings_types::ThreeLevelGraphicsDetail;
use openzt2_game_assets::{
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::environment_presentation_types::{AboveWaterFog, EnvironmentFog, WorldEnvironments};
use super::environment_state_types::{Daylight, ObjectAmbientLight};

pub(crate) mod current;

pub(crate) fn sample_water_environment_map(
    assets: Res<Assets<WorldDefinitionAsset>>,
    images: Res<Assets<Image>>,
    active: Res<WorldDefinitions>,
    environments: Query<(Ref<WorldEnvironment>, Ref<Daylight>)>,
    worlds: Query<&WorldEnvironments>,
    cameras: Query<(&Camera, &WorldMember), With<ZooCamera>>,
    mut sample: ResMut<
        openzt2_zoo_state::environment::environment_state_types::WaterEnvironmentMapSample,
    >,
) {
    use openzt2_zoo_state::environment::environment_state_types::WaterEnvironmentMapSample;
    let next = (|| {
        let Some(definitions) = active.get(&assets) else {
            return WaterEnvironmentMapSample::Unresolved;
        };
        let Some((environment, daylight)) = cameras
            .iter()
            .filter(|(camera, _)| camera.is_active)
            .find_map(|(_, member)| {
                let environment = worlds.get(member.root).ok()?.iter().next()?;
                environments.get(environment).ok()
            })
        else {
            return WaterEnvironmentMapSample::Unresolved;
        };
        let Some(definition) = definitions.find_environment(environment.definition) else {
            return WaterEnvironmentMapSample::Unresolved;
        };
        let time = daylight.fraction;
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

pub(super) fn fog_quality_for_view_distance(
    detail: ThreeLevelGraphicsDetail,
) -> EnvironmentFogQuality {
    match detail {
        ThreeLevelGraphicsDetail::Low => EnvironmentFogQuality::Low,
        ThreeLevelGraphicsDetail::Medium => EnvironmentFogQuality::Medium,
        ThreeLevelGraphicsDetail::High => EnvironmentFogQuality::High,
    }
}

pub(crate) fn project_resolved_camera_distance_fog(
    settings: Res<openzt2_game_assets::graphics_settings_types::GraphicsSettings>,
    terrains: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    environments: Query<(&AboveWaterFog, &Daylight)>,
    mut cameras: Query<(
        &EnvironmentFog,
        &Projection,
        Option<&TerrainWaterCameraContact>,
        &mut DistanceFog,
    )>,
) {
    let quality = fog_quality_for_view_distance(settings.view_distance);
    for (owner, projection, contact, mut fog) in &mut cameras {
        let environment = environments.get(owner.0).ok();
        let above = environment.map_or_else(AboveWaterFog::default, |(fog, _)| *fog);
        let sample = (|| {
            let source = regions.get(contact?.wet_region?.0)?;
            let terrain = terrains.get(source.terrain())?;
            let depth_ratio = source.depth_ratio(terrain)?;
            let configuration = &source
                .biome(terrain)?
                .water_presentation
                .as_ref()?
                .surface_material
                .configuration;
            Some(sample_authored_underwater_fog(
                &configuration.fog,
                quality,
                !configuration.light.is_empty() || !configuration.directional_lights.is_empty(),
                depth_ratio.clamp(0.0, 1.0),
                environment?.1.fraction,
            ))
        })();
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
use openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterCameraContact;

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
    assets: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    mut environments: Query<(
        Entity,
        &WorldEnvironment,
        Option<&ObjectAmbientLight>,
        Option<&EnvironmentVisuals>,
        Option<&mut EnvironmentWaterRayLight>,
    )>,
    camera_contacts: Query<&TerrainWaterCameraContact>,
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
        let sun_direction = camera_contacts
            .single()
            .ok()
            .and_then(|contact| contact.position)
            .and_then(|camera| {
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
    terrains: Res<Assets<TerrainAsset>>,
    environments: Query<(Ref<Daylight>, Ref<EnvironmentLightCurves>)>,
    roots: Query<&WorldEnvironments>,
    region_parents: Query<&ChildOf>,
    members: Query<&WorldMember>,
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
    let mut world_lighting = bevy::ecs::entity::EntityHashMap::default();
    for (entity, source) in regions.iter() {
        let root = members
            .get(entity)
            .ok()
            .map(|member| member.root)
            .or_else(|| {
                let parent = region_parents.get(entity).ok()?.parent();
                Some(members.get(parent).map_or(parent, |member| member.root))
            });
        let lighting = root.and_then(|root| {
            *world_lighting.entry(root).or_insert_with(|| {
                let environment = roots.get(root).ok()?.iter().next()?;
                let (daylight, curves) = environments.get(environment).ok()?;
                let fraction = daylight.fraction;
                Some((
                    fraction,
                    curves.ambient(
                        openzt2_game_data::world_definitions::environment::EnvironmentLightTarget::Water,
                        fraction,
                    ),
                    curves.water_direction(fraction),
                    daylight.is_changed() || curves.is_changed(),
                ))
            })
        });
        let daylight_changed = lighting.is_none_or(|(_, _, _, changed)| changed);
        if !daylight_changed
            && !changed.contains(entity)
            && !restored.contains(&entity)
            && !changed_terrains.contains(&source.terrain().id())
            && lights.contains(entity)
        {
            continue;
        }
        let sample = terrains.get(source.terrain()).and_then(|terrain| {
            let (fraction, water_ambient, water_direction, _) = lighting?;
            let water = source.biome(terrain)?.water_presentation.as_ref()?;
            let keys = &water.surface_material.configuration.light;
            let raw = if keys.is_empty() {
                Some(water_ambient.extend(1.0))
            } else {
                sample_water_ambient_keys(keys, fraction)
            }?;
            let lights = &water.surface_material.configuration.directional_lights;
            let direction = if let Some(first) = lights.first() {
                let sampled = super::daylight_environment_sampling::sample_directional_light_curve(
                    lights
                        .iter()
                        .filter(|sample| sample.child_index == first.child_index),
                    fraction,
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
