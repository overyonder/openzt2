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
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::terrain::terrain_navigation_change_types::{
    TerrainNavigationChanged, TerrainNavigationRestored,
};

#[derive(Component)]
pub(super) struct NavigationPathSurfaceCollider;
use openzt2_zoo_state::{
    locomotion::{
        locomotion_types::ActiveTerrainDerivedNavigationGraph,
        terrain_derived_navigation_graph_types::{NavigationPathGeometry, NavigationPathSurface},
    },
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
    topology::topology_graph_types::{PathTile, TileSurface},
};

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn project_path_surfaces_into_navigation_graph(
    mut roots: Query<
        (
            Entity,
            &mut ActiveTerrainDerivedNavigationGraph,
            &mut openzt2_zoo_state::locomotion::locomotion_types::LocomotionCapacity,
            &TerrainIndex,
        ),
        With<WorldRoot>,
    >,
    paths: Query<(Entity, &NavigationPathGeometry, &WorldMember)>,
    changed_paths: Query<
        Entity,
        (
            With<NavigationPathGeometry>,
            Or<(Changed<NavigationPathGeometry>, Changed<WorldMember>)>,
        ),
    >,
    has_path_geometry: Query<(), With<NavigationPathGeometry>>,
    mut removed_paths: RemovedComponents<NavigationPathGeometry>,
    mut removed_memberships: RemovedComponents<WorldMember>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>, &WorldMember)>,
    mut path_changes: MessageWriter<NavigationPathGraphChanged>,
    mut installations: MessageReader<NavigationGraphReplaced>,
    mut terrain_changes: MessageReader<TerrainNavigationChanged>,
    mut restored_terrain: MessageReader<TerrainNavigationRestored>,
    phase: Res<State<GamePhase>>,
) {
    let graph_replaced = installations.read().last().is_some();
    let terrain_edits = terrain_changes
        .read()
        .map(|edit| (edit.chunk, edit.min, edit.max))
        .chain(
            restored_terrain
                .read()
                .map(|edit| (edit.chunk, edit.min, edit.max)),
        );
    let Ok((root, mut graph, mut capacity, terrain_index)) = roots.single_mut() else {
        for _ in terrain_edits {}
        removed_paths.clear();
        removed_memberships.clear();
        return;
    };
    let mut changed: std::collections::BTreeMap<_, _> = removed_paths
        .read()
        .chain(
            removed_memberships
                .read()
                .filter(|&entity| has_path_geometry.contains(entity)),
        )
        .map(|entity| (entity, None))
        .collect();
    if graph_replaced {
        changed.clear();
        changed.extend(
            paths
                .iter()
                .filter(|(_, _, member)| member.root == root)
                .map(|(entity, geometry, _)| (entity, Some(geometry))),
        );
    } else {
        changed.extend(changed_paths.iter().map(|entity| {
            let geometry = paths
                .get(entity)
                .ok()
                .filter(|(_, _, member)| member.root == root)
                .map(|(_, geometry, _)| geometry);
            (entity, geometry)
        }));
    }
    let navigation = graph
        .bypass_change_detection()
        .terrain_derived_navigation_graph_mut();
    let mut terrain_cells = std::collections::BTreeSet::new();
    for (chunk, min, max) in terrain_edits {
        let Ok((chunk, _, member)) = terrain_chunks.get(chunk) else {
            continue;
        };
        if member.root != root {
            continue;
        }
        let bounds =
            super::terrain_edit_navigation_refresh::navigation_sample_edit_bounds(chunk, min, max);
        let size = f32::from(navigation.terrain_cell_size_centimetres) * 0.01;
        if size <= 0.0 {
            continue;
        }
        let first = (Vec2::new(bounds.min.x, -bounds.max.y) / size)
            .floor()
            .as_uvec2();
        let last = (Vec2::new(bounds.max.x, -bounds.min.y) / size)
            .floor()
            .as_uvec2();
        for z in first.y..=last.y {
            for x in first.x..=last.x {
                terrain_cells.insert([x, z]);
            }
        }
    }
    if changed.is_empty() && terrain_cells.is_empty() {
        return;
    }
    let changes = navigation.update_navigation_path_surfaces(changed, terrain_cells, |point| {
        let (chunk, edited, _) = terrain_chunks
            .get(terrain_chunk_at(terrain_index, point)?)
            .ok()?;
        let terrain = terrain_assets.get(&chunk.asset)?;
        Some(sample_terrain(chunk, terrain, edited, point)?.height_m)
    });
    if changes.ground_nodes.is_empty() && changes.elevated_nodes.is_empty() {
        return;
    }
    graph.set_changed();
    let point_capacity = graph
        .terrain_derived_navigation_graph()
        .navigation_node_count()
        .saturating_add(1);
    if capacity.max_route_points != point_capacity {
        capacity.max_route_points = point_capacity;
    }
    if *phase.get() == GamePhase::InGame {
        path_changes.write(NavigationPathGraphChanged { root, changes });
    }
}

#[derive(Message)]
pub(super) struct NavigationPathGraphChanged {
    pub(super) root: Entity,
    pub(super) changes: openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::NavigationPathSurfaceChanges,
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
