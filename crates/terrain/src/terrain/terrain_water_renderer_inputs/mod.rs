use crate::terrain::terrain_water_material_binding::bind_authored_water_frame_base_tints;
use bevy::camera::CameraProjection;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::math::Vec3A;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy::transform::helper::TransformHelper;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::material::runtime::resolved_effect_transform_bindings::EffectRenderViewTransforms;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::model_render::authored_model_material_pass_projection::WATER_FALLBACK_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER;
use openzt2_world_state::model_render::authored_model_material_pass_projection::WATER_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER;
use openzt2_world_state::model_render::authored_model_material_pass_projection::WATER_REFRACTION_EFFECT_PASS_RENDER_VIEW_LAYER;
use openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream;
use openzt2_zoo_state::environment::environment_state_types::WaterEnvironmentMapSample;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::NativeTerrainWaterGeometricWaveShaderState;
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::WaterWakePresentationPath;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionQuery;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::AuthoredTerrainWaterRendererTargets;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::AuthoredTerrainWaterSurfaceEffectPass;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterLowDetailTextureState;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterScumScroll;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterSurfacePassKind;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterSurfaceRegion;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterWaveStateKey;
use openzt2_zoo_state::terrain::terrain_water_underwater_particle_types::TerrainWaterRegionLight;
use openzt2_zoo_state::terrain::terrain_water_view_state::{
    TerrainWaterRegionFade, TerrainWaterRegionVisibility, TerrainWaterViewState,
};

pub(super) fn initialize_authored_terrain_water_region_material_state(
    mut commands: Commands,
    mut random: ResMut<NativeVisualRandomStream>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    mut initialized: Query<&mut NativeTerrainWaterGeometricWaveShaderState>,
) {
    for (entity, source) in regions.iter() {
        let Some(presentation) = terrain_assets
            .get(source.terrain())
            .and_then(|asset| source.biome(asset))
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        if let Ok(mut state) = initialized.get_mut(entity) {
            state.reconfigure(
                presentation.surface_material.geometric_waves,
                presentation.surface_material.ripple_waves,
                &mut || random.unit_f64(),
            );
            continue;
        }
        commands.entity(entity).insert(
            NativeTerrainWaterGeometricWaveShaderState::from_authored_geometric_wave(
                presentation.surface_material.geometric_waves,
                presentation.surface_material.ripple_waves,
                &mut || random.unit_f64(),
            ),
        );
    }
}

pub(super) fn advance_authored_terrain_water_surface_effect_state(
    time: Res<Time>,
    mut random: ResMut<NativeVisualRandomStream>,
    mut targets: Option<ResMut<AuthoredTerrainWaterRendererTargets>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    mut waves: Query<(Entity, &mut NativeTerrainWaterGeometricWaveShaderState)>,
    regions: TerrainWaterRegionQuery,
    mut scrolls: Query<(Entity, &mut TerrainWaterScumScroll)>,
    mut low_textures: Query<(Entity, &mut TerrainWaterLowDetailTextureState)>,
) {
    let Some(ref mut targets) = targets else {
        return;
    };
    targets.elapsed_seconds += time.delta_secs();
    for (entity, mut wave_state) in &mut waves {
        if regions.get(entity).is_none() {
            continue;
        }
        wave_state.advance_wave_channels(time.delta_secs(), &mut || random.unit_f64());
    }
    for (entity, mut state) in &mut low_textures {
        let Some(source) = regions.get(entity) else {
            continue;
        };
        let Some(water) = terrain_assets
            .get(source.terrain())
            .and_then(|asset| source.biome(asset))
            .and_then(|biome| biome.water_presentation.as_ref())
        else {
            continue;
        };
        for (index, animation) in water.texture_animations.iter().enumerate() {
            let now = targets.elapsed_seconds;
            let start = *state.start_seconds[index].get_or_insert(now);
            if !animation.scroll_texture {
                continue;
            }
            let mut elapsed = now - start;
            let interval = animation.scroll_interval;
            if elapsed >= interval {
                elapsed = interval;
                state.start_seconds[index] = Some(now);
                if animation.ping_pong {
                    state.reverse[index] = !state.reverse[index];
                }
            }
            let fraction = elapsed / interval;
            let start_min = Vec2::from_array(animation.start_minimum_uv);
            let start_max = Vec2::from_array(animation.start_maximum_uv);
            let end_min = Vec2::from_array(animation.end_minimum_uv);
            let end_max = Vec2::from_array(animation.end_maximum_uv);
            let (minimum, maximum) = if state.reverse[index] {
                (
                    end_min - (end_min - start_min) * fraction,
                    end_max - (end_max - start_max) * fraction,
                )
            } else {
                (
                    start_min + (end_min - start_min) * fraction,
                    start_max + (end_max - start_max) * fraction,
                )
            };
            let scale = maximum - minimum;
            state.transforms[index] = Mat4::from_cols(
                Vec4::X * scale.x,
                Vec4::Y * scale.y,
                Vec4::Z,
                minimum.extend(0.0).extend(1.0),
            );
        }
    }
    for (entity, mut scroll) in &mut scrolls {
        let Some(source) = regions.get(entity) else {
            continue;
        };
        let Some(scum) = terrain_assets
            .get(source.terrain())
            .and_then(|asset| source.biome(asset))
            .and_then(|biome| biome.water_presentation.as_ref())
            .and_then(|presentation| presentation.surface_material.configuration.scum.as_ref())
        else {
            continue;
        };
        let (Some(bottom_size), Some(top_size), Some(bottom_rate), Some(top_rate)) = (
            scum.bottom_size,
            scum.top_size,
            scum.bottom_rate,
            scum.top_rate,
        ) else {
            continue;
        };
        let rates = bottom_rate.into_iter().chain(top_rate);
        let sizes = bottom_size.into_iter().chain(top_size);
        let mut channels = scroll.offsets.to_array();
        for ((offset, rate), size) in channels.iter_mut().zip(rates).zip(sizes) {
            *offset += if size == 0.0 { rate } else { rate / size } * time.delta_secs();
            if *offset > 1.0 {
                *offset -= 1.0;
            }
            if *offset < 0.0 {
                *offset += 1.0;
            }
        }
        scroll.offsets = Vec4::from_array(channels);
    }
}

pub(super) fn update_authored_terrain_water_surface_visibility(
    mut commands: Commands,
    phase: Res<State<openzt2_game_assets::application_lifecycle::GamePhase>>,
    zoo_camera: Query<(Entity, &Camera, &Projection), With<ZooCamera>>,
    transforms: TransformHelper,
    waves: Query<&NativeTerrainWaterGeometricWaveShaderState>,
    groups: Query<&TerrainWaterSurfaceRegion>,
    region_sources: TerrainWaterRegionQuery,
    materials: Res<Assets<EffectPassMaterial>>,
    mut regions: Query<(
        Entity,
        &mut TerrainWaterRegionVisibility,
        Option<&WaterWakePresentationPath>,
    )>,
    mut surfaces: Query<(
        &AuthoredTerrainWaterSurfaceEffectPass,
        &TerrainWaterWaveStateKey,
        &ChildOf,
        &mut Visibility,
        &MeshMaterial3d<EffectPassMaterial>,
        &TerrainWaterSurfacePassKind,
    )>,
) {
    let capable: bevy::platform::collections::HashMap<_, _> = surfaces
        .iter()
        .filter_map(|(_, key, parent, _, material, kind)| {
            let material = materials.get(material)?;
            if !kind.uses_stitch_geometry()
                && !["Frequency", "Phase", "Amplitude", "PosX", "PosY", "K"]
                    .iter()
                    .all(|name| material.has_vertex_float_effect_semantic(name))
            {
                return None;
            }
            waves
                .get(key.0)
                .ok()
                .and_then(|_| groups.get(parent.parent()).ok())
                .and_then(|group| {
                    region_sources.get(group.0).map(|_| {
                        (
                            group.0,
                            if kind.uses_stitch_geometry() {
                                WaterWakePresentationPath::TerrainStitch
                            } else {
                                WaterWakePresentationPath::GeometricWave
                            },
                        )
                    })
                })
        })
        .collect();
    for (entity, mut region, path) in &mut regions {
        region.0 = false;
        if let Some(capability) = capable.get(&entity) {
            if path != Some(capability) {
                commands.entity(entity).insert(*capability);
            }
        } else if path.is_some() {
            commands
                .entity(entity)
                .remove::<WaterWakePresentationPath>();
        }
    }
    let frustum = zoo_camera
        .single()
        .ok()
        .filter(|(_, camera, _)| {
            camera.is_active
                && *phase.get() == openzt2_game_assets::application_lifecycle::GamePhase::InGame
        })
        .and_then(|(entity, _, projection)| {
            transforms
                .compute_global_transform(entity)
                .ok()
                .map(|transform| projection.compute_frustum(&transform))
        });
    for (surface, key, parent, mut visibility, _, kind) in &mut surfaces {
        let active_owner = groups
            .get(parent.parent())
            .ok()
            .is_some_and(|group| region_sources.get(group.0).is_some());
        let visible = active_owner
            && frustum.as_ref().is_some_and(|frustum| {
                if kind.uses_stitch_geometry() {
                    frustum.intersects_obb_identity(&Aabb::from_min_max(
                        Vec3::new(
                            surface.horizontal_minimum.x,
                            surface.surface_plane_height,
                            surface.horizontal_minimum.y,
                        ),
                        Vec3::new(
                            surface.horizontal_maximum.x,
                            surface.surface_plane_height,
                            surface.horizontal_maximum.y,
                        ),
                    ))
                } else {
                    waves
                        .get(key.0)
                        .ok()
                        .is_some_and(|wave| surface.intersects_camera_frustum(frustum, wave))
                }
            });
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if visible {
            if let Ok(group) = groups.get(parent.parent()) {
                if let Ok((_, mut region, _)) = regions.get_mut(group.0) {
                    region.0 = true;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update_authored_terrain_water_renderer_inputs(
    view: Option<Res<TerrainWaterViewState>>,
    environment_map: Option<Res<WaterEnvironmentMapSample>>,
    waves: Query<&NativeTerrainWaterGeometricWaveShaderState>,
    phase: Res<State<openzt2_game_assets::application_lifecycle::GamePhase>>,
    mut targets: Option<ResMut<AuthoredTerrainWaterRendererTargets>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    zoo_camera: Query<(Entity, &Camera, Ref<Projection>), With<ZooCamera>>,
    scene_bounds: Query<(&Aabb, &GlobalTransform, Option<&RenderLayers>), With<Mesh3d>>,
    mut camera_transforms: ParamSet<(
        TransformHelper,
        Query<(&mut Camera, &mut Projection, &mut Transform), Without<ZooCamera>>,
    )>,
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    region_sources: TerrainWaterRegionQuery,
    regions: Query<(
        Option<&TerrainWaterScumScroll>,
        &TerrainWaterRegionFade,
        Option<&TerrainWaterRegionLight>,
        Option<&TerrainWaterLowDetailTextureState>,
    )>,
    surface_groups: Query<&TerrainWaterSurfaceRegion>,
    mut water_surfaces: Query<
        (
            &MeshMaterial3d<EffectPassMaterial>,
            &AuthoredTerrainWaterSurfaceEffectPass,
            &TerrainWaterWaveStateKey,
            &ChildOf,
            &mut Visibility,
            &TerrainWaterSurfacePassKind,
        ),
        With<AuthoredTerrainWaterSurfaceEffectPass>,
    >,
) {
    let Some(ref mut targets) = targets else {
        return;
    };
    let camera = zoo_camera
        .single()
        .ok()
        .filter(|(_, camera, _)| {
            camera.is_active
                && *phase.get() == openzt2_game_assets::application_lifecycle::GamePhase::InGame
        })
        .and_then(|(entity, camera, projection)| {
            // Update runs before Bevy's transform propagation. Read the current
            // hierarchy, not last frame's GlobalTransform, when deciding demand.
            camera_transforms
                .p0()
                .compute_global_transform(entity)
                .ok()
                .map(|transform| (camera, projection, transform))
        });
    targets.active_water_plane_height = view
        .as_ref()
        .filter(|view| view.selection_resolved())
        .and_then(|view| view.surface_height());
    for (_, _, _, parent, mut visibility, _) in &mut water_surfaces {
        let resolved = surface_groups
            .get(parent.parent())
            .ok()
            .and_then(|owner| {
                region_sources
                    .get(owner.0)
                    .and_then(|_| regions.get(owner.0).ok())
            })
            .is_some_and(|(_, _, light, _)| light.is_some());
        if !resolved {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
    let render_view = camera.as_ref().and_then(|(_, projection, transform)| {
        zoo_camera.single().ok().map(|(entity, _, _)| {
            (
                entity,
                EffectRenderViewTransforms::from_camera(transform, projection),
            )
        })
    });
    for (handle, _, key, _, mut visibility, kind) in &mut water_surfaces {
        if !kind.uses_stitch_geometry() || *visibility == Visibility::Hidden {
            continue;
        }
        let Some((_, _, Some(light), Some(textures))) = regions.get(key.0).ok() else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let Some(material) = effect_pass_materials.get_mut_untracked(handle) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        if *kind == TerrainWaterSurfacePassKind::MediumEnvironment {
            let Some(texture) = environment_map
                .as_deref()
                .and_then(WaterEnvironmentMapSample::texture)
            else {
                visibility.set_if_neq(Visibility::Hidden);
                continue;
            };
            let Some((view_entity, render_view)) = &render_view else {
                visibility.set_if_neq(Visibility::Hidden);
                continue;
            };
            material.replace_texture_asset_at_d3d9_stage(0, texture.clone());
            material.bind_fixed_function_transform_expressions(
                Mat4::from_cols(Vec4::X, -Vec4::Z, Vec4::Y, Vec4::W),
                *view_entity,
                render_view,
                true,
                true,
                None,
                None,
                None,
            );
        } else {
            material.set_fixed_function_ambient_colour(light.raw_base_rgba());
            material.set_fixed_function_texture_transform(0, textures.transforms[0]);
            if *kind == TerrainWaterSurfacePassKind::MediumDetail {
                material.set_fixed_function_texture_transform(1, textures.transforms[1]);
            }
        }
        material.write_changed_fixed_function_registers_to_persistent_buffer(
            &mut effect_uniform_buffers,
        );
        material.write_changed_fixed_function_transforms_to_persistent_buffer(
            &mut effect_uniform_buffers,
        );
    }
    let enabled = water_surfaces.iter().any(|(_, _, _, _, visibility, kind)| {
        *visibility != Visibility::Hidden && !kind.uses_stitch_geometry()
    });
    let fallback_enabled = water_surfaces
        .iter()
        .any(|(_, _, _, parent, visibility, kind)| {
            *visibility != Visibility::Hidden
                && match kind {
                    TerrainWaterSurfacePassKind::Scum => true,
                    TerrainWaterSurfacePassKind::LowDetail
                    | TerrainWaterSurfacePassKind::MediumDetail
                    | TerrainWaterSurfacePassKind::MediumEnvironment => false,
                    TerrainWaterSurfacePassKind::FallbackReflection => surface_groups
                        .get(parent.parent())
                        .ok()
                        .and_then(|owner| {
                            region_sources
                                .get(owner.0)
                                .and_then(|_| regions.get(owner.0).ok())
                        })
                        .is_some_and(|(_, fade, light, _)| {
                            light.is_some() && fade.fraction() < 1.0
                        }),
                    _ => false,
                }
        });
    let refraction_enabled = camera.as_ref().is_some_and(|(_, _, transform)| {
        targets
            .active_water_plane_height
            .is_some_and(|height| transform.translation().y >= height)
    });
    let mut activation_changed = false;
    for entity in [
        targets.bump_camera,
        targets.reflection_camera,
        targets.refraction_camera,
        targets.fallback_reflection_camera,
    ] {
        if let Ok((mut camera, _, _)) = camera_transforms.p1().get_mut(entity) {
            let active = enabled
                && (entity != targets.refraction_camera || refraction_enabled)
                && (entity != targets.reflection_camera
                    || targets.active_water_plane_height.is_some())
                && (entity != targets.fallback_reflection_camera || fallback_enabled);
            if camera.is_active != active {
                camera.is_active = active;
                activation_changed = true;
            }
        }
    }
    if activation_changed || phase.is_changed() {
        info!(target: "openzt2_water_targets", enabled,
            surface_pass_count = water_surfaces.iter().count(),
            visible_surface_pass_count = water_surfaces.iter()
                .filter(|(_, _, _, _, visibility, _)| **visibility != Visibility::Hidden).count(),
            "water render-target demand");
    }
    if !enabled {
        return;
    }
    let Some((_, zoo_projection, zoo_global_transform)) = camera else {
        for entity in [
            targets.bump_camera,
            targets.reflection_camera,
            targets.refraction_camera,
            targets.fallback_reflection_camera,
        ] {
            if let Ok((mut camera, _, _)) = camera_transforms.p1().get_mut(entity) {
                camera.is_active = false;
            }
        }
        return;
    };
    let active_water_plane_height = targets.active_water_plane_height.unwrap_or(0.0);
    let zoo_transform = zoo_global_transform.compute_transform();
    for handle in &targets.water_bump_combination_materials {
        let Some(material) = effect_pass_materials.get_mut_untracked(handle) else {
            continue;
        };
        let first_axis_offset = (targets.elapsed_seconds * 0.1).fract();
        let second_axis_offset = (targets.elapsed_seconds * 0.05).fract();
        material.bind_float_vector_effect_semantic(
            "TextureOffset",
            Vec4::new(
                first_axis_offset,
                first_axis_offset,
                second_axis_offset,
                second_axis_offset,
            ),
        );
        material
            .write_programmable_float_registers_to_persistent_buffers(&mut effect_uniform_buffers);
    }
    let capture_layers = RenderLayers::from_layers(&[
        0,
        TERRAIN_RENDER_LAYER,
        WATER_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER,
        WATER_REFRACTION_EFFECT_PASS_RENDER_VIEW_LAYER,
        WATER_FALLBACK_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER,
    ]);
    let mut bounds_minimum = Vec3::splat(f32::INFINITY);
    let mut bounds_maximum = Vec3::splat(f32::NEG_INFINITY);
    for (bounds, transform, layers) in &scene_bounds {
        if layers.is_some_and(|layers| !layers.intersects(&capture_layers)) {
            continue;
        }
        let centre = transform.transform_point(Vec3::from(bounds.center));
        let half = Vec3::from(bounds.half_extents);
        let matrix = transform.affine().matrix3;
        let extent = Vec3::from(matrix.x_axis.abs()) * half.x
            + Vec3::from(matrix.y_axis.abs()) * half.y
            + Vec3::from(matrix.z_axis.abs()) * half.z;
        bounds_minimum = bounds_minimum.min(centre - extent);
        bounds_maximum = bounds_maximum.max(centre + extent);
    }
    let fallback_capture = construct_water_capture_projection(
        zoo_transform,
        &zoo_projection,
        0.0,
        bounds_minimum,
        bounds_maximum,
        true,
    );
    let primary_capture = construct_water_capture_projection(
        zoo_transform,
        &zoo_projection,
        active_water_plane_height,
        bounds_minimum,
        bounds_maximum,
        true,
    );
    if let Ok((mut camera, mut projection, mut transform)) = camera_transforms
        .p1()
        .get_mut(targets.fallback_reflection_camera)
    {
        camera.is_active = fallback_enabled && fallback_capture.is_some();
        if let Some((frame, capture, extent)) = &fallback_capture {
            *projection = Projection::custom(capture.clone());
            transform.set_if_neq(*frame);
            targets.fallback_reflection_world_extent = *extent;
        }
    }
    let Some((reflected_transform, reflection_projection, reflection_extent)) =
        primary_capture.or_else(|| fallback_capture.clone())
    else {
        for entity in [
            targets.reflection_camera,
            targets.refraction_camera,
            targets.fallback_reflection_camera,
        ] {
            if let Ok((mut camera, _, _)) = camera_transforms.p1().get_mut(entity) {
                camera.is_active = false;
            }
        }
        return;
    };
    let refraction_capture = construct_water_capture_projection(
        zoo_transform,
        &zoo_projection,
        active_water_plane_height,
        bounds_minimum,
        bounds_maximum,
        false,
    );
    if let Ok((mut camera, mut projection, mut transform)) =
        camera_transforms.p1().get_mut(targets.refraction_camera)
    {
        camera.is_active = refraction_enabled && refraction_capture.is_some();
        if let Some((capture_transform, capture_projection, _)) = &refraction_capture {
            *projection = Projection::custom(capture_projection.clone());
            transform.set_if_neq(*capture_transform);
        }
    }
    if let Ok((_, mut projection, mut transform)) =
        camera_transforms.p1().get_mut(targets.reflection_camera)
    {
        *projection = Projection::custom(reflection_projection.clone());
        transform.set_if_neq(reflected_transform);
    }
    targets.reflection_world_extent = reflection_extent;
    if let Some((_, _, extent)) = &refraction_capture {
        targets.refraction_world_extent = *extent;
    }
    let clip_from_view = zoo_projection.get_clip_from_view();
    let source_from_local = Mat4::from_cols(Vec4::X, -Vec4::Z, Vec4::Y, Vec4::W);
    let world_to_view = zoo_transform.to_matrix().inverse();
    let source_to_clip = clip_from_view * world_to_view * source_from_local;
    let source_to_refraction_clip =
        refraction_capture
            .as_ref()
            .map_or(Mat4::IDENTITY, |(transform, projection, _)| {
                projection.get_clip_from_view()
                    * transform.to_matrix().inverse()
                    * source_from_local
            });
    let reflection_world_to_view = reflected_transform.to_matrix().inverse();
    let source_to_reflection_clip =
        reflection_projection.get_clip_from_view() * reflection_world_to_view * source_from_local;
    let clip_to_uv = Mat4::from_cols(
        Vec4::new(0.5, 0.0, 0.0, 0.0),
        Vec4::new(0.0, -0.5, 0.0, 0.0),
        Vec4::Z,
        Vec4::new(0.5, 0.5, 0.0, 1.0),
    );
    let camera_position_source = Vec3::new(
        zoo_transform.translation.x,
        -zoo_transform.translation.z,
        zoo_transform.translation.y,
    );
    let camera_across_source = Vec3::new(
        zoo_transform.right().x,
        -zoo_transform.right().z,
        zoo_transform.right().y,
    );
    let camera_up_source = Vec3::new(
        zoo_transform.up().x,
        -zoo_transform.up().z,
        zoo_transform.up().y,
    );
    for (material_handle, surface, wave_key, parent, mut visibility, pass_kind) in
        &mut water_surfaces
    {
        if pass_kind.uses_stitch_geometry() {
            continue;
        }
        let Ok(geometric_wave_state) = waves.get(wave_key.0) else {
            continue;
        };
        if *visibility == Visibility::Hidden {
            continue;
        }
        let Some(material) = effect_pass_materials.get_mut_untracked(material_handle) else {
            continue;
        };
        let fallback = *pass_kind == TerrainWaterSurfacePassKind::FallbackReflection
            || *pass_kind == TerrainWaterSurfacePassKind::Scum;
        let reflection_size = Vec2::splat(25.0)
            / if fallback {
                targets.fallback_reflection_world_extent
            } else {
                targets.reflection_world_extent
            };
        let refraction_size = Vec2::splat(25.0) / targets.refraction_world_extent;
        material.bind_float_vector_effect_semantic(
            "ReflectSize",
            Vec4::new(reflection_size.x, reflection_size.y, 1.0, 0.0),
        );
        material.bind_float_vector_effect_semantic(
            "RefractSize",
            Vec4::new(refraction_size.x, refraction_size.y, 1.0, 0.0),
        );
        let Some((scroll, fade, light, _)) =
            surface_groups.get(parent.parent()).ok().and_then(|owner| {
                region_sources
                    .get(owner.0)
                    .and_then(|_| regions.get(owner.0).ok())
            })
        else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let Some(light) = light else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let ambient_colour = light.raw_base_rgba().truncate();
        let Some(source) = region_sources.get(wave_key.0) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        if let Some(presentation) = terrain_assets
            .get(source.terrain())
            .and_then(|asset| source.biome(asset))
            .and_then(|biome| biome.water_presentation.as_ref())
        {
            let fraction = fade.fraction();
            let above = zoo_transform.translation.y > surface.surface_plane_height;
            let transitioning = fraction > 0.0 && fraction < 1.0;
            let submitted = match pass_kind {
                TerrainWaterSurfacePassKind::Scum => true,
                TerrainWaterSurfacePassKind::LowDetail
                | TerrainWaterSurfacePassKind::MediumDetail
                | TerrainWaterSurfacePassKind::MediumEnvironment => false,
                TerrainWaterSurfacePassKind::FallbackReflection => {
                    transitioning
                        || (fraction <= 0.0
                            && presentation.surface_material.reflection.strength_percent > 5.0)
                }
                TerrainWaterSurfacePassKind::Refraction => fraction > 0.0 && above,
                TerrainWaterSurfacePassKind::Reflection => {
                    transitioning
                        || (fraction >= 1.0
                            && presentation.surface_material.reflection.strength_percent > 5.0)
                }
            };
            if !submitted {
                visibility.set_if_neq(Visibility::Hidden);
                continue;
            }
            bind_authored_water_frame_base_tints(
                material,
                presentation,
                ambient_colour,
                !above || fallback,
                (!fallback && fraction > 0.0).then_some(fraction),
                (!above && scroll.is_none() && fraction > 0.0).then_some(fraction),
                view.as_ref()
                    .map_or(0.0, |view| view.training_opacity_mix()),
            );
            if let Some(scroll) = scroll {
                let offsets = scroll.offsets;
                if let Some((bottom, top)) = presentation
                    .surface_material
                    .configuration
                    .scum
                    .as_ref()
                    .and_then(|scum| scum.bottom_size.zip(scum.top_size))
                {
                    material.bind_float_vector_effect_semantic(
                        "ScumOffsetLo",
                        Vec4::new(bottom[0].recip(), bottom[1].recip(), offsets.x, offsets.y),
                    );
                    material.bind_float_vector_effect_semantic(
                        "ScumOffsetHi",
                        Vec4::new(top[0].recip(), top[1].recip(), offsets.z, offsets.w),
                    );
                    material.bind_float_vector_effect_semantic("ScumAmbient", Vec4::W);
                    material.bind_float_vector_effect_semantic(
                        "ScumLightDir",
                        light.source_light_direction().extend(0.0),
                    );
                    material.bind_float_vector_effect_semantic(
                        "ScumDiffuse",
                        ambient_colour.extend(1.0),
                    );
                    material.bind_float_vector_effect_semantic(
                        "ReflectOpacity",
                        Vec4::new(
                            1.0,
                            0.5 - 1.5
                                * view
                                    .as_ref()
                                    .map_or(0.0, |view| view.training_opacity_mix()),
                            0.0,
                            0.05,
                        ),
                    );
                }
            }
        }
        material.bind_float_matrix_effect_semantic("WorldToNDC", source_to_clip);
        material.bind_float_matrix_effect_semantic(
            "WorldToRefract",
            clip_to_uv * source_to_refraction_clip,
        );
        material.bind_float_matrix_effect_semantic(
            "WorldToReflect",
            clip_to_uv
                * if fallback {
                    fallback_capture
                        .as_ref()
                        .map_or(Mat4::IDENTITY, |(frame, projection, _)| {
                            projection.get_clip_from_view()
                                * frame.to_matrix().inverse()
                                * source_from_local
                        })
                } else {
                    source_to_reflection_clip
                },
        );
        material.replace_texture_asset_for_effect_semantic(
            "EnvReflect",
            if fallback {
                targets.fallback_reflection.clone()
            } else {
                targets.reflection.clone()
            },
        );
        material.bind_float_vector_effect_semantic("CameraPos", camera_position_source.extend(1.0));
        material
            .bind_float_vector_effect_semantic("CameraAcross", camera_across_source.extend(0.0));
        material.bind_float_vector_effect_semantic("CameraUp", camera_up_source.extend(0.0));
        geometric_wave_state
            .bind_wave_channels_to_effect_pass(surface.surface_plane_height, material);
        material
            .write_programmable_float_registers_to_persistent_buffers(&mut effect_uniform_buffers);
    }
}

#[derive(Clone, Debug)]
struct TerrainWaterCaptureProjection {
    slopes: Vec4,
    near: f32,
    far: f32,
    invert_horizontal: bool,
}

impl CameraProjection for TerrainWaterCaptureProjection {
    fn get_clip_from_view(&self) -> Mat4 {
        let width = self.slopes.y - self.slopes.x;
        let height = self.slopes.w - self.slopes.z;
        let depth = self.far - self.near;
        let horizontal_sign = if self.invert_horizontal { -1.0 } else { 1.0 };
        Mat4::from_cols(
            Vec4::new(horizontal_sign * 2.0 / width, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 2.0 / height, 0.0, 0.0),
            Vec4::new(
                horizontal_sign * (self.slopes.y + self.slopes.x) / width,
                (self.slopes.w + self.slopes.z) / height,
                self.near / depth,
                -1.0,
            ),
            Vec4::new(0.0, 0.0, self.near * self.far / depth, 0.0),
        )
    }

    fn get_clip_from_view_for_sub(&self, view: &bevy::camera::SubCameraView) -> Mat4 {
        let minimum = view.offset / view.full_size.as_vec2();
        let maximum = (view.offset + view.size.as_vec2()) / view.full_size.as_vec2();
        let width = self.slopes.y - self.slopes.x;
        let height = self.slopes.w - self.slopes.z;
        Self {
            slopes: Vec4::new(
                self.slopes.x + minimum.x * width,
                self.slopes.x + maximum.x * width,
                self.slopes.w - maximum.y * height,
                self.slopes.w - minimum.y * height,
            ),
            ..self.clone()
        }
        .get_clip_from_view()
    }

    fn update(&mut self, _width: f32, _height: f32) {}

    fn far(&self) -> f32 {
        self.far
    }

    fn get_frustum_corners(&self, near: f32, far: f32) -> [Vec3A; 8] {
        std::array::from_fn(|index| {
            let distance = if index < 4 { near.abs() } else { far.abs() };
            let x = if index % 4 < 2 {
                self.slopes.y
            } else {
                self.slopes.x
            };
            let y = if matches!(index % 4, 1 | 2) {
                self.slopes.w
            } else {
                self.slopes.z
            };
            Vec3A::new(x * distance, y * distance, -distance)
        })
    }
}

fn construct_water_capture_projection(
    source: Transform,
    projection: &Projection,
    plane_height: f32,
    minimum: Vec3,
    maximum: Vec3,
    reflection: bool,
) -> Option<(Transform, TerrainWaterCaptureProjection, Vec2)> {
    if !minimum.is_finite()
        || !maximum.is_finite()
        || (!reflection && source.translation.y < plane_height)
    {
        return None;
    }
    let mut frame = source;
    if reflection && frame.translation.y >= plane_height {
        frame.translation.y = 2.0 * plane_height - frame.translation.y;
        let forward = source.forward();
        let up = source.up();
        frame = frame.looking_to(
            Vec3::new(forward.x, -forward.y, forward.z),
            Vec3::new(up.x, -up.y, up.z),
        );
    }
    let far_distance = (0..8)
        .map(|corner| {
            let point = Vec3::new(
                if corner & 1 == 0 {
                    minimum.x
                } else {
                    maximum.x
                },
                if corner & 2 == 0 {
                    minimum.y
                } else {
                    maximum.y
                },
                if corner & 4 == 0 {
                    minimum.z
                } else {
                    maximum.z
                },
            );
            (point - frame.translation).dot(*frame.forward())
        })
        .fold(0.0_f32, f32::max)
        .min(projection.far());
    if far_distance <= 0.1 {
        return None;
    }
    let normal = if reflection { Vec3::Y } else { Vec3::NEG_Y };
    let candidate = [*frame.right(), *frame.up(), *frame.forward()]
        .into_iter()
        .min_by(|left, right| left.dot(normal).abs().total_cmp(&right.dot(normal).abs()))?;
    let up = candidate.cross(normal).normalize();
    let capture = Transform::from_translation(frame.translation).looking_to(normal, up);
    let near = (plane_height - frame.translation.y).abs();
    if near == 0.0 {
        return None;
    }
    let corners = projection.get_frustum_corners(-0.1, -far_distance);
    let mut slopes = Vec4::new(
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
    );
    for (index, corner) in [7, 4, 5, 6].into_iter().enumerate() {
        let mut point = frame.transform_point(Vec3::from(corners[corner]));
        let delta = point - frame.translation;
        let numerator = plane_height - frame.translation.y;
        if numerator * delta.y > 0.000_001 {
            point = frame.translation + delta * (numerator / delta.y).min(1.0);
        }
        point.y = plane_height;
        let local = point - capture.translation;
        let x = local.dot(*capture.right()) / near;
        let y = local.dot(*capture.up()) / near;
        slopes.x = slopes.x.min(x);
        slopes.y = slopes.y.max(x);
        slopes.z = slopes.z.min(y - if index == 0 { 0.0 } else { 1.0 });
        slopes.w = slopes.w.max(y);
    }
    for (lower, upper) in [(0, 1), (2, 3)] {
        let extent = near * (slopes[upper] - slopes[lower]);
        if extent < 50.0 {
            let margin = (50.0 - extent) * 0.5 / near;
            slopes[lower] -= margin;
            slopes[upper] += margin;
        }
    }
    let extent = Vec2::new(slopes.y - slopes.x, slopes.w - slopes.z) * near;
    Some((
        capture,
        TerrainWaterCaptureProjection {
            slopes,
            near,
            far: near + 1000.0,
            invert_horizontal: reflection,
        },
        extent,
    ))
}

#[cfg(test)]
mod water_target_demand_tests {
    use super::*;
    use bevy::camera::CameraProjection;
    use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::NativeTerrainWaterGeometricWaveShaderState;

    #[test]
    fn capture_near_plane_is_the_water_surface_with_reversed_depth() {
        let source = Transform::from_xyz(0.0, 20.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z);
        let projection = Projection::Perspective(PerspectiveProjection::default());
        for reflection in [false, true] {
            let (camera, capture, extent) = construct_water_capture_projection(
                source,
                &projection,
                3.0,
                Vec3::splat(-100.0),
                Vec3::splat(100.0),
                reflection,
            )
            .expect("the scene lies in the source view");
            let near_world = camera.transform_point(Vec3::new(0.0, 0.0, -capture.near));
            assert!((near_world.y - 3.0).abs() < 0.0001);
            let near_clip = capture.get_clip_from_view() * Vec4::new(0.0, 0.0, -capture.near, 1.0);
            let far_clip = capture.get_clip_from_view() * Vec4::new(0.0, 0.0, -capture.far, 1.0);
            assert!((near_clip.z / near_clip.w - 1.0).abs() < 0.0001);
            assert!((far_clip.z / far_clip.w).abs() < 0.0001);
            assert!(extent.x >= 49.999 && extent.y >= 49.999);
        }
    }

    #[test]
    fn water_bounds_leave_and_reenter_the_view_without_a_centre_ray_hit() {
        use openzt2_game_data::terrain::TerrainWaterGeometricWavePresentation;
        use openzt2_game_data::terrain::TerrainWaterRippleWavePresentation;
        let mut random = NativeVisualRandomStream::from_seed(7);
        let wave_state = NativeTerrainWaterGeometricWaveShaderState::from_authored_geometric_wave(
            TerrainWaterGeometricWavePresentation {
                minimum_amplitude_percent: 10.0,
                maximum_amplitude_percent: 20.0,
                chop_percent: 25.0,
            },
            TerrainWaterRippleWavePresentation {
                lifespan_seconds: 4.0,
                startup_seconds: 1.0,
                minimum_amplitude_percent: 10.0,
                maximum_amplitude_percent: 20.0,
                chop_percent: 25.0,
                speed_metres_per_second: 1.0,
                ramp_minimum_metres: 1.0,
                ramp_maximum_metres: 5.0,
            },
            &mut || random.unit_f64(),
        );
        let projection = PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio: 1.0,
            near: 0.1,
            far: 100.0,
            ..default()
        };
        let camera = GlobalTransform::from(
            Transform::from_xyz(0.0, 20.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z),
        );
        let surface = AuthoredTerrainWaterSurfaceEffectPass {
            surface_plane_height: 0.0,
            horizontal_minimum: Vec2::new(15.0, 0.0),
            horizontal_maximum: Vec2::new(18.0, 2.0),
            maximum_absolute_input_height: 0.0,
        };
        assert!(
            surface.intersects_camera_frustum(&projection.compute_frustum(&camera), &wave_state)
        );
        let moved_camera = GlobalTransform::from(
            Transform::from_xyz(-60.0, 20.0, 0.0)
                .looking_at(Vec3::new(-60.0, 0.0, 0.0), Vec3::NEG_Z),
        );
        assert!(
            !surface
                .intersects_camera_frustum(&projection.compute_frustum(&moved_camera), &wave_state)
        );
        assert!(
            surface.intersects_camera_frustum(&projection.compute_frustum(&camera), &wave_state)
        );
    }
}
