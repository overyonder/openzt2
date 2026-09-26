use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::construction::construction_interaction_types::PlacementFailure;
use crate::plugins::construction::construction_transaction_types::PrepareConstruction;
use crate::plugins::economy::money_types::Money;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    topology_construction_planning::{
        calculate_fence_construction_plan, calculate_path_construction_plan,
    },
    topology_construction_preview_types::{FencePreview, PathPreview},
    topology_edit_outcome_messages::write_topology_edit_preparation_rejection,
    topology_edit_types::{
        TopologyEdit, TopologyEditPreparationRejected, TopologyEditPrepared, TopologySnapshot,
    },
    topology_graph_types::{TopologyGrid, TopologyIndex},
};

fn prepare_fence_construction_snapshots(
    preview: &FencePreview,
    index: &TopologyIndex,
    grid: TopologyGrid,
    definitions: WorldDefinitionsView<'_>,
    persistent: &Query<&PersistentId>,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    ids: &mut PersistentIdAllocator,
    root: Entity,
) -> Result<(Vec<TopologySnapshot>, Money), PlacementFailure> {
    let (cells, cost, gate) = calculate_fence_construction_plan(
        preview,
        index,
        grid,
        definitions,
        terrain_index,
        terrain_assets,
        terrain_chunks,
    )?;
    let existing_ids = cells
        .iter()
        .map(|cell| {
            index.nodes.get(cell).map_or(Ok(None), |entity| {
                persistent
                    .get(*entity)
                    .copied()
                    .map(Some)
                    .map_err(|_| PlacementFailure::InvalidTopology)
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut snapshots = Vec::with_capacity(cells.len() * 2 - 1);
    let mut node_ids = Vec::with_capacity(cells.len());
    for (&cell, existing_id) in cells.iter().zip(existing_ids) {
        if let Some(id) = existing_id {
            node_ids.push((cell, id));
        } else {
            let id = ids
                .allocate(root)
                .map_err(|_| PlacementFailure::InvalidTopology)?;
            snapshots.push(TopologySnapshot::Node { id, cell });
            node_ids.push((cell, id));
        }
    }
    for pair in node_ids.windows(2) {
        let a = pair[0].1;
        let b = pair[1].1;
        snapshots.push(TopologySnapshot::Fence {
            id: ids
                .allocate(root)
                .map_err(|_| PlacementFailure::InvalidTopology)?,
            definition: preview.definition,
            a,
            b,
            gate,
        });
    }
    Ok((snapshots, cost))
}

fn prepare_path_construction_snapshots(
    preview: &PathPreview,
    index: &TopologyIndex,
    grid: TopologyGrid,
    definitions: WorldDefinitionsView<'_>,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    ids: &mut PersistentIdAllocator,
    root: Entity,
) -> Result<(Vec<TopologySnapshot>, Money), PlacementFailure> {
    let (cells, cost) = calculate_path_construction_plan(
        preview,
        index,
        grid,
        definitions,
        terrain_index,
        terrain_assets,
        terrain_chunks,
    )?;
    let mut snapshots = Vec::with_capacity(cells.len());
    for cell in cells {
        snapshots.push(TopologySnapshot::Path {
            id: ids
                .allocate(root)
                .map_err(|_| PlacementFailure::InvalidTopology)?,
            definition: preview.definition,
            cell,
        });
    }
    Ok((snapshots, cost))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_topology_construction_transaction(
    mut commands: Commands,
    mut requests: MessageReader<PrepareConstruction>,
    index: Res<TopologyIndex>,
    grid: Res<TopologyGrid>,
    terrain: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut ids: ResMut<PersistentIdAllocator>,
    previews: Query<
        (Option<&FencePreview>, Option<&PathPreview>),
        Or<(With<FencePreview>, With<PathPreview>)>,
    >,
    transactions: Query<&WorldMember>,
    persistent: Query<&PersistentId>,
    mut prepared: MessageWriter<TopologyEditPrepared>,
    mut rejected: MessageWriter<TopologyEditPreparationRejected>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for request in requests.read() {
        let Ok((fence, path)) = previews.get(request.preview) else {
            continue;
        };
        let Ok(member) = transactions.get(request.transaction) else {
            write_topology_edit_preparation_rejection(
                request.transaction,
                PlacementFailure::InvalidTopology,
                &mut rejected,
            );
            continue;
        };
        if terrain.chunks.is_empty() {
            write_topology_edit_preparation_rejection(
                request.transaction,
                PlacementFailure::OutsideMap,
                &mut rejected,
            );
            continue;
        }

        let result = match (fence, path) {
            (Some(preview), None) => prepare_fence_construction_snapshots(
                preview,
                &index,
                *grid,
                definitions,
                &persistent,
                &terrain,
                &terrain_assets,
                &terrain_chunks,
                &mut ids,
                member.root,
            ),
            (None, Some(preview)) => prepare_path_construction_snapshots(
                preview,
                &index,
                *grid,
                definitions,
                &terrain,
                &terrain_assets,
                &terrain_chunks,
                &mut ids,
                member.root,
            ),
            _ => Err(PlacementFailure::InvalidTopology),
        };
        match result {
            Ok((snapshots, cost)) => {
                commands.entity(request.transaction).insert(TopologyEdit {
                    created: snapshots.into_boxed_slice(),
                    removed: Box::new([]),
                });
                prepared.write(TopologyEditPrepared {
                    transaction: request.transaction,
                    cost,
                });
            }
            Err(reason) => write_topology_edit_preparation_rejection(
                request.transaction,
                reason,
                &mut rejected,
            ),
        }
    }
}
