//! Applies prepared placed-object removal edits.

use bevy::prelude::*;

use crate::plugins::world_spawn::persistent_id_types::PersistentId;

use super::{
    object_placement_definition_queries::{
        collect_occupied_placement_cells_for_transform, resolve_object_placeable_definition,
    },
    placed_object_types::{PlacedObjectDefinitionReference, PlacedObjectFootprintOccupancy},
    placement_occupancy_index::PlacedObjectFootprintOccupancyIndex,
    placement_preview_types::ObjectPlacementCellCollectionScratch,
    placement_transaction_types::PreparedObjectPlacementMutation,
};

pub(super) fn apply_prepared_placed_object_removal_edit(
    commands: &mut Commands,
    edit: &mut PreparedObjectPlacementMutation,
    catalogue: crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    placement: &mut PlacedObjectFootprintOccupancyIndex,
    scratch: &mut ObjectPlacementCellCollectionScratch,
    placed: &Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &mut Transform,
        &mut PlacedObjectFootprintOccupancy,
    )>,
) -> bool {
    let target = edit.applied_entity.or_else(|| {
        edit.cells.first().and_then(|cell| {
            placement.entities_occupying_cell(*cell).find(|entity| {
                placed
                    .get(*entity)
                    .is_ok_and(|(id, _, _, _)| *id == edit.entity)
            })
        })
    });
    let Some(target) = target else {
        return false;
    };
    let valid = placed
        .get(target)
        .ok()
        .and_then(|(id, placeable, transform, _)| {
            let definition = resolve_object_placeable_definition(catalogue, placeable.definition)?;
            let exact_cells = collect_occupied_placement_cells_for_transform(
                definition,
                transform,
                &mut scratch.cells,
            )
            .is_some_and(|expected| expected == edit.cells.as_ref());
            Some(
                *id == edit.entity
                    && placeable.definition == edit.definition
                    && exact_cells
                    && edit
                        .cells
                        .iter()
                        .all(|cell| placement.cell_contains_entity(*cell, target)),
            )
        })
        .unwrap_or(false);
    if !valid {
        return false;
    }
    for cell in edit.cells.iter() {
        placement.remove_entity_from_cell(*cell, target);
    }
    commands.entity(target).despawn();
    edit.applied_entity = None;
    true
}
