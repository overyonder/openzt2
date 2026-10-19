use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use crate::topology::topology_construction_planning::calculate_fence_construction_plan;
use crate::topology::topology_construction_planning::calculate_path_construction_plan;
use crate::topology::topology_construction_planning::{
    GateHostEdges, calculate_gate_replacement_plan, fence_definition_is_gate,
};
use crate::topology::topology_construction_preview_types::FencePreview;
use crate::topology::topology_construction_preview_types::PathPreview;
use crate::topology::topology_edit_outcome_messages::write_topology_edit_preparation_rejection;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPreparationRejected;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPrepared;
use openzt2_zoo_state::topology::topology_edit_types::TopologySnapshot;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;

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
        // A closed shape returns to its first, still unbuilt node.
        let revisited = node_ids
            .iter()
            .find_map(|(visited, id)| (*visited == cell).then_some(*id));
        if let Some(id) = existing_id.or(revisited) {
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

/// Swaps the fence segment under a gate preview for the same segment with a
/// closed gate, so undo restores the plain fence.
fn prepare_gate_replacement_snapshots(
    preview: &FencePreview,
    index: &TopologyIndex,
    definitions: WorldDefinitionsView<'_>,
    hosts: &GateHostEdges,
    persistent: &Query<&PersistentId>,
    ids: &mut PersistentIdAllocator,
    root: Entity,
) -> Result<(TopologySnapshot, TopologySnapshot, Money), PlacementFailure> {
    let (edge, cost) = calculate_gate_replacement_plan(preview, index, definitions, hosts)?;
    let (fence, ..) = hosts
        .get(edge)
        .map_err(|_| PlacementFailure::InvalidTopology)?;
    let id = |entity| {
        persistent
            .get(entity)
            .copied()
            .map_err(|_| PlacementFailure::InvalidTopology)
    };
    let (a, b) = (id(fence.a)?, id(fence.b)?);
    let removed = TopologySnapshot::Fence {
        id: id(edge)?,
        definition: fence.definition,
        a,
        b,
        gate: None,
    };
    let created = TopologySnapshot::Fence {
        id: ids
            .allocate(root)
            .map_err(|_| PlacementFailure::InvalidTopology)?,
        definition: fence.definition,
        a,
        b,
        gate: Some(Gate {
            open: false,
            locked: false,
        }),
    };
    Ok((created, removed, cost))
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
    gate_hosts: GateHostEdges,
    (mut prepared, mut rejected): (
        MessageWriter<TopologyEditPrepared>,
        MessageWriter<TopologyEditPreparationRejected>,
    ),
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
            (Some(preview), None) if fence_definition_is_gate(definitions, preview.definition) => {
                prepare_gate_replacement_snapshots(
                    preview,
                    &index,
                    definitions,
                    &gate_hosts,
                    &persistent,
                    &mut ids,
                    member.root,
                )
                .map(|(created, removed, cost)| (vec![created], vec![removed], cost))
            }
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
            )
            .map(|(created, cost)| (created, Vec::new(), cost)),
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
            )
            .map(|(created, cost)| (created, Vec::new(), cost)),
            _ => Err(PlacementFailure::InvalidTopology),
        };
        match result {
            Ok((created, removed, cost)) => {
                commands.entity(request.transaction).insert(TopologyEdit {
                    created: created.into_boxed_slice(),
                    removed: removed.into_boxed_slice(),
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
