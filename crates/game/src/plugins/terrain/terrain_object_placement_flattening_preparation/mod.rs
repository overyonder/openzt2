use bevy::prelude::*;
use openzt2_game_data::world_definitions::object_placement::PlacementConstraints;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_interaction_types::ConstructionPreview;
use crate::plugins::construction::construction_interaction_types::PlacementFailure;
use crate::plugins::construction::construction_transaction_types::PrepareConstruction;
use crate::plugins::economy::money_types::Money;
use crate::plugins::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_transform;
use crate::plugins::placement::object_placement_definition_queries::resolve_object_placeable_definition;

use super::{
    terrain_chunk_identity_type::TerrainChunkId,
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
    terrain_edit_data_types::TerrainEditDelta,
    terrain_edit_rectangle_operations::terrain_sample_rectangle_length,
    terrain_edit_types::{TerrainEdit, TerrainEditPreparationRejected, TerrainEditPrepared},
    terrain_sample_grid_queries::read_materialized_terrain_sample,
    terrain_world_sampling::{terrain_cell_indices, terrain_chunk_at},
};

#[allow(
    clippy::too_many_arguments,
    reason = "the Bevy system reads each canonical construction and terrain owner directly"
)]
pub(super) fn prepare_authored_object_placement_terrain_flattening(
    mut commands: Commands,
    mut requests: MessageReader<PrepareConstruction>,
    previews: Query<&ConstructionPreview>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    chunks: Query<(
        &TerrainChunk,
        Option<&EditedTerrainSamples>,
        &TerrainChunkId,
    )>,
    mut prepared: MessageWriter<TerrainEditPrepared>,
    mut rejected: MessageWriter<TerrainEditPreparationRejected>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for request in requests.read() {
        let Ok(preview) = previews.get(request.preview) else {
            continue;
        };
        let Some(definition) = resolve_object_placeable_definition(catalogue, preview.definition)
        else {
            continue;
        };
        if !definition
            .constraints
            .contains_all(PlacementConstraints::FLATTEN_TERRAIN_TO_PLACEMENT_HEIGHT)
        {
            continue;
        }
        let mut occupied_cells = Vec::new();
        let Some(occupied_cells) = collect_occupied_placement_cells_for_transform(
            definition,
            &preview.transform,
            &mut occupied_cells,
        ) else {
            reject_authored_object_placement_terrain_flattening(&mut rejected, request.transaction);
            continue;
        };
        let target_height_centimetres = preview.transform.translation.y * 100.0;
        if !target_height_centimetres.is_finite()
            || target_height_centimetres < f32::from(i16::MIN)
            || target_height_centimetres > f32::from(i16::MAX)
        {
            reject_authored_object_placement_terrain_flattening(&mut rejected, request.transaction);
            continue;
        }
        let target_height_centimetres = target_height_centimetres.round() as i16;
        let Some(flattened_world_sample_positions) =
            collect_authored_object_placement_flattened_world_sample_positions(
                occupied_cells,
                &terrain_index,
                &chunks,
            )
        else {
            reject_authored_object_placement_terrain_flattening(&mut rejected, request.transaction);
            continue;
        };
        let mut deltas = chunks
            .iter()
            .filter_map(|(chunk, edited, persistent)| {
                prepare_authored_object_placement_terrain_flattening_delta(
                    persistent,
                    chunk,
                    terrain_assets.get(&chunk.asset)?,
                    edited,
                    &flattened_world_sample_positions,
                    target_height_centimetres,
                )
            })
            .collect::<Vec<_>>();
        deltas.sort_unstable_by_key(|delta| delta.chunk.0);
        if deltas.is_empty() {
            continue;
        }
        commands
            .entity(request.transaction)
            .insert(TerrainEdit { deltas });
        prepared.write(TerrainEditPrepared {
            transaction: request.transaction,
            cost: Money(0),
        });
    }
}

fn collect_authored_object_placement_flattened_world_sample_positions(
    occupied_cells: &[IVec2],
    terrain_index: &TerrainIndex,
    chunks: &Query<(
        &TerrainChunk,
        Option<&EditedTerrainSamples>,
        &TerrainChunkId,
    )>,
) -> Option<Vec<Vec2>> {
    let mut positions = Vec::with_capacity(occupied_cells.len().saturating_mul(4));
    for cell in occupied_cells {
        let chunk_entity = terrain_chunk_at(terrain_index, cell.as_vec2())?;
        let (chunk, _, _) = chunks.get(chunk_entity).ok()?;
        let terrain_cell = terrain_cell_indices(chunk, cell.as_vec2())?;
        for z_offset in 0..=1 {
            for x_offset in 0..=1 {
                let sample = terrain_cell + UVec2::new(x_offset, z_offset);
                positions.push(chunk.origin + sample.as_vec2() * chunk.spacing_m);
            }
        }
    }
    positions.sort_unstable_by_key(|position| (position.y.to_bits(), position.x.to_bits()));
    positions.dedup_by_key(|position| (position.y.to_bits(), position.x.to_bits()));
    (!positions.is_empty()).then_some(positions)
}

fn prepare_authored_object_placement_terrain_flattening_delta(
    persistent: &TerrainChunkId,
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
    flattened_world_sample_positions: &[Vec2],
    target_height_centimetres: i16,
) -> Option<TerrainEditDelta> {
    let mut selected_samples = flattened_world_sample_positions
        .iter()
        .filter_map(|position| {
            let grid = (*position - chunk.origin) / chunk.spacing_m;
            let rounded = grid.round();
            (chunk.spacing_m > 0.0
                && grid.is_finite()
                && (grid - rounded)
                    .abs()
                    .cmple(Vec2::splat(f32::EPSILON * 8.0))
                    .all()
                && rounded.cmpge(Vec2::ZERO).all()
                && rounded.cmplt(Vec2::splat(f32::from(chunk.side))).all())
            .then(|| rounded.as_uvec2())
        })
        .collect::<Vec<_>>();
    selected_samples.sort_unstable_by_key(|sample| (sample.y, sample.x));
    selected_samples.dedup();
    let minimum = selected_samples.iter().copied().reduce(UVec2::min)?;
    let maximum = selected_samples.iter().copied().reduce(UVec2::max)?;
    let mut before = Vec::with_capacity(terrain_sample_rectangle_length(minimum, maximum)?);
    let mut after = Vec::with_capacity(before.capacity());
    let mut changed = false;
    for z in minimum.y..=maximum.y {
        for x in minimum.x..=maximum.x {
            let sample =
                read_materialized_terrain_sample(chunk, asset, edited, x as usize, z as usize)?;
            before.push(sample);
            let mut flattened = sample;
            if selected_samples
                .binary_search_by_key(&(z, x), |sample| (sample.y, sample.x))
                .is_ok()
            {
                flattened.height_cm = target_height_centimetres;
                changed |= flattened != sample;
            }
            after.push(flattened);
        }
    }
    changed.then(|| TerrainEditDelta {
        chunk: *persistent,
        min: minimum,
        max: maximum,
        before: before.into_boxed_slice(),
        after: after.into_boxed_slice(),
    })
}

fn reject_authored_object_placement_terrain_flattening(
    rejected: &mut MessageWriter<TerrainEditPreparationRejected>,
    transaction: Entity,
) {
    rejected.write(TerrainEditPreparationRejected {
        transaction,
        reason: PlacementFailure::OutsideMap,
    });
}
