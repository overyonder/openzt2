use std::collections::BTreeSet;

use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::world_definitions::object_placement::{
    PlaceableDefinition, placement_stomp_rules::PlacementStompAction,
};
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId, prefab_source_asset_handle::PrefabSourceAssetHandle,
};
use openzt2_zoo_state::placement::{
    object_placement_definition_queries::{
        collect_occupied_placement_cells_for_origin_and_quarter_turns, resolve_object_placeable_definition,
    },
    object_placement_geometry::ScenePlacementFootprints,
    object_placement_validation::object_placement_stomp_action,
    placed_object_types::{PlacedObjectDefinitionReference, PlacedObjectFootprintOccupancy},
    placement_occupancy_index::PlacedObjectFootprintOccupancyIndex,
    placement_transaction_types::{PreparedObjectPlacementEditMutationKind, PreparedObjectPlacementMutation},
};

pub(super) type StompablePlacedObject = (
    &'static PersistentId,
    &'static PlacedObjectDefinitionReference,
    &'static Transform,
    &'static PlacedObjectFootprintOccupancy,
    &'static PrefabSourceAssetHandle,
);

pub(super) fn prepare_stomped_object_removals(
    cells: &[IVec2],
    definition: &PlaceableDefinition,
    catalogue: WorldDefinitionsView<'_>,
    footprints: &ScenePlacementFootprints,
    placement: &PlacedObjectFootprintOccupancyIndex,
    targets: &Query<StompablePlacedObject>,
    ignored: impl Fn(Entity) -> bool,
    scratch: &mut Vec<IVec2>,
) -> Option<Vec<PreparedObjectPlacementMutation>> {
    let overlapping = cells
        .iter()
        .flat_map(|cell| placement.entities_occupying_cell(*cell))
        .filter(|entity| !ignored(*entity))
        .collect::<BTreeSet<_>>();
    let mut removals = Vec::new();
    for entity in overlapping {
        let Ok((id, target, transform, occupancy, prefab)) = targets.get(entity) else {
            return None;
        };
        if object_placement_stomp_action(definition, catalogue, Some(target.definition))
            != PlacementStompAction::Delete
        {
            continue;
        }
        let target_definition = resolve_object_placeable_definition(catalogue, target.definition)?;
        let target_geometry = footprints.resolve(target_definition, Some(&prefab.0));
        let cells = collect_occupied_placement_cells_for_origin_and_quarter_turns(
            target_geometry.cells(occupancy.eighth_turns),
            occupancy.origin,
            occupancy.eighth_turns / 2,
            scratch,
        )?;
        removals.push(PreparedObjectPlacementMutation {
            entity: *id,
            definition: target.definition,
            transform: *transform,
            cells: cells.to_vec().into_boxed_slice(),
            previous_transform: None,
            previous_cells: Box::new([]),
            mutation: PreparedObjectPlacementEditMutationKind::Remove,
            applied_entity: Some(entity),
            preview: None,
            prefab: prefab.0.clone(),
        });
    }
    removals.sort_unstable_by_key(|removal| removal.entity.0);
    Some(removals)
}
