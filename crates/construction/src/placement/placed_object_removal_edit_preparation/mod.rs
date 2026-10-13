use bevy::prelude::*;
use openzt2_game_data::world_definitions::world_objects::WorldObjectPropertyFlags;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareDeletion;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::prefab_source_asset_handle::PrefabSourceAssetHandle;

use crate::placement::object_placement_request_routing::reject_object_placement_edit_preparation;
use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_origin_and_quarter_turns;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_definition_queries::select_authored_footprint_for_eighth_turns;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectFootprintOccupancy;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementCellCollectionScratch;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPreparationRejected;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPrepared;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEdit;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEditMutationKind;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementMutation;

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
    transport_sales: openzt2_zoo_state::transport_tours::transport_track_sale::TransportTrackSaleQueries,
) {
    let catalogue = active_definitions.get(&definitions);
    for request in requests.read() {
        if transport_sales.requires_transport_sale(request.target) {
            continue;
        }
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
            openzt2_zoo_state::economy::object_sale_refund::calculate_authored_object_sale_refund(
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
            cost: openzt2_zoo_state::economy::money_types::Money(refund),
        });
    }
}
