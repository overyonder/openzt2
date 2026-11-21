use openzt2_game_data::world_definitions::guest_simulation_definitions::{
    GuestMemoryKind, MemoryReplacement,
};
use openzt2_game_data::world_definitions::GUEST_MEMORY_CAPACITY;
use std::mem::size_of;

use bevy::prelude::{Entity, Quat, Vec3};

use super::{
    guest_simulation_calculations::{apply_q16, push_memory},
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

#[test]
fn entrance_displacement_preserves_authored_distance_and_gate_rotation() {
    use super::guest_arrival_calculations::displaced_from_entrance_gate;

    let gate = Vec3::new(70.0, 3.0, -13.0);
    assert_eq!(
        displaced_from_entrance_gate(gate, Quat::IDENTITY, [0.0, -100.0]),
        Vec3::new(70.0, 3.0, 87.0)
    );
    let turned = displaced_from_entrance_gate(
        gate,
        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        [0.0, -100.0],
    );
    assert!(
        (turned - Vec3::new(-30.0, 3.0, -13.0)).length() < 1e-3,
        "{turned}"
    );
}
