use bevy::camera::primitives::Aabb;
use bevy::ecs::system::SystemParam;
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
    TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionQuery,
    TerrainWaterRegionRetired, TerrainWaterRegionSource,
};

use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;

#[derive(Component, Default)]
pub(crate) struct ConstructionSurfaceRay(Option<Ray3d>);

#[derive(SystemParam)]
pub(crate) struct RemovedConstructionSurfaces<'w, 's> {
    terrain: RemovedComponents<'w, 's, TerrainRenderChunk>,
    terrain_meshes: RemovedComponents<'w, 's, Mesh3d>,
    water_membership: RemovedComponents<'w, 's, TerrainWaterMembershipIndex>,
    water_sources: RemovedComponents<'w, 's, TerrainWaterRegionSource>,
    water_edits: RemovedComponents<'w, 's, TerrainWaterRegionEdit>,
    water_retired: RemovedComponents<'w, 's, TerrainWaterRegionRetired>,
}

impl RemovedConstructionSurfaces<'_, '_> {
    fn changed(&mut self, terrain_chunks: &Query<(), With<TerrainRenderChunk>>) -> bool {
        let terrain = self.terrain.read().count() != 0;
        let meshes = self
            .terrain_meshes
            .read()
            .any(|entity| terrain_chunks.contains(entity));
        let membership = self.water_membership.read().count() != 0;
        let sources = self.water_sources.read().count() != 0;
        let edits = self.water_edits.read().count() != 0;
        let retired = self.water_retired.read().count() != 0;
        terrain || meshes || membership || sources || edits || retired
    }
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
        ConstructionSurfaceRay::default(),
        super::construction_delete_hover_target_resolution::DeleteHoverProbe::default(),
        WorldMember { root },
    ));
}

pub(crate) fn update_construction_cursor(
    ray: Res<WorldPointerRay>,
    ui_pointer_capture: Res<UiPointerCapture>,
    mut terrain_surface_mesh_ray_cast: MeshRayCast,
    terrain_chunks: Query<(), With<TerrainRenderChunk>>,
    terrains: Res<Assets<TerrainAsset>>,
    water_memberships: Query<&TerrainWaterMembershipIndex>,
    water_regions: TerrainWaterRegionQuery,
    mut cursors: Query<(&mut ConstructionCursor, &mut ConstructionSurfaceRay)>,
    tool: Res<ConstructionTool>,
    changed_chunks: Query<
        (),
        (
            With<TerrainRenderChunk>,
            Or<(
                Changed<TerrainRenderChunk>,
                Changed<Mesh3d>,
                AssetChanged<Mesh3d>,
                Changed<Aabb>,
                Changed<GlobalTransform>,
                Changed<InheritedVisibility>,
            )>,
        ),
    >,
    changed_water: Query<
        (),
        Or<(
            Changed<TerrainWaterMembershipIndex>,
            Changed<TerrainWaterRegionSource>,
            Changed<TerrainWaterRegionEdit>,
            Added<TerrainWaterRegionRetired>,
        )>,
    >,
    mut removed: RemovedConstructionSurfaces,
) {
    let removed = removed.changed(&terrain_chunks);
    let Ok((mut cursor, mut previous_ray)) = cursors.single_mut() else {
        return;
    };
    let pointer = ray.0.filter(|_| !ui_pointer_capture.over_ui);
    if pointer == previous_ray.0
        && !tool.is_changed()
        && !removed
        && !terrains.is_changed()
        && changed_chunks.is_empty()
        && changed_water.is_empty()
    {
        return;
    }
    previous_ray.0 = pointer;
    let hit = pointer.and_then(|ray| {
        let mut nearest = raycast_rendered_terrain_surface(
            ray,
            &mut terrain_surface_mesh_ray_cast,
            &terrain_chunks,
        );
        if ray.direction.y.abs() <= 0.0001 {
            return nearest;
        }
        let mut indexes = bevy::platform::collections::HashMap::<_, Vec<_>>::new();
        for index in &water_memberships {
            indexes.entry(index.terrain()).or_default().push(index);
        }
        for (owner, region) in water_regions.iter() {
            let Some(terrain) = terrains.get(region.terrain()) else {
                continue;
            };
            let Some(height) = region.surface_height(terrain) else {
                continue;
            };
            // The surface shader consumes source coordinates and displacement
            // weights, so its input mesh is not a world-space picking mesh.
            let distance = (height - ray.origin.y) / ray.direction.y;
            if distance < 0.0 {
                continue;
            }
            let world = ray.get_point(distance);
            if !indexes.get(&region.terrain().id()).is_some_and(|matches| {
                matches
                    .iter()
                    .any(|index| index.owner_at_world_xz(world.xz(), terrain) == Some(Some(owner)))
            }) {
                continue;
            }
            if nearest.is_none_or(|(point, _)| distance < point.distance(ray.origin)) {
                nearest = Some((world, Vec3::Y));
            }
        }
        nearest
    });
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
