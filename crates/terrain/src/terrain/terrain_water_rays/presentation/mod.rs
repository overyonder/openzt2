use super::{
    build_water_ray_disk_mesh, build_water_ray_strip_mesh,
    systems::{TerrainWaterRayEffect, TerrainWaterRayPass},
    view_occlusion::WaterRayViewOcclusion,
};
use bevy::{
    mesh::VertexAttributeValues, prelude::*, render::storage::ShaderBuffer,
    transform::helper::TransformHelper,
};
use openzt2_game_assets::{
    material::runtime::effect_pass_gpu_data::EffectPassMaterial,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
};
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_zoo_state::{
    environment::environment_state_types::EnvironmentWaterRayLight,
    terrain::{
        terrain_water_presentation_types::TerrainWaterRegionQuery,
        terrain_water_ray_types::TerrainWaterRayAnimation,
        terrain_water_view_state::TerrainWaterViewState,
    },
};

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(in crate::terrain) fn update_terrain_water_ray_presentation(
    mut commands: Commands,
    time: Res<Time>,
    view: Res<TerrainWaterViewState>,
    lights: Query<&EnvironmentWaterRayLight>,
    cameras: Query<(Entity, &Camera, &Projection, Option<&WaterRayViewOcclusion>), With<ZooCamera>>,
    sources: TerrainWaterRegionQuery,
    assets: Res<Assets<TerrainAsset>>,
    mut effects: Query<(
        Entity,
        &ChildOf,
        &TerrainWaterRayEffect,
        &mut TerrainWaterRayAnimation,
        &mut Visibility,
    )>,
    mut spatial: ParamSet<(
        TransformHelper,
        Query<(
            &ChildOf,
            &TerrainWaterRayPass,
            &Mesh3d,
            &MeshMaterial3d<EffectPassMaterial>,
            &mut Transform,
        )>,
    )>,
    mut materials: ResMut<Assets<EffectPassMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (_, _, _, _, mut visibility) in &mut effects {
        *visibility = Visibility::Hidden;
    }
    for (entity, _, _, _) in &cameras {
        commands.entity(entity).remove::<WaterRayViewOcclusion>();
    }
    if effects.is_empty() {
        return;
    }
    let (Ok(light), Ok((camera_entity, camera, projection, query))) =
        (lights.single(), cameras.single())
    else {
        return;
    };
    let Some(sun) = light.sun_direction() else {
        return;
    };
    let Ok(camera_transform) = spatial.p0().compute_global_transform(camera_entity) else {
        return;
    };
    if !camera.is_active {
        return;
    }
    let Some(position) = view.camera_position() else {
        return;
    };
    let Projection::Perspective(perspective) = projection else {
        return;
    };
    let Some(viewport) = camera.physical_viewport_size() else {
        return;
    };
    let world_to_clip =
        projection.get_clip_from_view() * Mat4::from(camera_transform.affine().inverse());
    let centre = position + sun * (perspective.far * 0.95);
    let side = perspective.far * 0.95 * 0.05;
    let right = *camera_transform.right();
    let up = *camera_transform.up();
    let corners = billboard_clip_corners(world_to_clip, centre, right, up, side);
    let offset = centre - position;
    let camera_offset = Vec3::new(
        right.dot(offset),
        up.dot(offset),
        camera_transform.forward().dot(offset),
    );
    let vertical_tangent = (perspective.fov * 0.5).tan();
    let half_fov_tangents = Vec2::new(
        vertical_tangent * perspective.aspect_ratio,
        vertical_tangent,
    );
    let (total, clipped) = projected_pixel_area(
        camera_offset,
        Vec2::splat(side),
        half_fov_tangents,
        viewport,
    );
    let (large_total, large_clipped) = projected_pixel_area(
        camera_offset,
        Vec2::splat(side * 6.0),
        half_fov_tangents,
        viewport,
    );
    if let Some(query) = query {
        // The renderer consumes an extracted copy; the mailbox retains only
        // the last completed GPU result across main/render worlds.
        let visible = query.visible_fraction().unwrap_or(0.0);
        apply_ray_contributions(
            light,
            sun,
            position,
            right,
            visible,
            if large_total > 0.0 {
                large_clipped / large_total
            } else {
                0.0
            },
            &view,
            &sources,
            &assets,
            &mut effects,
            &mut spatial.p1(),
            &mut materials,
            &mut buffers,
            &mut meshes,
            time.delta_secs(),
        );
    }
    let mut next = query.cloned().unwrap_or_default();
    next.clip_corners = corners;
    next.full_sample_count = total;
    next.projected_fraction = if total > 0.0 { clipped / total } else { 0.0 };
    commands.entity(camera_entity).insert(next);
}

fn billboard_clip_corners(
    matrix: Mat4,
    centre: Vec3,
    right: Vec3,
    up: Vec3,
    side: f32,
) -> [Vec4; 4] {
    [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
        .map(|(x, y)| matrix * (centre + (right * x + up * y) * (side * 0.5)).extend(1.0))
}

fn projected_pixel_area(
    camera_offset: Vec3,
    rectangle: Vec2,
    half_fov_tangents: Vec2,
    size: UVec2,
) -> (f32, f32) {
    // Area normalization uses radial distance, independently of the GPU
    // quad's perspective projection. Quantize each dimension before its product.
    let half_extents = half_fov_tangents * camera_offset.length();
    if half_extents.min_element() <= 0.0 {
        return (0.0, 0.0);
    }
    let pixels = |span: Vec2| {
        let dimensions = span / (half_extents * 2.0) * size.as_vec2();
        dimensions.x.trunc() * dimensions.y.trunc()
    };
    let total = pixels(rectangle);
    if camera_offset.z <= 0.0 {
        return (total, 0.0);
    }
    let centre = camera_offset.truncate();
    let min = (centre - rectangle * 0.5).clamp(-half_extents, half_extents);
    let max = (centre + rectangle * 0.5).clamp(-half_extents, half_extents);
    (total, pixels(max - min))
}

#[cfg(test)]
mod tests {
    use super::projected_pixel_area;
    use bevy::prelude::*;

    #[test]
    fn radial_pixel_areas_match_hand_derived_rectangles() {
        // A 90-degree square view at radial distance 10 spans 20 world units.
        // Off-axis (6, 0, 8) keeps that span despite its forward depth of 8.
        for (offset, rectangle, expected) in [
            (
                Vec3::new(0.0, 0.0, 10.0),
                Vec2::splat(2.0),
                (10_000.0, 10_000.0),
            ),
            (
                Vec3::new(6.0, 0.0, 8.0),
                Vec2::splat(2.0),
                (10_000.0, 10_000.0),
            ),
            // Horizontal [5, 11] clips to [5, 10]: 250 rather than 300 pixels.
            (
                Vec3::new(8.0, 0.0, 6.0),
                Vec2::new(6.0, 2.0),
                (30_000.0, 25_000.0),
            ),
            // 102.5 pixels per dimension truncates to 102, not area 10,506.
            (
                Vec3::new(0.0, 0.0, 10.0),
                Vec2::splat(2.05),
                (10_404.0, 10_404.0),
            ),
        ] {
            assert_eq!(
                projected_pixel_area(offset, rectangle, Vec2::ONE, UVec2::splat(1000)),
                expected,
            );
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn apply_ray_contributions(
    light: &EnvironmentWaterRayLight,
    sun: Vec3,
    camera: Vec3,
    camera_right: Vec3,
    visible: f32,
    projected: f32,
    view: &TerrainWaterViewState,
    sources: &TerrainWaterRegionQuery,
    assets: &Assets<TerrainAsset>,
    effects: &mut Query<(
        Entity,
        &ChildOf,
        &TerrainWaterRayEffect,
        &mut TerrainWaterRayAnimation,
        &mut Visibility,
    )>,
    passes: &mut Query<(
        &ChildOf,
        &TerrainWaterRayPass,
        &Mesh3d,
        &MeshMaterial3d<EffectPassMaterial>,
        &mut Transform,
    )>,
    materials: &mut Assets<EffectPassMaterial>,
    buffers: &mut Assets<ShaderBuffer>,
    meshes: &mut Assets<Mesh>,
    delta: f32,
) {
    let Some(horizontal) = light.object_light_horizontal_vector() else {
        return;
    };
    if sun.y == 0.0 || view.underwater_status() != Some(true) {
        return;
    }
    let Some(owner) = view.camera_region() else {
        return;
    };
    let Some(height) = sources.get(owner).and_then(|source| {
        assets
            .get(source.terrain())
            .and_then(|asset| source.surface_height(asset))
    }) else {
        return;
    };
    for (entity, parent, effect, mut animation, mut visibility) in effects.iter_mut() {
        animation.advance_intensity(visible, delta);
        if parent.parent() != owner {
            continue;
        }
        let lower_cos = f64::from_bits(0x3ff7bc89e0000000).cos() as f32;
        let strip_angle = ((sun.y - lower_cos)
            / (f64::from_bits(0x3ff0c15240000000).cos() as f32 - lower_cos))
            .clamp(0.0, 1.0);
        let brightness = ((horizontal.length() - 0.2) / 0.6).clamp(0.0, 1.0);
        let strip = strip_angle
            * brightness
            * 0.3
            * (1.0 + (animation.intensity() - 1.0) * (projected * 2.0).clamp(0.0, 1.0));
        let disk = ((sun.y - lower_cos)
            / (f64::from_bits(0x3ff38c3540000000).cos() as f32 - lower_cos))
            .clamp(0.0, 1.0)
            * animation.intensity();
        let hit = camera + sun * ((height - camera.y) / sun.y);
        let strip_matrix = Mat4::from_cols(
            Vec4::X,
            Vec4::Z,
            Vec4::new(sun.x / sun.y, 1.0, sun.z / sun.y, 0.0),
            Vec4::W,
        );
        let disk_matrix = Mat4::from_cols(
            camera_right.extend(0.0),
            Vec4::new(-camera_right.z, 0.0, camera_right.x, 0.0),
            Vec4::Y,
            Vec4::W,
        );
        let mut valid = true;
        for (pass_parent, pass, mesh, material, mut transform) in passes.iter_mut() {
            if pass_parent.parent() != entity {
                continue;
            }
            let contribution = if pass.disk {
                disk
            } else if effect.secondary_strip {
                strip * if pass.texture_layer == 0 { 0.33 } else { 0.68 }
            } else {
                strip
            };
            let Some(mut material) = materials.get_mut(&material.0) else {
                valid = false;
                continue;
            };
            let channel = ((f64::from(contribution) * 255.0 + 0.5) as u32).min(255);
            valid &= material.set_fixed_function_ambient_colour_argb(channel * 0x01010101);
            material.write_changed_fixed_function_registers_to_persistent_buffer(buffers);
            let source_mesh = if pass.disk {
                build_water_ray_disk_mesh()
            } else {
                build_water_ray_strip_mesh()
            };
            if let Some(mut mesh) = meshes.get_mut(&mesh.0) {
                *mesh = transform_ray_mesh(
                    source_mesh,
                    if pass.disk { disk_matrix } else { strip_matrix },
                );
            }
            transform.translation = if pass.disk { hit } else { camera };
        }
        if valid && (strip > 0.0 || disk > 0.0) {
            *visibility = Visibility::Inherited;
        }
    }
}

fn transform_ray_mesh(mut mesh: Mesh, matrix: Mat4) -> Mesh {
    if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for position in positions {
            *position = matrix
                .transform_point3(Vec3::from_array(*position))
                .to_array();
        }
    }
    let normals_matrix = matrix.inverse().transpose();
    if let Some(VertexAttributeValues::Float32x3(normals)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        for normal in normals {
            *normal = normals_matrix
                .transform_vector3(Vec3::from_array(*normal))
                .normalize_or_zero()
                .to_array();
        }
    }
    mesh
}
