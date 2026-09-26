use super::terrain_water_geometric_wave_shader_state::NativeTerrainWaterGeometricWaveShaderState;
use super::terrain_water_renderer_types::AuthoredTerrainWaterRendererTargets;
use super::terrain_water_renderer_types::AuthoredTerrainWaterSurfaceEffectPass;
use super::terrain_water_renderer_types::TerrainWaterWaveStateKey;
use crate::assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use crate::plugins::camera::camera_runtime_state_types::ZooCamera;
use bevy::camera::primitives::Aabb;
use bevy::camera::primitives::Frustum;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy::transform::helper::TransformHelper;

impl AuthoredTerrainWaterRendererTargets {
    pub(super) fn queue_wave_impact(
        &mut self,
        key: TerrainWaterWaveStateKey,
        position: Vec2,
        strength: f32,
    ) {
        if let Some(state) = self.wave_states.get_mut(&key) {
            state.queue_native_impact_wave(position, strength);
        }
    }
}

impl AuthoredTerrainWaterSurfaceEffectPass {
    fn intersects_camera_frustum(
        &self,
        frustum: &Frustum,
        geometric_wave_state: &NativeTerrainWaterGeometricWaveShaderState,
    ) -> bool {
        let Some(displacement) = geometric_wave_state
            .maximum_horizontal_and_vertical_shader_displacement(
                self.maximum_absolute_input_height,
            )
        else {
            // Do not reject a surface whose supplied wave inputs cannot be bounded.
            return true;
        };
        let minimum = self.horizontal_minimum - Vec2::splat(displacement.x);
        let maximum = self.horizontal_maximum + Vec2::splat(displacement.x);
        frustum.intersects_obb_identity(&Aabb::from_min_max(
            Vec3::new(
                minimum.x,
                self.surface_plane_height - displacement.y,
                minimum.y,
            ),
            Vec3::new(
                maximum.x,
                self.surface_plane_height + displacement.y,
                maximum.y,
            ),
        ))
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update_authored_terrain_water_renderer_inputs(
    time: Res<Time>,
    phase: Res<State<crate::application_lifecycle::GamePhase>>,
    mut targets: Option<ResMut<AuthoredTerrainWaterRendererTargets>>,
    zoo_camera: Query<(Entity, &Camera, Ref<Projection>), With<ZooCamera>>,
    mut camera_transforms: ParamSet<(
        TransformHelper,
        Query<(&mut Camera, &mut Projection, &mut Transform), Without<ZooCamera>>,
    )>,
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    mut water_surfaces: Query<
        (
            &MeshMaterial3d<EffectPassMaterial>,
            &AuthoredTerrainWaterSurfaceEffectPass,
            &TerrainWaterWaveStateKey,
            &mut Visibility,
        ),
        With<AuthoredTerrainWaterSurfaceEffectPass>,
    >,
) {
    let Some(ref mut targets) = targets else {
        return;
    };
    targets.elapsed_seconds += time.delta_secs();
    // One advancing wave field per terrain biome, retained across mesh edits.
    // Hidden sections and both authored passes borrow that same field.
    for wave_state in targets.wave_states.values_mut() {
        wave_state.advance_wave_channels(time.delta_secs());
    }
    let camera = zoo_camera
        .single()
        .ok()
        .filter(|(_, camera, _)| {
            camera.is_active && *phase.get() == crate::application_lifecycle::GamePhase::InGame
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
    let frustum = camera
        .as_ref()
        .map(|(_, projection, transform)| projection.compute_frustum(transform));
    for (_, surface, wave_key, mut visibility) in &mut water_surfaces {
        let wave_state = &targets.wave_states[wave_key];
        let visible = frustum
            .as_ref()
            .is_some_and(|frustum| surface.intersects_camera_frustum(frustum, &wave_state));
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let previous_plane = targets.active_water_plane_height;
    targets.active_water_plane_height = camera.as_ref().and_then(|(_, _, transform)| {
        select_nearest_present_water_plane_height(
            transform.translation(),
            *transform.forward(),
            water_surfaces
                .iter()
                .filter_map(|(_, surface, _, visibility)| {
                    (*visibility != Visibility::Hidden).then_some(surface)
                }),
        )
    });
    let enabled = targets.active_water_plane_height.is_some();
    let mut activation_changed = false;
    for entity in [
        targets.bump_camera,
        targets.reflection_camera,
        targets.refraction_camera,
    ] {
        if let Ok((mut camera, _, _)) = camera_transforms.p1().get_mut(entity) {
            if camera.is_active != enabled {
                camera.is_active = enabled;
                activation_changed = true;
            }
        }
    }
    if activation_changed || phase.is_changed() {
        info!(target: "openzt2_water_targets", enabled,
            surface_pass_count = water_surfaces.iter().count(),
            visible_surface_pass_count = water_surfaces.iter()
                .filter(|(_, _, _, visibility)| **visibility != Visibility::Hidden).count(),
            "water render-target demand");
    }
    let (Some((_, zoo_projection, zoo_global_transform)), Some(active_water_plane_height)) =
        (camera, targets.active_water_plane_height)
    else {
        return;
    };
    let water_view_projection_changed = zoo_projection.is_changed()
        || previous_plane.map(f32::to_bits) != targets.active_water_plane_height.map(f32::to_bits)
        || activation_changed;
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
    if let Ok((_, mut projection, mut transform)) =
        camera_transforms.p1().get_mut(targets.refraction_camera)
    {
        if water_view_projection_changed || projection.is_added() {
            *projection = (*zoo_projection).clone();
        }
        transform.set_if_neq(zoo_transform);
    }
    let reflected_position = Vec3::new(
        zoo_transform.translation.x,
        2.0 * active_water_plane_height - zoo_transform.translation.y,
        zoo_transform.translation.z,
    );
    let reflected_forward = Vec3::new(
        zoo_transform.forward().x,
        -zoo_transform.forward().y,
        zoo_transform.forward().z,
    );
    let reflected_up = Vec3::new(
        zoo_transform.up().x,
        -zoo_transform.up().y,
        zoo_transform.up().z,
    );
    let reflected_transform =
        Transform::from_translation(reflected_position).looking_to(reflected_forward, reflected_up);
    if let Ok((_, mut projection, mut transform)) =
        camera_transforms.p1().get_mut(targets.reflection_camera)
    {
        if water_view_projection_changed || projection.is_added() {
            *projection = (*zoo_projection).clone();
        }
        transform.set_if_neq(reflected_transform);
    }
    let clip_from_view = zoo_projection.get_clip_from_view();
    let source_from_local = Mat4::from_cols(Vec4::X, Vec4::Z, Vec4::Y, Vec4::W);
    let world_to_view = zoo_transform.to_matrix().inverse();
    let source_to_clip = clip_from_view * world_to_view * source_from_local;
    let reflection_world_to_view = reflected_transform.to_matrix().inverse();
    let source_to_reflection_clip = clip_from_view * reflection_world_to_view * source_from_local;
    let clip_to_uv = Mat4::from_cols(
        Vec4::new(0.5, 0.0, 0.0, 0.0),
        Vec4::new(0.0, -0.5, 0.0, 0.0),
        Vec4::Z,
        Vec4::new(0.5, 0.5, 0.0, 1.0),
    );
    let camera_position_source = Vec3::new(
        zoo_transform.translation.x,
        zoo_transform.translation.z,
        zoo_transform.translation.y,
    );
    let camera_across_source = Vec3::new(
        zoo_transform.right().x,
        zoo_transform.right().z,
        zoo_transform.right().y,
    );
    let camera_up_source = Vec3::new(
        zoo_transform.up().x,
        zoo_transform.up().z,
        zoo_transform.up().y,
    );
    for (material_handle, surface, wave_key, visibility) in &water_surfaces {
        let geometric_wave_state = &targets.wave_states[wave_key];
        if *visibility == Visibility::Hidden {
            continue;
        }
        let Some(material) = effect_pass_materials.get_mut_untracked(material_handle) else {
            continue;
        };
        material.bind_float_matrix_effect_semantic("WorldToNDC", source_to_clip);
        material.bind_float_matrix_effect_semantic("WorldToRefract", clip_to_uv * source_to_clip);
        material.bind_float_matrix_effect_semantic(
            "WorldToReflect",
            clip_to_uv * source_to_reflection_clip,
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

fn select_nearest_present_water_plane_height<'a>(
    camera_position: Vec3,
    camera_forward: Vec3,
    water_surfaces: impl Iterator<Item = &'a AuthoredTerrainWaterSurfaceEffectPass>,
) -> Option<f32> {
    water_surfaces
        .map(|surface| {
            let distance_along_view_ray =
                (surface.surface_plane_height - camera_position.y) / camera_forward.y;
            let intersection = camera_position + camera_forward * distance_along_view_ray;
            let ray_hits = distance_along_view_ray.is_finite()
                && distance_along_view_ray >= 0.0
                && intersection.x >= surface.horizontal_minimum.x
                && intersection.x <= surface.horizontal_maximum.x
                && intersection.z >= surface.horizontal_minimum.y
                && intersection.z <= surface.horizontal_maximum.y;
            let horizontal = Vec2::new(camera_position.x, camera_position.z)
                .clamp(surface.horizontal_minimum, surface.horizontal_maximum);
            let distance = if ray_hits {
                distance_along_view_ray * distance_along_view_ray
            } else {
                camera_position.distance_squared(Vec3::new(
                    horizontal.x,
                    surface.surface_plane_height,
                    horizontal.y,
                ))
            };
            // The caller has tested the whole displaced surface against the
            // frustum. A centre-ray miss must not discard water at screen edges.
            (!ray_hits, distance, surface.surface_plane_height)
        })
        .min_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
        })
        .map(|(_, _, surface_plane_height)| surface_plane_height)
}

#[cfg(test)]
mod water_target_demand_tests {
    use super::*;
    use bevy::camera::CameraProjection;

    #[test]
    fn dry_scene_has_no_plane_and_off_axis_water_is_retained() {
        let position = Vec3::new(0.0, 20.0, 0.0);
        assert!(
            select_nearest_present_water_plane_height(position, Vec3::NEG_Y, [].iter()).is_none()
        );
        let surface = AuthoredTerrainWaterSurfaceEffectPass {
            surface_plane_height: 3.0,
            horizontal_minimum: Vec2::splat(10.0),
            horizontal_maximum: Vec2::splat(20.0),
            maximum_absolute_input_height: 3.0,
        };
        assert_eq!(
            select_nearest_present_water_plane_height(
                position,
                Vec3::NEG_Y,
                [&surface].into_iter()
            )
            .map(f32::to_bits),
            Some(3.0_f32.to_bits())
        );
        assert!(
            select_nearest_present_water_plane_height(position, Vec3::NEG_Y, [].iter()).is_none()
        );
    }

    #[test]
    fn water_bounds_leave_and_reenter_the_view_without_a_centre_ray_hit() {
        use openzt2_game_data::terrain::{
            TerrainWaterGeometricWavePresentation, TerrainWaterRippleWavePresentation,
        };
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
            7,
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
        assert!(!surface
            .intersects_camera_frustum(&projection.compute_frustum(&moved_camera), &wave_state));
        assert!(
            surface.intersects_camera_frustum(&projection.compute_frustum(&camera), &wave_state)
        );
    }
}
