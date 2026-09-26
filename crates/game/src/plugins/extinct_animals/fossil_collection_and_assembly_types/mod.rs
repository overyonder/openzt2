use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CollectedFossilPiece {
    pub(super) fossil_set_identifier: AssetId,
    pub(super) global_piece_index: u16,
}

/// The assembly slot occupied by this piece.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FossilPiecePlacedInAssemblySlot {
    pub(super) assembly_entity: Entity,
    pub(super) fossil_slot_identifier: AssetId,
}

/// Fossil-puzzle content roots for this world.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FossilPlacementScope {
    pub(super) puzzle_root: AssetId,
    pub(super) entity_root: AssetId,
}

/// Placement eligibility projected from the compiled fossil policy onto the
/// actual assembly-table entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FossilPlacementSurface {
    pub(super) accepts_fossil_pieces: bool,
}

#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct CollectedFossilPieceInventory {
    collected_piece_bit_words: Vec<u64>,
    global_piece_count: u32,
}

impl CollectedFossilPieceInventory {
    pub(super) fn with_global_fossil_piece_count(global_piece_count: u32) -> Self {
        Self {
            collected_piece_bit_words: vec![
                0;
                calculate_u64_word_count_for_bit_count(
                    global_piece_count
                )
            ],
            global_piece_count,
        }
    }

    pub(super) fn contains_global_fossil_piece_index(&self, global_piece_index: u32) -> bool {
        is_indexed_bit_set_in_bounded_u64_words(
            &self.collected_piece_bit_words,
            self.global_piece_count,
            global_piece_index,
        )
    }

    pub(super) fn is_uninitialized(&self) -> bool {
        self.global_piece_count == 0 && self.collected_piece_bit_words.is_empty()
    }

    pub(super) fn mark_global_fossil_piece_collected(
        &mut self,
        global_piece_index: u32,
    ) -> Option<bool> {
        set_indexed_bit_in_bounded_u64_words(
            &mut self.collected_piece_bit_words,
            self.global_piece_count,
            global_piece_index,
        )
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FossilAssemblyTable {
    pub(super) object_definition_identifier: AssetId,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct FossilSetAssembly {
    fossil_set_identifier: AssetId,
    placed_piece_bit_words: Vec<u64>,
    placed_piece_count: u16,
}

impl FossilSetAssembly {
    pub(super) fn new(fossil_set_identifier: AssetId, fossil_set_piece_count: u16) -> Self {
        Self {
            fossil_set_identifier,
            placed_piece_bit_words: vec![
                0;
                calculate_u64_word_count_for_bit_count(u32::from(
                    fossil_set_piece_count
                ))
            ],
            placed_piece_count: 0,
        }
    }

    pub(super) fn contains_set_local_fossil_piece_index(
        &self,
        set_local_piece_index: u16,
        fossil_set_piece_count: u16,
    ) -> bool {
        is_indexed_bit_set_in_bounded_u64_words(
            &self.placed_piece_bit_words,
            u32::from(fossil_set_piece_count),
            u32::from(set_local_piece_index),
        )
    }

    pub(crate) fn fossil_set_identifier(&self) -> AssetId {
        self.fossil_set_identifier
    }

    pub(crate) fn placed_fossil_piece_count(&self) -> u16 {
        self.placed_piece_count
    }

    pub(super) fn mark_set_local_fossil_piece_placed(
        &mut self,
        set_local_piece_index: u16,
        fossil_set_piece_count: u16,
    ) -> Option<bool> {
        let changed = set_indexed_bit_in_bounded_u64_words(
            &mut self.placed_piece_bit_words,
            u32::from(fossil_set_piece_count),
            u32::from(set_local_piece_index),
        )?;
        if changed {
            self.placed_piece_count = self.placed_piece_count.checked_add(1)?;
        }
        Some(changed)
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PlaceFossilPieceInAssemblyRequest {
    pub(super) assembly_entity: Entity,
    pub(super) global_piece_index: u16,
    pub(super) authored_slot_index: u16,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompletedFossilSetAssembly {
    pub(crate) assembly_entity: Entity,
    pub(crate) fossil_set_identifier: AssetId,
}

const fn calculate_u64_word_count_for_bit_count(bit_count: u32) -> usize {
    bit_count.div_ceil(u64::BITS) as usize
}

fn is_indexed_bit_set_in_bounded_u64_words(
    bit_words: &[u64],
    bit_count: u32,
    bit_index: u32,
) -> bool {
    if bit_index >= bit_count {
        return false;
    }
    bit_words
        .get(bit_index as usize / u64::BITS as usize)
        .is_some_and(|word| word & (1_u64 << (bit_index % u64::BITS)) != 0)
}

fn set_indexed_bit_in_bounded_u64_words(
    bit_words: &mut [u64],
    bit_count: u32,
    bit_index: u32,
) -> Option<bool> {
    if bit_index >= bit_count {
        return None;
    }
    let word = bit_words.get_mut(bit_index as usize / u64::BITS as usize)?;
    let mask = 1_u64 << (bit_index % u64::BITS);
    let changed = *word & mask == 0;
    *word |= mask;
    Some(changed)
}
