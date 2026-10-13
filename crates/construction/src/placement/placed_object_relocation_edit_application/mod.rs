//! Applies prepared placed-object relocation edits in either history direction.

use bevy::prelude::*;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;

use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_transform;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::calculate_object_placement_footprint_origin;
use openzt2_zoo_state::placement::object_placement_validation::object_placement_cell_is_blocked_by_existing_object;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectFootprintOccupancy;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementCellCollectionScratch;
use openzt2_zoo_state::placement::placement_preview_types::PlacementRotationRequest;
use openzt2_zoo_state::placement::placement_preview_types::RelocatingPlacedObject;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementMutation;

pub(super) fn apply_prepared_placed_object_relocation_edit(
    commands: &mut Commands,
    application: EditApplication,
    edit: &PreparedObjectPlacementMutation,
    catalogue: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    placement: &mut PlacedObjectFootprintOccupancyIndex,
    scratch: &mut ObjectPlacementCellCollectionScratch,
    placed: &mut Query<(
        &PersistentId,
        &PlacedObjectDefinitionReference,
        &mut Transform,
        &mut PlacedObjectFootprintOccupancy,
    )>,
) -> bool {
    let Some(target) = edit.applied_entity else {
        return false;
    };
    let forward = matches!(
        application,
        EditApplication::InitialCommit | EditApplication::Redo
    );
    let (transform, cells) = if forward {
        (edit.transform, edit.cells.as_ref())
    } else {
        let Some(transform) = edit.previous_transform else {
            return false;
        };
        (transform, edit.previous_cells.as_ref())
    };
    let Some(definition) = resolve_object_placeable_definition(catalogue, edit.definition) else {
        return false;
    };
    if cells.is_empty()
        || cells.iter().any(|cell| {
            object_placement_cell_is_blocked_by_existing_object(
                placement,
                *cell,
                Some(target),
                definition,
                catalogue,
                |entity| {
                    placed
                        .get(entity)
                        .ok()
                        .map(|(_, placeable, _, _)| placeable.definition)
                },
            )
        })
    {
        return false;
    }
    let Ok((id, placeable, mut live_transform, mut occupancy)) = placed.get_mut(target) else {
        return false;
    };
    if *id != edit.entity || placeable.definition != edit.definition {
        return false;
    }
    let Some(turns) = calculate_authored_object_placement_eighth_turns(definition, &transform)
    else {
        return false;
    };
    let Some(origin) = calculate_object_placement_footprint_origin(definition, &transform, turns)
    else {
        return false;
    };
    if !collect_occupied_placement_cells_for_transform(definition, &transform, &mut scratch.cells)
        .is_some_and(|expected| expected == cells)
    {
        return false;
    }
    for cell in edit.cells.iter().chain(edit.previous_cells.iter()) {
        placement.remove_entity_from_cell(*cell, target);
    }
    *live_transform = transform;
    *occupancy = PlacedObjectFootprintOccupancy {
        origin,
        eighth_turns: turns,
    };
    for cell in cells.iter().copied() {
        placement.insert_entity_into_cell(cell, target);
    }
    commands
        .entity(target)
        .remove::<RelocatingPlacedObject>()
        .remove::<PlacementRotationRequest>();
    if let Some(preview) = edit.preview {
        commands.entity(preview).despawn();
    }
    true
}
