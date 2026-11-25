use bevy::{camera::primitives::Aabb, ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_data::world_definitions::environment::EnvironmentLightTarget;
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentLight;
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentSpecularLightColor;
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentTerrainAmbientLight;
use openzt2_zoo_state::environment::environment_state_types::WorldEnvironment;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::prefab_ambient_light_contribution::PrefabAmbientLightContribution;
use openzt2_world_state::world_spawn::prefab_model_tint::PrefabModelTint;

use super::{
    daylight_curve_sampling::{
        calculate_environment_month_fraction, interpolate_signed_normalized_direction,
        interpolate_unsigned_normalized_u16, select_cyclic_daylight_curve_sample,
    },
    daylight_environment_sampling::{
        calculate_weather_light_and_fog_multipliers, interpolate_f32,
        interpolate_wrapped_angle_radians, EnvironmentLightCurves, EnvironmentVisualCurves,
        EnvironmentVisualProjections,
    },
    environment_presentation_types::{
        AboveWaterFog, EnvironmentCameraRelativeVisual, EnvironmentOfWorld,
        EnvironmentPresentationPending, EnvironmentSky, EnvironmentTexture,
        EnvironmentVisualSampleIndex, EnvironmentVisuals,
    },
    environment_state_types::{Daylight, ObjectAmbientLight},
    environment_visual_construction::create_environment_texture_renderer_components,
    weather_types::{Weather, WeatherTransition},
};

pub(super) fn publish_world_daylight_from_zoo_clock(
    clocks: Query<Ref<ZooClock>>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut environments: Query<(&EnvironmentOfWorld, &mut Daylight)>,
) {
    let definitions_changed = assets.is_changed() || definitions.is_changed();
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    for (owner, mut daylight) in &mut environments {
        let Ok(clock) = clocks.get(owner.0) else {
            continue;
        };
        if !clock.is_changed() && !definitions_changed {
            continue;
        }
        if let Some(fraction) = calculate_environment_month_fraction(&clock, definitions.timing()) {
            daylight.set_if_neq(Daylight { fraction });
        }
    }
}

pub(super) fn sample_authored_object_ambient_light(
    mut commands: Commands,
    mut environments: Query<(
        Entity,
        Ref<Daylight>,
        Ref<EnvironmentLightCurves>,
        Option<&mut ObjectAmbientLight>,
    )>,
) {
    for (entity, daylight, curves, existing) in &mut environments {
        let fraction = daylight.fraction;
        if existing.is_some() && !curves.is_changed() && !daylight.is_changed() {
            continue;
        }
        let next = ObjectAmbientLight(
            curves.ambient(EnvironmentLightTarget::Object, fraction),
            curves.first_object_light_weighted_rgb(fraction),
        );
        if let Some(mut existing) = existing {
            existing.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

pub(super) fn sample_authored_environment_visual_curves(
    mut commands: Commands,
    assets: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    mut source_events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut environments: Query<(
        Entity,
        Ref<Daylight>,
        Ref<WorldEnvironment>,
        Option<&mut EnvironmentLightCurves>,
        Option<&mut EnvironmentVisualCurves>,
        Option<&mut EnvironmentVisualProjections>,
    )>,
) {
    let definition_revision = source_events.read().fold(false, |changed, event| {
        changed || !matches!(event, AssetEvent::Unused { .. })
    });
    let Some(definitions) = active.get(&assets) else {
        return;
    };
    for (entity, daylight, environment, light, curves, projections) in &mut environments {
        let prepare = light.is_none()
            || curves.is_none()
            || definition_revision
            || active.is_changed()
            || environment.is_changed();
        if !prepare && !daylight.is_changed() && projections.is_some() {
            continue;
        }
        let Some(definition) = definitions.find_environment(environment.definition) else {
            continue;
        };
        if prepare {
            let next = EnvironmentLightCurves::new(definitions, definition);
            if let Some(mut light) = light {
                *light = next;
            } else {
                commands.entity(entity).insert(next);
            }
        }
        let prepared = prepare.then(|| EnvironmentVisualCurves::from_definition(definition));
        let next = prepared
            .as_ref()
            .or(curves.as_deref())
            .expect("visual curves prepared for this environment")
            .sample(definition, daylight.fraction);
        if let Some(prepared) = prepared {
            if let Some(mut curves) = curves {
                *curves = prepared;
            } else {
                commands.entity(entity).insert(prepared);
            }
        }
        if let Some(mut projections) = projections {
            projections.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

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
    settings: Res<GraphicsSettings>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut environments: Query<(
        Entity,
        Ref<Daylight>,
        &WorldEnvironment,
        Option<Ref<Weather>>,
        Option<Ref<WeatherTransition>>,
        Option<&EnvironmentPresentationPending>,
        Option<&Children>,
        Option<&EnvironmentVisuals>,
        Ref<EnvironmentVisualProjections>,
        Ref<EnvironmentLightCurves>,
        &mut AboveWaterFog,
        Ref<ObjectAmbientLight>,
        Option<&mut PrefabAmbientLightContribution>,
        &mut EnvironmentTerrainAmbientLight,
    )>,
    mut lights: Query<
        (
            &mut EnvironmentLight,
            &mut DirectionalLight,
            &mut Transform,
            Option<&mut EnvironmentSpecularLightColor>,
        ),
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
            Option<&mut PrefabModelTint>,
            Option<&mut EnvironmentCameraRelativeVisual>,
            Option<&EnvironmentTexture>,
            &mut Transform,
            &mut Visibility,
        ),
        (Without<EnvironmentSky>, Without<EnvironmentLight>),
    >,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let fog_quality = super::underwater_environment_presentation::fog_quality_for_view_distance(
        settings.view_distance,
    );
    for (
        entity,
        daylight,
        environment,
        weather,
        transition,
        pending,
        children,
        visuals,
        visual_projections,
        light_curves,
        mut fog,
        ambient,
        contribution,
        mut terrain_ambient,
    ) in &mut environments
    {
        let Some(definition) = definitions.find_environment(environment.definition) else {
            continue;
        };
        if !daylight.is_changed()
            && !ambient.is_changed()
            && !visual_projections.is_changed()
            && !light_curves.is_changed()
            && !weather.as_ref().is_some_and(|weather| weather.is_changed())
            && !settings.is_changed()
            && !transition.as_ref().is_some_and(DetectChanges::is_changed)
            && pending.is_none()
        {
            continue;
        }
        let fraction = daylight.fraction;
        let (light_multiplier, fog_multiplier) = weather.as_deref().map_or((1.0, 1.0), |weather| {
            calculate_weather_light_and_fog_multipliers(definitions, weather, transition.as_deref())
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
            let color = Color::linear_rgb(
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
            for child in children.into_iter().flat_map(|children| children.iter()) {
                if let Ok((mut marker, mut light, mut transform, specular)) = lights.get_mut(child)
                {
                    if marker.kind_rank != 0 {
                        continue;
                    }
                    marker.keyframe = sample.left_keyframe_index as u16;
                    light.color = color;
                    update_specular(&mut commands, child, specular, color);
                    light.illuminance = illuminance;
                    if direction != Vec3::ZERO {
                        transform.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, direction);
                    }
                }
            }
        }
        if light_rows.is_empty() {
            for child in children.into_iter().flat_map(|children| children.iter()) {
                let Ok((mut marker, mut light, mut transform, specular)) = lights.get_mut(child)
                else {
                    continue;
                };
                let Some(sampled_directional_light) =
                    light_curves.directional(marker.target, marker.kind_rank, None, fraction)
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
                update_specular(
                    &mut commands,
                    child,
                    specular,
                    Color::linear_rgb(
                        interpolate_f32(
                            left_keyframe.specular[0],
                            right_keyframe.specular[0],
                            interpolation_fraction,
                        ),
                        interpolate_f32(
                            left_keyframe.specular[1],
                            right_keyframe.specular[1],
                            interpolation_fraction,
                        ),
                        interpolate_f32(
                            left_keyframe.specular[2],
                            right_keyframe.specular[2],
                            interpolation_fraction,
                        ),
                    ),
                );
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
        let ambient = ambient.0;
        terrain_ambient.set_if_neq(EnvironmentTerrainAmbientLight(Some(
            light_curves.ambient(EnvironmentLightTarget::Terrain, fraction) * light_multiplier,
        )));
        if ambient.length_squared() > 0.0 {
            let next = PrefabAmbientLightContribution {
                color_linear: ambient,
                intensity: light_multiplier,
            };
            if let Some(mut current) = contribution {
                current.set_if_neq(next);
            } else {
                commands.entity(entity).insert(next);
            }
        } else if contribution.is_some() {
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
            fog.set_if_neq(AboveWaterFog {
                color,
                start,
                end,
                relative_to_far_plane: false,
            });
        }
        if fog_rows.is_empty() {
            if let Some(sampled_fog) = light_curves.fog_for_quality(fog_quality, fraction) {
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
                fog.set_if_neq(AboveWaterFog {
                    color,
                    start: start_factor,
                    end: end_factor,
                    relative_to_far_plane: true,
                });
            } else {
                fog.set_if_neq(AboveWaterFog::default());
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
            for child in children.into_iter().flat_map(|children| children.iter()) {
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
        let mut samples =
            visual_samples.iter_many_mut(visuals.into_iter().flat_map(RelationshipTarget::iter));
        while let Some(item) = samples.fetch_next() {
            let Ok((sample, tint, camera_relative, texture, mut transform, mut visibility)) = item
            else {
                continue;
            };
            let Some(projection) = visual_projections.0.get(sample.0 as usize) else {
                continue;
            };
            visibility.set_if_neq(if projection.active {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
            let scale = Vec3::splat(projection.scale);
            if transform.scale != scale {
                transform.scale = scale;
            }
            if let Some(mut camera_relative) = camera_relative {
                if camera_relative.projected_scale != projection.scale {
                    camera_relative.projected_scale = projection.scale;
                }
            }
            if let Some(mut tint) = tint {
                if tint.0 != projection.color {
                    tint.0 = projection.color;
                }
            }
            if let Some(texture) = texture {
                if let Some(mut material) = texture
                    .material
                    .as_ref()
                    .and_then(|handle| materials.get_mut(handle))
                {
                    if material.base_color != projection.color {
                        material.base_color = projection.color;
                        material.emissive = projection.color.to_linear();
                    }
                }
            }
        }
        commands
            .entity(entity)
            .remove::<EnvironmentPresentationPending>();
    }
}

fn update_specular(
    commands: &mut Commands,
    entity: Entity,
    current: Option<Mut<EnvironmentSpecularLightColor>>,
    color: Color,
) {
    if let Some(mut current) = current {
        if current.0 != color {
            current.0 = color;
        }
    } else {
        commands
            .entity(entity)
            .insert(EnvironmentSpecularLightColor(color));
    }
}
