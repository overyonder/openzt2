use openzt2_game_data::world_definitions::extinct_animal_recovery::FossilPieceDefinition;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use super::fossil_collection_and_assembly_types::CollectedFossilPieceInventory;

pub(super) struct SelectedUncollectedFossilPiece<'a> {
    pub(super) global_piece_index: u32,
    pub(super) definition: &'a FossilPieceDefinition,
}

pub(super) fn find_global_fossil_piece_index_by_definition_identifier(
    definitions: WorldDefinitionsView<'_>,
    fossil_piece_identifier: AssetId,
) -> Option<u32> {
    definitions
        .fossil_pieces()
        .position(|piece| piece.id == fossil_piece_identifier)
        .and_then(|index| u32::try_from(index).ok())
}

pub(super) fn find_fossil_assembly_slot_identifier_by_authored_index(
    definitions: WorldDefinitionsView<'_>,
    authored_slot_index: u16,
) -> Option<AssetId> {
    definitions
        .fossil_slots()
        .nth(usize::from(authored_slot_index))
        .map(|slot| slot.id)
}

pub(super) fn find_fossil_piece_index_within_set(
    fossil_set_piece_identifiers: &[AssetId],
    fossil_piece_identifier: AssetId,
) -> Option<u16> {
    fossil_set_piece_identifiers
        .iter()
        .position(|candidate| candidate.0 == fossil_piece_identifier.0)
        .and_then(|index| u16::try_from(index).ok())
}

pub(super) fn select_weighted_uncollected_fossil_piece<'a>(
    definitions: WorldDefinitionsView<'a>,
    set_pieces: &[AssetId],
    collection: &CollectedFossilPieceInventory,
    weighted_selection_offset: u32,
) -> Option<SelectedUncollectedFossilPiece<'a>> {
    let total_uncollected_discovery_weight =
        calculate_total_uncollected_fossil_piece_discovery_weight(
            definitions,
            set_pieces,
            collection,
        )?;
    if weighted_selection_offset >= total_uncollected_discovery_weight {
        return None;
    }

    let mut remaining_selection_offset = weighted_selection_offset;
    for fossil_piece_identifier in set_pieces {
        let piece_id = *fossil_piece_identifier;
        let index = find_global_fossil_piece_index_by_definition_identifier(definitions, piece_id)?;
        let piece = definitions.find_fossil_piece(piece_id)?;
        if collection.contains_global_fossil_piece_index(index) {
            continue;
        }
        let weight = u32::from(piece.discovery_weight);
        if remaining_selection_offset < weight {
            return Some(SelectedUncollectedFossilPiece {
                global_piece_index: index,
                definition: piece,
            });
        }
        remaining_selection_offset -= weight;
    }
    None
}

pub(super) fn calculate_total_uncollected_fossil_piece_discovery_weight(
    definitions: WorldDefinitionsView<'_>,
    set_pieces: &[AssetId],
    collection: &CollectedFossilPieceInventory,
) -> Option<u32> {
    let mut total = 0_u32;
    for fossil_piece_identifier in set_pieces {
        let piece_id = *fossil_piece_identifier;
        let index = find_global_fossil_piece_index_by_definition_identifier(definitions, piece_id)?;
        let piece = definitions.find_fossil_piece(piece_id)?;
        if !collection.contains_global_fossil_piece_index(index) {
            total = total.checked_add(u32::from(piece.discovery_weight))?;
        }
    }
    if total == 0 {
        return None;
    }
    Some(total)
}

pub(super) fn are_all_fossil_set_pieces_collected(
    definitions: WorldDefinitionsView<'_>,
    set_pieces: &[AssetId],
    collection: &CollectedFossilPieceInventory,
) -> bool {
    !set_pieces.is_empty()
        && set_pieces.iter().all(|fossil_piece_identifier| {
            find_global_fossil_piece_index_by_definition_identifier(
                definitions,
                *fossil_piece_identifier,
            )
            .is_some_and(|index| collection.contains_global_fossil_piece_index(index))
        })
}
