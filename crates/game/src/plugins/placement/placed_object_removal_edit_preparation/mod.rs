use bevy::prelude::*;
use openzt2_game_data::world_definitions::world_objects::WorldObjectPropertyFlags;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_interaction_types::PlacementFailure;
use crate::plugins::construction::construction_transaction_types::PrepareDeletion;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::prefab_source_asset_handle::PrefabSourceAssetHandle;

use super::{
    object_placement_definition_queries::{
        collect_occupied_placement_cells_for_origin_and_quarter_turns,
        resolve_object_placeable_definition, select_authored_footprint_for_eighth_turns,
    },
    object_placement_request_routing::reject_object_placement_edit_preparation,
    ObjectPlacementCellCollectionScratch, ObjectPlacementEditPreparationRejected,
    ObjectPlacementEditPrepared, PlacedObjectDefinitionReference, PlacedObjectFootprintOccupancy,
    PreparedObjectPlacementEdit, PreparedObjectPlacementEditMutationKind,
    PreparedObjectPlacementMutation,
};

/// Validates and records one requested placed-object removal transaction.
pub(super) fn prepare_placed_object_removal_edit(
    mut commands: Commands,
    mut requests: MessageReader<PrepareDeletion>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    placed: Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &Transform,
        &PlacedObjectFootprintOccupancy,
        &PrefabSourceAssetHandle,
    )>,
    mut scratch: ResMut<ObjectPlacementCellCollectionScratch>,
    mut prepared: MessageWriter<ObjectPlacementEditPrepared>,
    mut rejected: MessageWriter<ObjectPlacementEditPreparationRejected>,
) {
    let catalogue = active_definitions.get(&definitions);
    for request in requests.read() {
        let Ok((id, placeable, transform, occupancy, prefab)) = placed.get(request.target) else {
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
        let Some(definition) = resolve_object_placeable_definition(catalogue, placeable.definition)
        else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::AuthoredRule(placeable.definition),
            );
            continue;
        };
        let Some(object) = catalogue.find_object(placeable.definition) else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::AuthoredRule(placeable.definition),
            );
            continue;
        };
        if !object
            .properties
            .contains_all(WorldObjectPropertyFlags::DELETABLE)
        {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::InvalidTopology,
            );
            continue;
        }
        let Some(cells) = collect_occupied_placement_cells_for_origin_and_quarter_turns(
            select_authored_footprint_for_eighth_turns(definition, occupancy.eighth_turns),
            occupancy.origin,
            occupancy.eighth_turns / 2,
            &mut scratch.cells,
        ) else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::InvalidTopology,
            );
            continue;
        };
        let Some(refund) =
            crate::plugins::economy::object_sale_refund::calculate_authored_object_sale_refund(
                object,
            )
            .and_then(|refund| refund.0.checked_neg())
        else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::Unaffordable,
            );
            continue;
        };
        commands
            .entity(request.transaction)
            .insert(PreparedObjectPlacementEdit {
                mutations: Box::new([PreparedObjectPlacementMutation {
                    entity: *id,
                    definition: placeable.definition,
                    transform: *transform,
                    cells: cells.to_vec().into_boxed_slice(),
                    previous_transform: None,
                    previous_cells: Box::new([]),
                    mutation: PreparedObjectPlacementEditMutationKind::Remove,
                    applied_entity: Some(request.target),
                    preview: None,
                    prefab: prefab.0.clone(),
                }]),
            });
        prepared.write(ObjectPlacementEditPrepared {
            transaction: request.transaction,
            cost: crate::plugins::economy::money_types::Money(refund),
        });
    }
}
