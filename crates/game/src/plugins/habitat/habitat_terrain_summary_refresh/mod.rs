use crate::plugins::topology::topology_graph_types::TopologyGrid;
use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::terrain::terrain_world_sampling::sample_authored_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::terrain::terrain_edit_types::TerrainChanged;

use super::{
    habitat_terrain_summary_calculation::{
        calculate_habitat_terrain_summary_from_topology_cells, HabitatTopologyCellTerrainFact,
    },
    habitat_topology_cell_queries::{
        convert_finite_world_position_to_topology_cell, topology_cells_overlap_inclusive_bounds,
    },
    habitat_types::{
        HabitatChanged, HabitatClimate, HabitatRegion, HabitatSummary,
        RefreshTerrainDerivedHabitatSummariesWithinTopologyBounds,
    },
};

/// Requests refreshes for terrain-derived habitat summaries after edited
/// samples change. One-cell padding covers bilinear samples on either side of
/// the edited sample rectangle.
pub(super) fn request_habitat_summary_refreshes_after_terrain_changes(
    mut terrain_changes: MessageReader<TerrainChanged>,
    terrain_chunks: Query<&TerrainChunk>,
    topology_grid: Res<TopologyGrid>,
    mut habitat_summary_refreshes: MessageWriter<
        RefreshTerrainDerivedHabitatSummariesWithinTopologyBounds,
    >,
) {
    for terrain_change in terrain_changes.read() {
        let Ok(terrain_chunk) = terrain_chunks.get(terrain_change.chunk) else {
            continue;
        };
        let Some(topology_bounds) = calculate_topology_cell_bounds_affected_by_terrain_change(
            terrain_chunk,
            *topology_grid,
            terrain_change.min,
            terrain_change.max,
        ) else {
            continue;
        };
        habitat_summary_refreshes.write(RefreshTerrainDerivedHabitatSummariesWithinTopologyBounds(
            topology_bounds,
        ));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn refresh_terrain_derived_habitat_summaries(
    mut commands: Commands,
    mut refresh_requests: MessageReader<RefreshTerrainDerivedHabitatSummariesWithinTopologyBounds>,
    terrain_index: Res<TerrainIndex>,
    topology_grid: Res<TopologyGrid>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut habitats: Query<(
        Entity,
        &HabitatRegion,
        &mut HabitatSummary,
        Option<&mut HabitatClimate>,
    )>,
    mut habitat_changes: MessageWriter<HabitatChanged>,
) {
    let Some(topology_bounds) =
        merge_requested_habitat_summary_refresh_bounds(&mut refresh_requests)
    else {
        return;
    };
    let topology_cell_size_metres = topology_grid.spacing_m;
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let topology_cell_area_square_metres = topology_cell_size_metres * topology_cell_size_metres;
    for (habitat_entity, habitat_region, mut habitat_summary, habitat_climate) in &mut habitats {
        if !topology_cells_overlap_inclusive_bounds(&habitat_region.topology_cells, topology_bounds)
        {
            continue;
        }
        let mut read_terrain_cell_fact = |topology_cell| {
            read_terrain_fact_for_topology_cell(
                topology_cell,
                *topology_grid,
                &terrain_index,
                &terrain_assets,
                &terrain_chunks,
            )
        };
        let refreshed_summary = calculate_habitat_terrain_summary_from_topology_cells(
            &habitat_region.topology_cells,
            topology_cell_area_square_metres,
            habitat_summary.boundary_is_breached,
            &mut read_terrain_cell_fact,
        );
        if *habitat_summary == refreshed_summary {
            continue;
        }
        *habitat_summary = refreshed_summary;
        match (
            derive_dominant_habitat_climate(&habitat_summary, world_definitions),
            habitat_climate,
        ) {
            (Some(refreshed_climate), Some(mut current_climate)) => {
                *current_climate = refreshed_climate;
            }
            (Some(refreshed_climate), None) => {
                commands.entity(habitat_entity).insert(refreshed_climate);
            }
            (None, Some(_)) => {
                commands.entity(habitat_entity).remove::<HabitatClimate>();
            }
            (None, None) => {}
        }
        habitat_changes.write(HabitatChanged {
            habitat_entity,
            changed_topology_bounds: topology_bounds,
        });
    }
}

pub(super) fn derive_dominant_habitat_climate(
    habitat_summary: &HabitatSummary,
    world_definitions: WorldDefinitionsView<'_>,
) -> Option<HabitatClimate> {
    let dominant_biome_identifier = habitat_summary
        .biome_areas_square_metres
        .iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))?
        .0;
    let biome_definition = world_definitions.find_biome(dominant_biome_identifier)?;
    Some(HabitatClimate {
        biome_identifier: dominant_biome_identifier,
        temperature_range_celsius: biome_definition.temperature_c.map(|value| value),
        humidity_range_permille: biome_definition.humidity_permille.map(|value| value),
    })
}

pub(super) fn calculate_topology_cell_bounds_affected_by_terrain_change(
    terrain_chunk: &TerrainChunk,
    topology_grid: TopologyGrid,
    minimum_changed_sample: UVec2,
    maximum_changed_sample: UVec2,
) -> Option<IRect> {
    if !terrain_chunk.origin.is_finite()
        || !terrain_chunk.spacing_m.is_finite()
        || terrain_chunk.spacing_m <= 0.0
        || minimum_changed_sample.cmpgt(maximum_changed_sample).any()
    {
        return None;
    }
    let minimum_world_position =
        terrain_chunk.origin + minimum_changed_sample.as_vec2() * terrain_chunk.spacing_m;
    let maximum_world_position =
        terrain_chunk.origin + maximum_changed_sample.as_vec2() * terrain_chunk.spacing_m;
    let minimum_topology_cell = convert_finite_world_position_to_topology_cell(
        minimum_world_position - topology_grid.origin,
        topology_grid.spacing_m,
    )?;
    let maximum_topology_cell = convert_finite_world_position_to_topology_cell(
        maximum_world_position - topology_grid.origin,
        topology_grid.spacing_m,
    )?;
    Some(IRect::from_corners(
        minimum_topology_cell - IVec2::ONE,
        maximum_topology_cell + IVec2::ONE,
    ))
}

pub(super) fn read_terrain_fact_for_topology_cell(
    topology_cell: IVec2,
    topology_grid: TopologyGrid,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> Option<HabitatTopologyCellTerrainFact> {
    let world_position = topology_grid.cell_translation(topology_cell.extend(0)).xz()
        + Vec2::splat(0.5 * topology_grid.spacing_m);
    let terrain_chunk_entity = terrain_chunk_at(terrain_index, world_position)?;
    let (terrain_chunk, edited_terrain_samples) = terrain_chunks.get(terrain_chunk_entity).ok()?;
    let terrain_asset = terrain_assets.get(&terrain_chunk.asset)?;
    let terrain_sample = sample_authored_terrain(
        terrain_chunk,
        terrain_asset,
        edited_terrain_samples,
        world_position,
    )?;
    Some(HabitatTopologyCellTerrainFact {
        water: terrain_sample.geometry.water_height_m.is_some(),
        biome: terrain_sample.biome.map(|biome| biome.id),
    })
}

fn merge_requested_habitat_summary_refresh_bounds(
    refresh_requests: &mut MessageReader<RefreshTerrainDerivedHabitatSummariesWithinTopologyBounds>,
) -> Option<IRect> {
    refresh_requests
        .read()
        .fold(None, |merged_bounds, request| {
            Some(merged_bounds.map_or(request.0, |current: IRect| {
                IRect::from_corners(
                    current.min.min(request.0.min),
                    current.max.max(request.0.max),
                )
            }))
        })
}
