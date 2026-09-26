use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_interaction_types::ConstructionPreview;
use crate::plugins::construction::construction_interaction_types::PlacementFailure;
use crate::plugins::construction::construction_interaction_types::PlacementValidity;
use crate::plugins::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use crate::plugins::construction::construction_transaction_types::PrepareConstruction;
use crate::plugins::habitat::habitat_types::HabitatMember;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    ground_path_object_placement_collision_queries::GroundPathTilesUnderObjectPlacement,
    object_placement_definition_queries::{
        collect_occupied_placement_cells_for_origin_and_quarter_turns,
        collect_occupied_placement_cells_for_transform, resolve_object_placeable_definition,
        select_authored_footprint_for_eighth_turns,
    },
    object_placement_request_routing::reject_object_placement_edit_preparation,
    object_placement_validation::{
        calculate_authored_object_placement_eighth_turns,
        object_placement_cell_is_blocked_by_existing_object,
        validate_object_placement_footprint_cells, ObjectPlacementAuthoritativeFacts,
    },
    ObjectPlacementCellCollectionScratch, ObjectPlacementEditPreparationRejected,
    ObjectPlacementEditPrepared, ObjectPlacementPrefabSource,
    ObjectPlacementPreviewPermissionFacts, PlacedObjectDefinitionReference,
    PlacedObjectFootprintOccupancy, PlacedObjectFootprintOccupancyIndex,
    PlacedObjectRelocationSource, PreparedObjectPlacementEdit,
    PreparedObjectPlacementEditMutationKind, PreparedObjectPlacementMutation,
    RelocatingPlacedObject,
};

/// Validates and records one requested placement or relocation transaction.
pub(super) fn prepare_object_placement_or_relocation_edit(
    mut commands: Commands,
    mut requests: MessageReader<PrepareConstruction>,
    tool: Res<ConstructionTool>,
    previews: Query<(
        &ConstructionPreview,
        &WorldMember,
        Option<&ObjectPlacementPreviewPermissionFacts>,
        Option<&HabitatMember>,
        Option<&PlacedObjectRelocationSource>,
        &ObjectPlacementPrefabSource,
    )>,
    relocating: Query<
        (
            &PersistentId,
            &PlacedObjectDefinitionReference,
            &Transform,
            &PlacedObjectFootprintOccupancy,
        ),
        With<RelocatingPlacedObject>,
    >,
    placed: Query<&PlacedObjectDefinitionReference>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    (placement, ground_paths): (
        Res<PlacedObjectFootprintOccupancyIndex>,
        GroundPathTilesUnderObjectPlacement,
    ),
    mut allocator: ResMut<PersistentIdAllocator>,
    mut scratch: ResMut<ObjectPlacementCellCollectionScratch>,
    mut prepared: MessageWriter<ObjectPlacementEditPrepared>,
    mut rejected: MessageWriter<ObjectPlacementEditPreparationRejected>,
) {
    let ConstructionTool::Place(active_definition) = *tool else {
        return;
    };
    let catalogue = active_definitions.get(&definitions);
    for request in requests.read() {
        let Ok((preview, member, permission, habitat, relocation, prefab)) =
            previews.get(request.preview)
        else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::InvalidTopology,
            );
            continue;
        };
        let Some(catalogue) = catalogue else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::Locked,
            );
            continue;
        };
        if preview.definition != active_definition {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::InvalidTopology,
            );
            continue;
        }
        let Some(permission) = permission else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::Locked,
            );
            continue;
        };
        let Some(definition) = resolve_object_placeable_definition(catalogue, preview.definition)
        else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::AuthoredRule(preview.definition),
            );
            continue;
        };
        let Some(turns) =
            calculate_authored_object_placement_eighth_turns(definition, &preview.transform)
        else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::AuthoredRule(preview.definition),
            );
            continue;
        };
        let footprint = select_authored_footprint_for_eighth_turns(definition, turns);
        let terrain = |position: Vec2| {
            let entity = terrain_chunk_at(&terrain_index, position)?;
            let (chunk, edited) = chunks.get(entity).ok()?;
            let asset = terrain_assets.get(&chunk.asset)?;
            sample_terrain(chunk, asset, edited, position)
        };
        let collides_with_ground_path =
            ground_paths.object_placement_cell_collides_with_ground_path(definition, catalogue);
        let validity = validate_object_placement_footprint_cells(
            definition,
            footprint,
            &preview.transform,
            terrain,
            |cell| {
                collides_with_ground_path(cell)
                    || object_placement_cell_is_blocked_by_existing_object(
                        &placement,
                        cell,
                        relocation.map(|source| source.0),
                        definition,
                        catalogue,
                        |entity| {
                            placed
                                .get(entity)
                                .ok()
                                .map(|placeable| placeable.definition)
                        },
                    )
            },
            ObjectPlacementAuthoritativeFacts {
                habitat: habitat.map(|member| member.habitat_entity),
                unlocked: permission.unlocked,
                affordable: permission.affordable,
                topology_valid: permission.topology_valid,
                headroom_valid: permission.headroom_valid,
            },
        );
        let PlacementValidity::Valid { cost } = validity else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                match validity {
                    PlacementValidity::Invalid(reason) => reason,
                    _ => PlacementFailure::InvalidTopology,
                },
            );
            continue;
        };
        let Some(cells) = collect_occupied_placement_cells_for_transform(
            definition,
            &preview.transform,
            &mut scratch.cells,
        ) else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::InvalidTopology,
            );
            continue;
        };
        let cells = cells.to_vec();
        let (id, previous_transform, previous_cells, mutation, applied_entity, cost) =
            if let Some(source) = relocation {
                let Ok((id, placeable, transform, occupancy)) = relocating.get(source.0) else {
                    reject_object_placement_edit_preparation(
                        &mut rejected,
                        request.transaction,
                        PlacementFailure::InvalidTopology,
                    );
                    continue;
                };
                if placeable.definition != preview.definition {
                    reject_object_placement_edit_preparation(
                        &mut rejected,
                        request.transaction,
                        PlacementFailure::InvalidTopology,
                    );
                    continue;
                }
                let Some(previous_definition) =
                    resolve_object_placeable_definition(catalogue, placeable.definition)
                else {
                    reject_object_placement_edit_preparation(
                        &mut rejected,
                        request.transaction,
                        PlacementFailure::AuthoredRule(placeable.definition),
                    );
                    continue;
                };
                let Some(previous_cells) =
                    collect_occupied_placement_cells_for_origin_and_quarter_turns(
                        select_authored_footprint_for_eighth_turns(
                            previous_definition,
                            occupancy.eighth_turns,
                        ),
                        occupancy.origin,
                        occupancy.eighth_turns / 2,
                        &mut scratch.cells,
                    )
                    .map(<[IVec2]>::to_vec)
                else {
                    reject_object_placement_edit_preparation(
                        &mut rejected,
                        request.transaction,
                        PlacementFailure::InvalidTopology,
                    );
                    continue;
                };
                (
                    *id,
                    Some(*transform),
                    previous_cells,
                    PreparedObjectPlacementEditMutationKind::Relocate,
                    Some(source.0),
                    crate::plugins::economy::money_types::Money(0),
                )
            } else {
                let Ok(id) = allocator.allocate(member.root) else {
                    reject_object_placement_edit_preparation(
                        &mut rejected,
                        request.transaction,
                        PlacementFailure::InvalidTopology,
                    );
                    continue;
                };
                (
                    id,
                    None,
                    Vec::new(),
                    PreparedObjectPlacementEditMutationKind::Create,
                    None,
                    cost,
                )
            };
        commands
            .entity(request.transaction)
            .insert(PreparedObjectPlacementEdit {
                mutations: Box::new([PreparedObjectPlacementMutation {
                    entity: id,
                    definition: preview.definition,
                    transform: preview.transform,
                    cells: cells.into_boxed_slice(),
                    previous_transform,
                    previous_cells: previous_cells.into_boxed_slice(),
                    mutation,
                    applied_entity,
                    preview: Some(request.preview),
                    prefab: prefab.0.clone(),
                }]),
            });
        prepared.write(ObjectPlacementEditPrepared {
            transaction: request.transaction,
            cost,
        });
    }
}
