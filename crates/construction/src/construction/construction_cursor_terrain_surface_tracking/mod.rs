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
    TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionQuery,
    TerrainWaterRegionRetired, TerrainWaterRegionSource,
};

use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;

#[derive(Component, Default, PartialEq)]
pub(super) struct EffectiveConstructionRay(pub(super) Option<Ray3d>);

#[derive(Message)]
pub(super) struct ConstructionSurfaceDiscarded;

pub(super) fn notify_discarded_construction_terrain(
    discarded: On<Discard<(TerrainRenderChunk, Mesh3d)>>,
    terrain: Query<(), With<TerrainRenderChunk>>,
    mut changed: MessageWriter<ConstructionSurfaceDiscarded>,
) {
    if terrain.contains(discarded.entity) {
        changed.write(ConstructionSurfaceDiscarded);
    }
}

pub(super) fn notify_discarded_construction_water(
    _: On<
        Discard<(
            TerrainWaterMembershipIndex,
            TerrainWaterRegionSource,
            TerrainWaterRegionEdit,
            TerrainWaterRegionRetired,
        )>,
    >,
    mut changed: MessageWriter<ConstructionSurfaceDiscarded>,
) {
    changed.write(ConstructionSurfaceDiscarded);
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
        EffectiveConstructionRay::default(),
        WorldMember { root },
    ));
}

pub(super) fn project_effective_construction_ray(
    ray: Res<WorldPointerRay>,
    ui_capture: Res<UiPointerCapture>,
    mut cursor: Single<&mut EffectiveConstructionRay>,
) {
    cursor.set_if_neq(EffectiveConstructionRay(
        ray.0.filter(|_| !ui_capture.over_ui),
    ));
}

pub(super) fn update_construction_cursor(
    mut terrain_surface_mesh_ray_cast: MeshRayCast,
    terrain_chunks: Query<(), With<TerrainRenderChunk>>,
    terrains: Res<Assets<TerrainAsset>>,
    water_memberships: Query<&TerrainWaterMembershipIndex>,
    water_regions: TerrainWaterRegionQuery,
    mut cursors: Query<(&mut ConstructionCursor, Ref<EffectiveConstructionRay>)>,
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
    mut discarded: MessageReader<ConstructionSurfaceDiscarded>,
) {
    let removed = !discarded.is_empty();
    discarded.clear();
    let Ok((mut cursor, pointer)) = cursors.single_mut() else {
        return;
    };
    if !pointer.is_changed()
        && !tool.is_changed()
        && !removed
        && !terrains.is_changed()
        && changed_chunks.is_empty()
        && changed_water.is_empty()
    {
        return;
    }
    let hit = pointer.0.and_then(|ray| {
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
    let next = hit.map_or(
        ConstructionCursor {
            over_terrain: false,
            ..*cursor
        },
        |(world, normal)| ConstructionCursor {
            world,
            normal,
            over_terrain: true,
        },
    );
    cursor.set_if_neq(next);
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
