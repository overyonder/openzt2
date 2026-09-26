use openzt2_game_data::world_definitions::guest_simulation_definitions::{
    GuestMemoryKind, MemoryReplacement,
};
use openzt2_game_data::world_definitions::GUEST_MEMORY_CAPACITY;
use std::mem::size_of;

use bevy::prelude::Entity;

use super::{
    guest_simulation_calculations::{
        apply_q16, push_memory, survey_rate_permille, survey_view_score_permille,
    },
    guest_simulation_types::{GuestMemories, GuestMemoryEntry},
};

fn entry(subject: Entity, kind: GuestMemoryKind, value: i16) -> GuestMemoryEntry {
    GuestMemoryEntry {
        subject,
        kind,
        value_permille: value,
        age_ticks: 0,
    }
}

#[test]
fn need_decay_preserves_q16_residual_and_clamps() {
    let mut value = 1_000;
    let mut residual = 0;
    apply_q16(&mut value, &mut residual, -1);
    assert_eq!((value, residual), (999, u16::MAX));
    apply_q16(&mut value, &mut residual, 1);
    assert_eq!((value, residual), (1_000, 0));
    apply_q16(&mut value, &mut residual, i32::MIN);
    assert_eq!((value, residual), (0, 0));
}

#[test]
fn survey_view_score_uses_typed_fixed_unit_inputs() {
    assert_eq!(survey_rate_permille(5, 10), 500);
    assert_eq!(survey_rate_permille(20, 10), 1_000);
    assert_eq!(survey_rate_permille(1, 0), 0);
    assert_eq!(survey_view_score_permille(1_000, 500, 1_000, true), 916);
    assert_eq!(survey_view_score_permille(1_000, 500, 1_000, false), 583);
}

#[test]
fn memory_is_inline_bounded_and_replaces_by_evidence_policy() {
    let mut memories = GuestMemories::new(2);
    push_memory(
        &mut memories,
        entry(Entity::from_bits(1), GuestMemoryKind::AnimalView, 100),
        MemoryReplacement::Oldest,
    );
    push_memory(
        &mut memories,
        entry(Entity::from_bits(2), GuestMemoryKind::Facility, 5),
        MemoryReplacement::Oldest,
    );
    push_memory(
        &mut memories,
        entry(Entity::from_bits(3), GuestMemoryKind::Litter, -80),
        MemoryReplacement::LowestAbsoluteValue,
    );
    assert_eq!(memories.entries.len(), 2);
    assert!(memories
        .entries
        .iter()
        .any(|item| item.subject.to_bits() == 1));
    assert!(memories
        .entries
        .iter()
        .any(|item| item.subject.to_bits() == 3));
    assert_eq!(memories.entries.capacity(), GUEST_MEMORY_CAPACITY);
    assert!(size_of::<GuestMemories>() >= GUEST_MEMORY_CAPACITY * size_of::<GuestMemoryEntry>());
}
