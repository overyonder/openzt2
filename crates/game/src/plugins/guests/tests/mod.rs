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

#[test]
fn entrance_displacement_follows_the_gate_and_stops_inside_the_map() {
    use super::guest_arrival_calculations::displaced_from_entrance_gate;
    use crate::plugins::locomotion::locomotion_types::SpatialGrid;
    use bevy::prelude::{Quat, Vec2, Vec3};

    // A 100 m by 50 m map spanning Bevy x 0..100 and z -50..0, in 2 m cells.
    let grid = SpatialGrid {
        origin: Vec2::new(0.0, -50.0),
        cell_size_m: 2.0,
        width: 50,
        height: 25,
        ..SpatialGrid::default()
    };
    let gate = Vec3::new(70.0, 3.0, -13.0);
    // guestMgr.xml: 100 m along the gate's source -y, which is Bevy +z, stops
    // half a cell inside the map edge.
    assert_eq!(
        displaced_from_entrance_gate(gate, Quat::IDENTITY, [0.0, -100.0], &grid),
        Some(Vec3::new(70.0, 3.0, -1.0))
    );
    // A gate turned a quarter turn sends the same displacement along -x.
    let turned = displaced_from_entrance_gate(
        gate,
        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        [0.0, -100.0],
        &grid,
    )
    .unwrap();
    assert!(
        (turned - Vec3::new(1.0, 3.0, -13.0)).length() < 1e-3,
        "{turned}"
    );
    // A displacement that stays on the map is used as authored.
    assert_eq!(
        displaced_from_entrance_gate(gate, Quat::IDENTITY, [-2.0, -10.0], &grid),
        Some(Vec3::new(68.0, 3.0, -3.0))
    );
    assert_eq!(
        displaced_from_entrance_gate(gate, Quat::IDENTITY, [0.0, -100.0], &SpatialGrid::default()),
        None
    );
}
