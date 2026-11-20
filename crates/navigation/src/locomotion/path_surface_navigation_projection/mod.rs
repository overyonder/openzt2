use super::world_navigation_installation::NavigationGraphReplaced;
use avian3d::prelude::{Collider, RigidBody};
use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::{
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_zoo_state::terrain::terrain_navigation_change_types::{
    TerrainNavigationChanged, TerrainNavigationRestored,
};

#[derive(Component)]
pub(super) struct NavigationPathSurfaceCollider;
use openzt2_zoo_state::{
    locomotion::{
        locomotion_types::ActiveTerrainDerivedNavigationGraph,
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
    (mut removed_paths, mut removed_surfaces): (
        RemovedComponents<PathTile>,
        RemovedComponents<TileSurface>,
    ),
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut path_changes: MessageWriter<NavigationPathGraphChanged>,
    mut installations: MessageReader<NavigationGraphReplaced>,
    mut terrain_changes: MessageReader<TerrainNavigationChanged>,
    mut restored_terrain: MessageReader<TerrainNavigationRestored>,
    phase: Res<State<GamePhase>>,
) {
    let removed = removed_paths.read().chain(removed_surfaces.read()).count() != 0;
    let graph_replaced = installations.read().count() != 0;
    let terrain_changed =
        (terrain_changes.read().count() != 0) | (restored_terrain.read().count() != 0);
    let Some(graph) = graph.as_mut() else {
        return;
    };
    if !graph_replaced
        && !terrain_changed
        && !definitions.is_changed()
        && !assets.is_changed()
        && changes.is_empty()
        && !removed
    {
        return;
    }
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
    let changes = graph
        .bypass_change_detection()
        .terrain_derived_navigation_graph_mut()
        .replace_navigation_path_surfaces(elevated, ground, |point| {
            let (chunk, edited) = terrain_chunks
                .get(terrain_chunk_at(&terrain_index, point)?)
                .ok()?;
            let terrain = terrain_assets.get(&chunk.asset)?;
            Some(sample_terrain(chunk, terrain, edited, point)?.height_m)
        });
    if !changes.ground && !changes.elevated {
        return;
    }
    graph.set_changed();
    let point_capacity = graph
        .terrain_derived_navigation_graph()
        .navigation_node_count()
        .saturating_add(1);
    if let Some(capacity) = capacity.as_mut() {
        capacity.max_route_points = point_capacity;
    }
    if *phase.get() == GamePhase::InGame {
        path_changes.write(NavigationPathGraphChanged {
            elevated: changes.elevated,
        });
    }
}

#[derive(Message)]
pub(super) struct NavigationPathGraphChanged {
    pub(super) elevated: bool,
}

#[allow(clippy::type_complexity)]
pub(super) fn project_elevated_path_colliders(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    paths: Query<(
        Entity,
        Ref<PathTile>,
        Ref<TileSurface>,
        Ref<GlobalTransform>,
        Option<&Children>,
    )>,
    mut colliders: Query<&mut Collider, With<NavigationPathSurfaceCollider>>,
    detached: Query<
        Entity,
        (
            With<Children>,
            Or<(Without<PathTile>, Without<TileSurface>)>,
        ),
    >,
    mut removed_paths: RemovedComponents<PathTile>,
    mut removed_surfaces: RemovedComponents<TileSurface>,
    children: Query<&Children>,
) {
    let refresh_all = definitions.is_changed() || assets.is_changed();
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    for entity in removed_paths.read().chain(removed_surfaces.read()) {
        if !detached.contains(entity) {
            continue;
        }
        if let Ok(children) = children.get(entity) {
            for child in children.iter() {
                if colliders.contains(child) {
                    commands.entity(child).try_despawn();
                }
            }
        }
    }
    for (entity, path, surface, transform, children) in &paths {
        if !refresh_all && !path.is_changed() && !surface.is_changed() && !transform.is_changed() {
            continue;
        }
        let collider = children
            .into_iter()
            .flat_map(|children| children.iter())
            .find(|&child| colliders.contains(child));
        let definition = definitions.find_path(path.definition).filter(|definition| {
            definition.elevated
                && NavigationPathSurface::new(
                    entity,
                    *transform,
                    f32::from(definition.width_cm) * 0.01,
                    surface.height_cm,
                )
                .is_some()
        });
        let Some(definition) = definition else {
            if let Some(collider) = collider {
                commands.entity(collider).try_despawn();
            }
            continue;
        };
        if !refresh_all && !path.is_changed() && !surface.is_changed() && collider.is_some() {
            // Child transforms already track movement of their path.
            continue;
        }
        let half = f32::from(definition.width_cm) * 0.005;
        let mut vertices = [(-half, -half), (half, -half), (half, half), (-half, half)]
            .into_iter()
            .enumerate()
            .map(|(index, (x, z))| Vec3::new(x, f32::from(surface.height_cm[index]) * 0.01, z))
            .collect::<Vec<_>>();
        vertices.push(Vec3::Y * (f32::from(surface.height_cm[4]) * 0.01));
        let next = Collider::trimesh(vertices, vec![[0, 4, 1], [1, 4, 2], [2, 4, 3], [3, 4, 0]]);
        if let Some(collider) = collider {
            if let Ok(mut current) = colliders.get_mut(collider) {
                *current = next;
            }
        } else {
            commands.entity(entity).with_child((
                NavigationPathSurfaceCollider,
                RigidBody::Static,
                next,
                Transform::IDENTITY,
            ));
        }
    }
}
