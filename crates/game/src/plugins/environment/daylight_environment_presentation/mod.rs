use bevy::{
    camera::primitives::Aabb,
    pbr::{DistanceFog, FogFalloff},
    prelude::*,
};
use openzt2_game_data::world_definitions::environment::EnvironmentLightTarget;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::camera::camera_runtime_state_types::ZooCamera;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::prefab_ambient_light_contribution::PrefabAmbientLightContribution;
use crate::plugins::world_spawn::prefab_model_tint::PrefabModelTint;

use super::{
    daylight_curve_sampling::{
        calculate_daylight_fraction, interpolate_signed_normalized_direction,
        interpolate_unsigned_normalized_u16, select_cyclic_daylight_curve_sample,
    },
    daylight_environment_sampling::{
        calculate_environment_visual_sample_projection,
        calculate_weather_light_and_fog_multipliers, interpolate_f32,
        interpolate_wrapped_angle_radians, sample_environment_ambient_light_at_daylight_fraction,
        sample_environment_directional_light_at_daylight_fraction,
        sample_environment_fog_at_daylight_fraction,
    },
    environment_presentation_types::{
        EnvironmentCameraRelativeVisual, EnvironmentFog, EnvironmentLight,
        EnvironmentPresentationPending, EnvironmentSky, EnvironmentTexture, EnvironmentVisualOwner,
        EnvironmentVisualSampleIndex,
    },
    environment_state_types::{Daylight, WorldEnvironment},
    environment_visual_construction::create_environment_texture_renderer_components,
    weather_types::{Weather, WeatherTransition},
};

pub(super) fn center_camera_relative_environment_visuals_on_active_zoo_camera(
    camera: Single<(Ref<Transform>, Ref<Projection>), With<ZooCamera>>,
    children: Query<&Children>,
    primitive_bounds: Query<(&Aabb, &GlobalTransform)>,
    mut visuals: Query<
        (
            Entity,
            Ref<GlobalTransform>,
            &mut Transform,
            &mut EnvironmentCameraRelativeVisual,
        ),
        Without<ZooCamera>,
    >,
) {
    let (camera_transform, projection) = camera.into_inner();
    let camera_projection_changed = camera_transform.is_changed() || projection.is_changed();
    let camera_translation = camera_transform.translation;
    for (entity, global, mut transform, mut camera_relative) in &mut visuals {
        if !camera_projection_changed
            && !global.is_changed()
            && !camera_relative.is_changed()
            && camera_relative.source_radius_m.is_some()
        {
            continue;
        }
        if !camera_relative.scale_to_camera_far_plane {
            let projected_scale = Vec3::splat(camera_relative.projected_scale);
            if transform.translation != camera_translation || transform.scale != projected_scale {
                transform.translation = camera_translation;
                transform.scale = projected_scale;
            }
            continue;
        }
        if camera_relative.source_radius_m.is_none() {
            let root_from_world = global.affine().inverse();
            camera_relative.source_radius_m = children
                .iter_descendants_depth_first::<Children>(entity)
                .filter_map(|descendant| primitive_bounds.get(descendant).ok())
                .flat_map(|(bounds, primitive_global)| {
                    let root_from_primitive = root_from_world * primitive_global.affine();
                    let center = Vec3::from(bounds.center);
                    let half_extents = Vec3::from(bounds.half_extents);
                    [
                        Vec3::new(-1.0, -1.0, -1.0),
                        Vec3::new(-1.0, -1.0, 1.0),
                        Vec3::new(-1.0, 1.0, -1.0),
                        Vec3::new(-1.0, 1.0, 1.0),
                        Vec3::new(1.0, -1.0, -1.0),
                        Vec3::new(1.0, -1.0, 1.0),
                        Vec3::new(1.0, 1.0, -1.0),
                        Vec3::ONE,
                    ]
                    .map(move |corner| {
                        root_from_primitive
                            .transform_point3(center + half_extents * corner)
                            .length()
                    })
                })
                .reduce(f32::max)
                .filter(|radius| radius.is_finite() && *radius > f32::EPSILON);
        }
        if let Some(source_radius_m) = camera_relative.source_radius_m {
            let projected_scale = Vec3::splat(
                camera_relative.projected_scale * projection.far() * 0.95 / source_radius_m,
            );
            if transform.translation != camera_translation || transform.scale != projected_scale {
                transform.translation = camera_translation;
                transform.scale = projected_scale;
            }
        }
    }
}

// Query filters keep the mutable light and sky accesses disjoint.
#[allow(clippy::type_complexity)]
pub(super) fn project_current_daylight_weather_and_sky_into_bevy_renderer(
    mut commands: Commands,
    clock: Res<ZooClock>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut environments: Query<(
        Entity,
        &mut Daylight,
        &WorldEnvironment,
        Option<Ref<Weather>>,
        Option<&WeatherTransition>,
        Option<&EnvironmentPresentationPending>,
        &Children,
    )>,
    mut lights: Query<
        (&mut EnvironmentLight, &mut DirectionalLight, &mut Transform),
        Without<EnvironmentSky>,
    >,
    mut skies: Query<
        (
            &mut EnvironmentSky,
            &mut Transform,
            Option<&mut EnvironmentTexture>,
        ),
        Without<EnvironmentLight>,
    >,
    mut visual_samples: Query<
        (
            &EnvironmentVisualSampleIndex,
            &EnvironmentVisualOwner,
            Option<&mut PrefabModelTint>,
            Option<&mut EnvironmentCameraRelativeVisual>,
            Option<&EnvironmentTexture>,
            &mut Transform,
            &mut Visibility,
        ),
        (Without<EnvironmentSky>, Without<EnvironmentLight>),
    >,
    mut fogs: Query<(&mut DistanceFog, &Projection), (With<ZooCamera>, With<EnvironmentFog>)>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, mut daylight, environment, weather, transition, pending, children) in
        &mut environments
    {
        let Some(definition) = definitions.find_environment(environment.definition) else {
            continue;
        };
        let Some(fraction) =
            calculate_daylight_fraction(clock.tick_in_day, definitions.timing().ticks_per_day)
        else {
            continue;
        };
        if daylight.fraction == fraction
            && !weather.as_ref().is_some_and(|weather| weather.is_changed())
            && transition.is_none()
            && pending.is_none()
        {
            continue;
        }
        daylight.fraction = fraction;
        let (light_multiplier, fog_multiplier) = weather.as_deref().map_or((1.0, 1.0), |weather| {
            calculate_weather_light_and_fog_multipliers(
                definitions,
                weather,
                transition,
                clock.tick,
            )
        });

        let light_rows = definition.light_keyframes.as_slice();
        if let Some(sample) =
            select_cyclic_daylight_curve_sample(fraction, light_rows.len(), |index| {
                light_rows[index].day_fraction
            })
        {
            let left = &light_rows[sample.left_keyframe_index];
            let right = &light_rows[sample.right_keyframe_index];
            let direction = interpolate_signed_normalized_direction(
                [
                    left.direction_snorm[0],
                    left.direction_snorm[1],
                    left.direction_snorm[2],
                ],
                [
                    right.direction_snorm[0],
                    right.direction_snorm[1],
                    right.direction_snorm[2],
                ],
                sample.interpolation_fraction,
            );
            let color = Color::srgb(
                interpolate_unsigned_normalized_u16(
                    left.color_unorm[0],
                    right.color_unorm[0],
                    sample.interpolation_fraction,
                ),
                interpolate_unsigned_normalized_u16(
                    left.color_unorm[1],
                    right.color_unorm[1],
                    sample.interpolation_fraction,
                ),
                interpolate_unsigned_normalized_u16(
                    left.color_unorm[2],
                    right.color_unorm[2],
                    sample.interpolation_fraction,
                ),
            );
            let left_illuminance = left.illuminance_lux;
            let right_illuminance = right.illuminance_lux;
            let illuminance = (left_illuminance
                + (right_illuminance - left_illuminance) * sample.interpolation_fraction)
                * light_multiplier;
            for child in children.iter() {
                if let Ok((mut marker, mut light, mut transform)) = lights.get_mut(child) {
                    if marker.kind_rank != 0 {
                        continue;
                    }
                    marker.keyframe = sample.left_keyframe_index as u16;
                    light.color = color;
                    light.illuminance = illuminance;
                    if direction != Vec3::ZERO {
                        transform.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, direction);
                    }
                }
            }
        }
        if light_rows.is_empty() {
            for child in children.iter() {
                let Ok((mut marker, mut light, mut transform)) = lights.get_mut(child) else {
                    continue;
                };
                let Some(sampled_directional_light) =
                    sample_environment_directional_light_at_daylight_fraction(
                        definitions,
                        definition,
                        marker.target,
                        marker.kind_rank,
                        fraction,
                    )
                else {
                    continue;
                };
                let left_keyframe = sampled_directional_light.left_keyframe;
                let right_keyframe = sampled_directional_light.right_keyframe;
                let interpolation_fraction = sampled_directional_light.interpolation_fraction;
                let direction = Vec3::new(
                    interpolate_f32(
                        left_keyframe.direction[0],
                        right_keyframe.direction[0],
                        interpolation_fraction,
                    ),
                    interpolate_f32(
                        left_keyframe.direction[1],
                        right_keyframe.direction[1],
                        interpolation_fraction,
                    ),
                    interpolate_f32(
                        left_keyframe.direction[2],
                        right_keyframe.direction[2],
                        interpolation_fraction,
                    ),
                )
                .normalize_or_zero();
                let color = Color::linear_rgb(
                    interpolate_f32(
                        left_keyframe.diffuse[0],
                        right_keyframe.diffuse[0],
                        interpolation_fraction,
                    ),
                    interpolate_f32(
                        left_keyframe.diffuse[1],
                        right_keyframe.diffuse[1],
                        interpolation_fraction,
                    ),
                    interpolate_f32(
                        left_keyframe.diffuse[2],
                        right_keyframe.diffuse[2],
                        interpolation_fraction,
                    ),
                );
                let illuminance = interpolate_f32(
                    left_keyframe.intensity,
                    right_keyframe.intensity,
                    interpolation_fraction,
                ) * light_consts::lux::DIRECT_SUNLIGHT
                    * light_multiplier;
                marker.keyframe = sampled_directional_light.left_keyframe_index as u16;
                light.color = color;
                light.illuminance = illuminance;
                // The authored object rig has one shadow factor shared by
                // its lights. Only its sun casts the map; side and back are
                // fill lights in the source renderer.
                light.shadow_maps_enabled = marker.kind_rank == 1
                    && interpolate_f32(
                        left_keyframe.shadow,
                        right_keyframe.shadow,
                        interpolation_fraction,
                    ) > 0.0;
                if direction != Vec3::ZERO {
                    transform.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, direction);
                }
            }
        }
        let ambient = sample_environment_ambient_light_at_daylight_fraction(
            definitions,
            definition,
            EnvironmentLightTarget::Object,
            fraction,
        );
        if ambient.length_squared() > 0.0 {
            commands
                .entity(entity)
                .insert(PrefabAmbientLightContribution {
                    color_linear: ambient,
                    intensity: light_multiplier,
                });
        } else {
            commands
                .entity(entity)
                .remove::<PrefabAmbientLightContribution>();
        }

        let fog_rows = definition.fog_keyframes.as_slice();
        if let Some(sample) =
            select_cyclic_daylight_curve_sample(fraction, fog_rows.len(), |index| {
                fog_rows[index].day_fraction
            })
        {
            let left = &fog_rows[sample.left_keyframe_index];
            let right = &fog_rows[sample.right_keyframe_index];
            let color = Color::srgb(
                interpolate_unsigned_normalized_u16(
                    left.color_unorm[0],
                    right.color_unorm[0],
                    sample.interpolation_fraction,
                ),
                interpolate_unsigned_normalized_u16(
                    left.color_unorm[1],
                    right.color_unorm[1],
                    sample.interpolation_fraction,
                ),
                interpolate_unsigned_normalized_u16(
                    left.color_unorm[2],
                    right.color_unorm[2],
                    sample.interpolation_fraction,
                ),
            );
            let start = (left.start_cm as f32
                + (right.start_cm as f32 - left.start_cm as f32) * sample.interpolation_fraction)
                * 0.01
                * fog_multiplier;
            let end = (left.end_cm as f32
                + (right.end_cm as f32 - left.end_cm as f32) * sample.interpolation_fraction)
                * 0.01
                * fog_multiplier;
            for (mut fog, _) in &mut fogs {
                fog.color = color;
                fog.falloff = FogFalloff::Linear { start, end };
            }
        }
        if fog_rows.is_empty() {
            if let Some(sampled_fog) =
                sample_environment_fog_at_daylight_fraction(definitions, definition, fraction)
            {
                let left_keyframe = sampled_fog.left_keyframe;
                let right_keyframe = sampled_fog.right_keyframe;
                let interpolation_fraction = sampled_fog.interpolation_fraction;
                let color = Color::srgb(
                    interpolate_unsigned_normalized_u16(
                        left_keyframe.color_unorm[0],
                        right_keyframe.color_unorm[0],
                        interpolation_fraction,
                    ),
                    interpolate_unsigned_normalized_u16(
                        left_keyframe.color_unorm[1],
                        right_keyframe.color_unorm[1],
                        interpolation_fraction,
                    ),
                    interpolate_unsigned_normalized_u16(
                        left_keyframe.color_unorm[2],
                        right_keyframe.color_unorm[2],
                        interpolation_fraction,
                    ),
                );
                let start_factor = interpolate_f32(
                    left_keyframe.start_factor,
                    right_keyframe.start_factor,
                    interpolation_fraction,
                ) * fog_multiplier;
                let end_factor = interpolate_f32(
                    left_keyframe.end_factor,
                    right_keyframe.end_factor,
                    interpolation_fraction,
                ) * fog_multiplier;
                for (mut fog, projection) in &mut fogs {
                    // Fog distances use the active projection's far plane.
                    let far = projection.far();
                    fog.color = color;
                    fog.falloff = FogFalloff::Linear {
                        start: start_factor * far,
                        end: end_factor * far,
                    };
                }
            }
        }

        let sky_rows = definition.sky_keyframes.as_slice();
        if let Some(sample) =
            select_cyclic_daylight_curve_sample(fraction, sky_rows.len(), |index| {
                sky_rows[index].day_fraction
            })
        {
            let selected = if sample.interpolation_fraction < 0.5 {
                &sky_rows[sample.left_keyframe_index]
            } else {
                &sky_rows[sample.right_keyframe_index]
            };
            let left = &sky_rows[sample.left_keyframe_index];
            let right = &sky_rows[sample.right_keyframe_index];
            let rotation = interpolate_wrapped_angle_radians(
                f32::from(left.rotation_snorm) / f32::from(i16::MAX) * std::f32::consts::PI,
                f32::from(right.rotation_snorm) / f32::from(i16::MAX) * std::f32::consts::PI,
                sample.interpolation_fraction,
            );
            let tint = Color::srgba(
                interpolate_unsigned_normalized_u16(
                    left.tint_unorm[0],
                    right.tint_unorm[0],
                    sample.interpolation_fraction,
                ),
                interpolate_unsigned_normalized_u16(
                    left.tint_unorm[1],
                    right.tint_unorm[1],
                    sample.interpolation_fraction,
                ),
                interpolate_unsigned_normalized_u16(
                    left.tint_unorm[2],
                    right.tint_unorm[2],
                    sample.interpolation_fraction,
                ),
                interpolate_unsigned_normalized_u16(
                    left.tint_unorm[3],
                    right.tint_unorm[3],
                    sample.interpolation_fraction,
                ),
            );
            for child in children.iter() {
                if let Ok((mut sky, mut transform, texture)) = skies.get_mut(child) {
                    let definition = selected.texture;
                    sky.definition = definition;
                    if let Some(image) = definitions.texture_image(definition) {
                        if let Some(mut texture) = texture {
                            texture.image = image.clone();
                            if let Some(mut material) = texture
                                .material
                                .as_ref()
                                .and_then(|handle| materials.get_mut(handle))
                            {
                                material.base_color_texture = Some(image.clone());
                                material.emissive_texture = Some(image);
                                material.base_color = tint;
                                material.emissive = tint.to_linear();
                            }
                        } else {
                            commands.entity(child).insert(
                                create_environment_texture_renderer_components(
                                    image,
                                    tint,
                                    &mut meshes,
                                    &mut materials,
                                ),
                            );
                        }
                    }
                    transform.rotation = Quat::from_rotation_y(rotation);
                }
            }
        }
        for (sample, owner, tint, camera_relative, texture, mut transform, mut visibility) in
            &mut visual_samples
        {
            if owner.0 != entity {
                continue;
            }
            let projection = calculate_environment_visual_sample_projection(
                definition,
                sample.0 as usize,
                fraction,
            );
            *visibility = if projection.active {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            transform.scale = Vec3::splat(projection.scale);
            if let Some(mut camera_relative) = camera_relative {
                camera_relative.projected_scale = projection.scale;
            }
            if let Some(mut tint) = tint {
                tint.0 = projection.color;
            }
            if let Some(texture) = texture {
                if let Some(mut material) = texture
                    .material
                    .as_ref()
                    .and_then(|handle| materials.get_mut(handle))
                {
                    material.base_color = projection.color;
                    material.emissive = projection.color.to_linear();
                }
            }
        }
        commands
            .entity(entity)
            .remove::<EnvironmentPresentationPending>();
    }
}
