//! Queries and projections over authored object-placement definitions.

use bevy::prelude::*;
use openzt2_game_data::world_definitions::object_placement::{
    EntranceDefinition, EntrancePurpose, FootprintCell, FootprintCellFlags, PlaceableDefinition,
};

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::game_session_types::WorldSessionMode;
use crate::plugins::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use crate::plugins::progression::catalogue_entry_availability::catalogue_entry_is_available;
use crate::plugins::progression::unlock_types::UnlockedCatalogueDefinitionSet;

use super::{
    object_placement_validation::{
        calculate_authored_object_placement_eighth_turns,
        calculate_object_placement_footprint_origin, rotate_object_placement_cell_by_quarter_turns,
    },
    PlacedObjectAuthoredEntrance,
};

pub(crate) fn collect_occupied_placement_cells_for_transform<'a>(
    definition: &PlaceableDefinition,
    transform: &Transform,
    scratch: &'a mut Vec<IVec2>,
) -> Option<&'a [IVec2]> {
    let turns = calculate_authored_object_placement_eighth_turns(definition, transform)?;
    collect_occupied_placement_cells_for_origin_and_quarter_turns(
        select_authored_footprint_for_eighth_turns(definition, turns),
        calculate_object_placement_footprint_origin(definition, transform, turns)?,
        turns / 2,
        scratch,
    )
}

pub(super) fn collect_occupied_placement_cells_for_origin_and_quarter_turns<'a>(
    footprint: &[FootprintCell],
    origin: IVec2,
    turns: u8,
    scratch: &'a mut Vec<IVec2>,
) -> Option<&'a [IVec2]> {
    scratch.clear();
    scratch.reserve(footprint.len());
    footprint
        .iter()
        .filter(|cell| cell.flags.contains_all(FootprintCellFlags::OCCUPIED))
        .for_each(|cell| {
            scratch.push(
                origin
                    + rotate_object_placement_cell_by_quarter_turns(
                        IVec2::new(i32::from(cell.offset[0]), i32::from(cell.offset[1])),
                        turns,
                    ),
            );
        });
    scratch.sort_unstable_by_key(|cell| (cell.y, cell.x));
    scratch.dedup();
    (!scratch.is_empty()).then_some(scratch.as_slice())
}

pub(super) fn select_authored_footprint_for_eighth_turns<'a>(
    definition: &'a PlaceableDefinition,
    eighth_turns: u8,
) -> &'a [FootprintCell] {
    let selected = if eighth_turns & 1 == 0 {
        &definition.footprint
    } else {
        &definition.diagonal_footprint
    };
    selected
}

/// Object IDs are the public placement identity. PlacedObjectDefinitionReference definitions use the
/// same stable ID and are resolved through the active archive-precedence index.
pub(crate) fn resolve_object_placeable_definition<'a>(
    asset: WorldDefinitionsView<'a>,
    object: openzt2_game_data::AssetId,
) -> Option<&'a PlaceableDefinition> {
    asset.find_object(object)?;
    asset.find_placeable(object)
}

pub(super) fn object_placeable_definition_is_unlocked(
    asset: WorldDefinitionsView<'_>,
    object: openzt2_game_data::AssetId,
    mode: Option<WorldSessionMode>,
    scenario: Option<&ScenarioContentAvailability>,
    unlocks: &UnlockedCatalogueDefinitionSet,
) -> bool {
    let Some((index, entry)) = asset
        .catalogue()
        .enumerate()
        .find(|(_, entry)| entry.definition.0 == object.0)
    else {
        return false;
    };
    catalogue_entry_is_available(asset, index, entry, mode, scenario, unlocks)
}

pub(super) fn authored_object_entrance_definitions(
    definition: &PlaceableDefinition,
) -> &[EntranceDefinition] {
    &definition.entrances
}

pub(super) fn project_authored_object_entrance_definition_to_component(
    entrance: &EntranceDefinition,
) -> PlacedObjectAuthoredEntrance {
    let position = entrance.position_cm.map(|value| f32::from(value) * 0.01);
    let forward = entrance
        .forward_snorm
        .map(|value| f32::from(value) / f32::from(i16::MAX));
    PlacedObjectAuthoredEntrance {
        local_position: Vec3::from_array(position),
        local_forward: Vec3::from_array(forward).normalize_or_zero(),
        purpose: match &entrance.purpose {
            EntrancePurpose::Guest => EntrancePurpose::Guest,
            EntrancePurpose::Staff => EntrancePurpose::Staff,
            EntrancePurpose::Service => EntrancePurpose::Service,
            EntrancePurpose::Vehicle => EntrancePurpose::Vehicle,
            EntrancePurpose::Animal => EntrancePurpose::Animal,
        },
    }
}
