use bevy::prelude::*;

use crate::plugins::{
    animal_lifecycle::types::Animal,
    habitat::habitat_types::{Containment, ContainmentChanged},
    locomotion::locomotion_types::Arrived,
    placement::placement_preview_types::RelocatingPlacedObject,
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_entity_crating::WorldEntityIsCrated,
};

use super::types::{Dead, Escaped, Rampaging, Tranquilized};

pub(super) fn detect_animal_containment_breaches_and_mark_newly_escaped_animals(
    mut commands: Commands,
    clock: Res<ZooClock>,
    mut containment_changes: MessageReader<ContainmentChanged>,
    animals: Query<Option<&Escaped>, (With<Animal>, Without<Dead>)>,
) {
    for containment_change in containment_changes.read() {
        if containment_change.is_contained {
            continue;
        }
        let Ok(existing_escape_state) = animals.get(containment_change.affected_entity) else {
            continue;
        };
        if existing_escape_state.is_none() {
            commands
                .entity(containment_change.affected_entity)
                .insert(Escaped {
                    since_tick: clock.tick,
                });
        }
    }
}

/// Crating is a containment outcome, so a crated animal cannot remain escaped.
pub(super) fn clear_animal_escape_state_after_crating(
    mut commands: Commands,
    animals: Query<Entity, (With<Animal>, With<Escaped>, Added<WorldEntityIsCrated>)>,
) {
    for animal in &animals {
        commands.entity(animal).remove::<Escaped>();
    }
}

/// Selecting an animal for ordinary relocation returns it to player control.
pub(super) fn clear_animal_escape_state_after_player_relocation(
    mut commands: Commands,
    animals: Query<Entity, (With<Animal>, With<Escaped>, Added<RelocatingPlacedObject>)>,
) {
    for animal in &animals {
        commands.entity(animal).remove::<Escaped>();
    }
}

pub(super) fn clear_escape_and_rampage_after_tranquilized_animal_reaches_containment(
    mut commands: Commands,
    mut arrivals: MessageReader<Arrived>,
    animals: Query<
        &Containment,
        (
            With<Animal>,
            With<Escaped>,
            With<Tranquilized>,
            Without<Dead>,
        ),
    >,
) {
    for arrival in arrivals.read() {
        let Ok(containment) = animals.get(arrival.entity) else {
            continue;
        };
        if !containment.is_contained {
            continue;
        }
        commands
            .entity(arrival.entity)
            .remove::<(Escaped, Rampaging, Tranquilized)>();
    }
}
