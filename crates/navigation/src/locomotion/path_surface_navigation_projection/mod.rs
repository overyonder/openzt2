use avian3d::prelude::{Collider, RigidBody};
use bevy::prelude::*;
use openzt2_game_assets::{
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};

#[derive(Component)]
struct NavigationPathSurfaceCollider;
use openzt2_zoo_state::{
    locomotion::{
        locomotion_types::{ActiveTerrainDerivedNavigationGraph, Destination, Route},
        terrain_derived_navigation_graph_types::NavigationPathSurface,
    },
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
    topology::topology_graph_types::{PathTile, TileSurface},
};

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn project_path_surfaces_into_navigation_graph(
    mut commands: Commands,
    mut graph: Option<ResMut<ActiveTerrainDerivedNavigationGraph>>,
    mut capacity: Option<
        ResMut<openzt2_zoo_state::locomotion::locomotion_types::LocomotionCapacity>,
    >,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    paths: Query<(Entity, &PathTile, &TileSurface, &GlobalTransform)>,
    changes: Query<
        (),
        (
            With<PathTile>,
            Or<(
                Changed<PathTile>,
                Changed<TileSurface>,
                Changed<GlobalTransform>,
            )>,
        ),
    >,
    mut removed_paths: RemovedComponents<PathTile>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut routes: Query<(&mut Route, &mut Destination)>,
    colliders: Query<Entity, With<NavigationPathSurfaceCollider>>,
) {
    let removed = removed_paths.read().count() != 0;
    let Some(graph) = graph.as_mut() else {
        return;
    };
    if !graph.is_changed()
        && !definitions.is_changed()
        && !assets.is_changed()
        && changes.is_empty()
        && !removed
    {
        return;
    }
    let rebuild_colliders = !changes.is_empty()
        || removed
        || definitions.is_changed()
        || assets.is_changed()
        || graph.is_added();
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let mut elevated = Vec::new();
    let mut ground = Vec::new();
    for (entity, path, surface, transform) in &paths {
        let Some(definition) = definitions.find_path(path.definition) else {
            continue;
        };
        let Some(projected) = NavigationPathSurface::new(
            entity,
            *transform,
            f32::from(definition.width_cm) * 0.01,
            surface.height_cm,
        ) else {
            continue;
        };
        if definition.elevated {
            elevated.push(projected);
        } else {
            ground.push(projected);
        }
    }
    let changed = graph
        .terrain_derived_navigation_graph_mut()
        .replace_navigation_path_surfaces(elevated, ground, |point| {
            let (chunk, edited) = terrain_chunks
                .get(terrain_chunk_at(&terrain_index, point)?)
                .ok()?;
            let terrain = terrain_assets.get(&chunk.asset)?;
            Some(sample_terrain(chunk, terrain, edited, point)?.height_m)
        });
    if !changed {
        return;
    }
    if rebuild_colliders {
        for collider in &colliders {
            commands.entity(collider).despawn();
        }
        for (entity, path, surface, transform) in &paths {
            let Some(definition) = definitions
                .find_path(path.definition)
                .filter(|definition| definition.elevated)
            else {
                continue;
            };
            if NavigationPathSurface::new(
                entity,
                *transform,
                f32::from(definition.width_cm) * 0.01,
                surface.height_cm,
            )
            .is_none()
            {
                continue;
            }
            let half = f32::from(definition.width_cm) * 0.005;
            let mut vertices = [(-half, -half), (half, -half), (half, half), (-half, half)]
                .into_iter()
                .enumerate()
                .map(|(index, (x, z))| Vec3::new(x, f32::from(surface.height_cm[index]) * 0.01, z))
                .collect::<Vec<_>>();
            vertices.push(Vec3::Y * (f32::from(surface.height_cm[4]) * 0.01));
            commands.entity(entity).with_child((
                NavigationPathSurfaceCollider,
                RigidBody::Static,
                Collider::trimesh(vertices, vec![[0, 4, 1], [1, 4, 2], [2, 4, 3], [3, 4, 0]]),
                Transform::IDENTITY,
            ));
        }
    }
    let point_capacity = graph
        .terrain_derived_navigation_graph()
        .navigation_node_count()
        .saturating_add(1);
    if let Some(capacity) = capacity.as_mut() {
        capacity.max_route_points = point_capacity;
    }
    for (mut route, mut destination) in &mut routes {
        route.clear();
        if route.points.capacity() < point_capacity {
            route.points.reserve_exact(point_capacity);
        }
        destination.set_changed();
    }
}
