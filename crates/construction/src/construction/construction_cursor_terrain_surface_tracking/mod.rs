use bevy::camera::primitives::Aabb;
use bevy::picking::mesh_picking::ray_cast::MeshRayCast;
use bevy::picking::mesh_picking::ray_cast::MeshRayCastSettings;
use bevy::picking::mesh_picking::ray_cast::RayCastVisibility;
use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_interface_state::camera::world_pointer_ray_types::WorldPointerRay;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainRenderChunk;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery,
};

use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainRenderRevision;

#[derive(Default)]
pub(crate) struct TerrainCursorHit {
    ray: Option<Ray3d>,
    chunks: usize,
    hit: Option<(Vec3, Vec3)>,
}

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

#[expect(clippy::needless_pass_by_value)]
pub(crate) fn update_construction_cursor(
    ray: Res<WorldPointerRay>,
    ui_pointer_capture: Res<UiPointerCapture>,
    mut terrain_surface_mesh_ray_cast: MeshRayCast,
    terrain_chunks: Query<(), With<TerrainRenderChunk>>,
    terrains: Res<Assets<TerrainAsset>>,
    water_memberships: Query<&TerrainWaterMembershipIndex>,
    water_regions: TerrainWaterRegionQuery,
    mut cursors: Query<&mut ConstructionCursor>,
    tool: Res<ConstructionTool>,
    pickable_chunks: Query<
        (),
        (
            With<TerrainRenderChunk>,
            With<Mesh3d>,
            With<Aabb>,
            With<InheritedVisibility>,
            With<GlobalTransform>,
        ),
    >,
    changed_chunks: Query<
        (),
        (
            With<TerrainRenderChunk>,
            Or<(
                Added<TerrainRenderChunk>,
                Changed<Mesh3d>,
                AssetChanged<Mesh3d>,
                Changed<Aabb>,
                Changed<GlobalTransform>,
                Changed<InheritedVisibility>,
                Changed<TerrainRenderRevision>,
            )>,
        ),
    >,
    mut cached: Local<TerrainCursorHit>,
) {
    if cursors.is_empty() {
        cached.ray = None;
        return;
    }
    let pointer = ray.0.filter(|_| !ui_pointer_capture.over_ui);
    let chunks = pickable_chunks.iter().len();
    if pointer != cached.ray
        || chunks != cached.chunks
        || tool.is_changed()
        || !changed_chunks.is_empty()
    {
        cached.ray = pointer;
        cached.chunks = chunks;
        cached.hit = pointer.and_then(|ray| {
            raycast_rendered_terrain_surface(
                ray,
                &mut terrain_surface_mesh_ray_cast,
                &terrain_chunks,
            )
        });
    }
    let hit = pointer.and_then(|ray| {
        let mut nearest = cached.hit;
        for (owner, region) in water_regions.iter() {
            let Some(terrain) = terrains.get(region.terrain()) else {
                continue;
            };
            let Some(height) = region.surface_height(terrain) else {
                continue;
            };
            // The surface shader consumes source coordinates and displacement
            // weights, so its input mesh is not a world-space picking mesh.
            if ray.direction.y.abs() <= 0.0001 {
                continue;
            }
            let distance = (height - ray.origin.y) / ray.direction.y;
            if distance < 0.0 {
                continue;
            }
            let world = ray.get_point(distance);
            if !water_memberships.iter().any(|index| {
                index.terrain() == region.terrain().id()
                    && index.owner_at_world_xz(world.xz(), terrain) == Some(Some(owner))
            }) {
                continue;
            }
            if nearest.is_none_or(|(point, _)| distance < point.distance(ray.origin)) {
                nearest = Some((world, Vec3::Y));
            }
        }
        nearest
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

fn raycast_rendered_terrain_surface(
    ray: Ray3d,
    mesh_ray_cast: &mut MeshRayCast,
    terrain_chunks: &Query<(), With<TerrainRenderChunk>>,
) -> Option<(Vec3, Vec3)> {
    let filter = |entity| terrain_chunks.contains(entity);
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
