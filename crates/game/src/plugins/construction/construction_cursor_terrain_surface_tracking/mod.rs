use bevy::{
    picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings, RayCastVisibility},
    prelude::*,
};

use crate::plugins::{
    camera::world_pointer_ray_types::WorldPointerRay,
    terrain::{
        terrain_chunk_presentation_types::TerrainRenderChunk,
        terrain_water_presentation_types::{TerrainWaterSurface, TerrainWaterfallSurface},
    },
    ui::picking::UiPointerCapture,
    world_spawn::{world_membership_types::WorldMember, world_membership_types::WorldRoot},
};

use super::construction_interaction_types::ConstructionCursor;

pub(super) fn ensure_construction_cursor(
    mut commands: Commands,
    roots: Query<Entity, With<WorldRoot>>,
    cursors: Query<(), With<ConstructionCursor>>,
) {
    if !cursors.is_empty() {
        return;
    }
    let Ok(root) = roots.single() else { return };
    commands.spawn((
        ConstructionCursor {
            world: Vec3::ZERO,
            normal: Vec3::Y,
            over_terrain: false,
        },
        WorldMember { root },
    ));
}

pub(crate) fn update_construction_cursor(
    ray: Res<WorldPointerRay>,
    ui_pointer_capture: Res<UiPointerCapture>,
    mut terrain_surface_mesh_ray_cast: MeshRayCast,
    terrain_chunks: Query<(), With<TerrainRenderChunk>>,
    horizontal_water_surfaces: Query<
        (),
        (With<TerrainWaterSurface>, Without<TerrainWaterfallSurface>),
    >,
    mut cursors: Query<&mut ConstructionCursor>,
) {
    if cursors.is_empty() {
        return;
    }
    let hit = (!ui_pointer_capture.over_ui)
        .then_some(ray.0)
        .flatten()
        .and_then(|ray| {
            raycast_rendered_terrain_and_horizontal_water_surfaces(
                ray,
                &mut terrain_surface_mesh_ray_cast,
                &terrain_chunks,
                &horizontal_water_surfaces,
            )
        });
    for mut cursor in &mut cursors {
        let over_terrain = hit.is_some();
        if cursor.over_terrain != over_terrain {
            cursor.over_terrain = over_terrain;
        }
        if let Some((world, normal)) = hit {
            if cursor.world != world {
                cursor.world = world;
            }
            if cursor.normal != normal {
                cursor.normal = normal;
            }
        }
    }
}

fn raycast_rendered_terrain_and_horizontal_water_surfaces(
    ray: Ray3d,
    mesh_ray_cast: &mut MeshRayCast,
    terrain_chunks: &Query<(), With<TerrainRenderChunk>>,
    horizontal_water_surfaces: &Query<
        (),
        (With<TerrainWaterSurface>, Without<TerrainWaterfallSurface>),
    >,
) -> Option<(Vec3, Vec3)> {
    let filter =
        |entity| terrain_chunks.contains(entity) || horizontal_water_surfaces.contains(entity);
    mesh_ray_cast
        .cast_ray(
            ray,
            &MeshRayCastSettings::default()
                .with_filter(&filter)
                .with_visibility(RayCastVisibility::Visible),
        )
        .first()
        .map(|(_, hit)| (hit.point, hit.normal.normalize_or(Vec3::Y)))
}
