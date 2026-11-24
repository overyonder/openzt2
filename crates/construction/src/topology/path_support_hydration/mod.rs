use super::path_presentation_neighborhood::{cells_for_region, maximum_path_step, terrain_region};
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_zoo_state::topology::topology_graph_types::PathSupport;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;

/// Marks a path after its support has been resolved.
#[derive(Component)]
pub(super) struct PathSupportHydrated;

/// Prefab selected for a path support.
#[derive(Component)]
pub(super) struct PathSupportPrefab(pub(super) AssetId);

/// A base that authors `fitLowest` reaches the lowest ground within its
/// `fitRadius`; with no radius every sample is the centre.
// Centre plus eight ring samples, not every vertex in the radius.
const FIT_LOWEST_SAMPLE_DIRECTIONS: [Vec2; 9] = [
    Vec2::ZERO,
    Vec2::X,
    Vec2::NEG_X,
    Vec2::Y,
    Vec2::NEG_Y,
    Vec2::new(
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
    ),
    Vec2::new(
        std::f32::consts::FRAC_1_SQRT_2,
        -std::f32::consts::FRAC_1_SQRT_2,
    ),
    Vec2::new(
        -std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
    ),
    Vec2::new(
        -std::f32::consts::FRAC_1_SQRT_2,
        -std::f32::consts::FRAC_1_SQRT_2,
    ),
];

/// Adds a support beneath each elevated tile, anchored at the terrain height.
pub(super) fn hydrate_authored_path_supports_at_sampled_terrain_height(
    mut commands: Commands,
    paths: Query<
        (
            Entity,
            &PathTile,
            &openzt2_zoo_state::topology::topology_graph_types::TileSurface,
            &WorldMember,
        ),
        Without<PathSupportHydrated>,
    >,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    terrain_index: Single<&TerrainIndex>,
    topology_index: Single<&TopologyIndex>,
    topology_grid: Single<&TopologyGrid>,
) {
    let terrain_index = terrain_index.into_inner();
    let topology_index = topology_index.into_inner();
    let topology_grid = topology_grid.into_inner();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (path_entity, path, surface, world_member) in &paths {
        let Some(definition) = definitions.find_path(path.definition) else {
            continue;
        };
        if !definition.elevated && path.cell.z <= 0 {
            commands.entity(path_entity).insert(PathSupportHydrated);
            continue;
        }
        let placement_step =
            openzt2_zoo_state::topology::ground_path_layout_calculation::calculate_snapped_ground_path_layout(
                path.cell,
                definition.width_cm,
                definitions.topology_cell_size_cm(),
            )
            .map_or(1, |(_, _, step)| step);
        let mut neighbours = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y]
            .into_iter()
            .filter(|offset| {
                topology_index
                    .paths
                    .contains_key(&(path.cell + *offset * placement_step))
            });
        let first_direction = neighbours.next();
        let path_turns = first_direction
            .zip(neighbours.next())
            .is_some_and(|(first, second)| first.dot(second) == 0);
        let curved_support_prefab = AssetId(definition.curve_support_prefab.0);
        let support_prefab = if path_turns && curved_support_prefab != AssetId::default() {
            curved_support_prefab
        } else {
            AssetId(definition.support_prefab.0)
        };
        let column = if path_turns {
            definition
                .curve_support_column
                .as_ref()
                .or(definition.support_column.as_ref())
        } else {
            definition.support_column.as_ref()
        };
        if support_prefab == AssetId::default() && column.is_none() {
            commands.entity(path_entity).insert(PathSupportHydrated);
            continue;
        }
        let path_world_position = topology_grid.cell_translation(path.cell);
        let support_top = path_world_position.y + f32::from(surface.height_cm[4]) * 0.01;
        let path_world_horizontal_position = path_world_position.xz();
        let Some(ground_height_metres) = lowest_ground_height_within_fit_radius(
            path_world_horizontal_position,
            column.map_or(0.0, |column| column.fit_lowest_radius_metres),
            &terrain_index,
            &terrain_chunks,
            &terrain_assets,
        ) else {
            continue;
        };
        let support_entity = commands
            .spawn((
                PathSupport {
                    ground_height_m: ground_height_metres,
                },
                Transform::from_translation(Vec3::new(
                    0.0,
                    ground_height_metres - path_world_position.y,
                    0.0,
                )),
                ChildOf(path_entity),
                Visibility::Inherited,
                *world_member,
            ))
            .id();
        if let Some(column) = column {
            let direction = first_direction.unwrap_or(IVec3::Y);
            if !openzt2_world::world_spawn::expanding_column_presentation::spawn_expanding_column_presentation(
                &mut commands, definitions, support_entity, Transform::IDENTITY,
                support_top - ground_height_metres,
                Quat::from_rotation_y((direction.x as f32).atan2(direction.y as f32)), column,
            ) {
                commands.entity(support_entity).despawn();
                continue;
            }
        } else {
            commands
                .entity(support_entity)
                .insert(PathSupportPrefab(support_prefab));
        }
        commands.entity(path_entity).insert(PathSupportHydrated);
    }
}

pub(super) fn lowest_ground_height_within_fit_radius(
    centre: Vec2,
    fit_radius_metres: f32,
    terrain_index: &TerrainIndex,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    terrain_assets: &Assets<TerrainAsset>,
) -> Option<f32> {
    FIT_LOWEST_SAMPLE_DIRECTIONS
        .iter()
        .filter_map(|direction| {
            let world = centre + *direction * fit_radius_metres;
            let (chunk, edited_samples) = terrain_chunks
                .get(terrain_chunk_at(terrain_index, world)?)
                .ok()?;
            sample_terrain(
                chunk,
                terrain_assets.get(&chunk.asset)?,
                edited_samples,
                world,
            )
            .map(|sample| sample.height_m)
        })
        .reduce(f32::min)
}

pub(super) fn remove_supports_when_path_tile_is_removed(
    removed: On<Remove<PathTile>>,
    children: Query<&Children>,
    supports: Query<(), With<PathSupport>>,
    mut commands: Commands,
) {
    if let Ok(children) = children.get(removed.entity) {
        for child in children.iter().filter(|child| supports.contains(*child)) {
            commands.entity(child).try_despawn();
        }
    }
    commands
        .entity(removed.entity)
        .try_remove::<PathSupportHydrated>();
}

pub(super) fn invalidate_path_supports_after_topology_or_terrain_changes(
    mut commands: Commands,
    mut topology_changes: MessageReader<
        openzt2_zoo_state::topology::topology_edit_types::PathConnectivityChanged,
    >,
    mut terrain_changes: MessageReader<
        openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged,
    >,
    terrain_chunks: Query<&TerrainChunk>,
    paths: Query<(&PathTile, Option<&Children>), With<PathSupportHydrated>>,
    changed_paths: Query<Entity, (Changed<PathTile>, With<PathSupportHydrated>)>,
    supports: Query<(), With<PathSupport>>,
    grid: Single<&TopologyGrid>,
    index: Single<&TopologyIndex>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
) {
    let grid = grid.into_inner();
    let index = index.into_inner();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let mut affected: bevy::ecs::entity::EntityHashSet = changed_paths.iter().collect();
    let reach_cells = IVec2::splat(maximum_path_step(definitions));
    for change in topology_changes.read() {
        let cell = change.0.xy();
        affected.extend(
            index
                .paths_within(cell - reach_cells, cell + reach_cells)
                .filter(|&entity| {
                    paths
                        .get(entity)
                        .is_ok_and(|(path, _)| path.cell.z == change.0.z)
                }),
        );
    }
    let regions = terrain_changes
        .read()
        .filter_map(|change| terrain_region(change, &terrain_chunks));
    let reach = definitions
        .paths()
        .flat_map(|path| {
            [
                path.support_column.as_ref(),
                path.curve_support_column.as_ref(),
            ]
            .into_iter()
            .flatten()
            .map(|column| column.fit_lowest_radius_metres)
        })
        .fold(0.0_f32, f32::max);
    for (min, max) in regions {
        let (min, max) = cells_for_region(*grid, min, max, reach);
        affected.extend(index.paths_within(min, max));
    }
    for entity in affected {
        let Ok((_, children)) = paths.get(entity) else {
            continue;
        };
        commands.entity(entity).remove::<PathSupportHydrated>();
        if let Some(children) = children {
            for child in children.iter().filter(|child| supports.contains(*child)) {
                commands.entity(child).try_despawn();
            }
        }
    }
}
