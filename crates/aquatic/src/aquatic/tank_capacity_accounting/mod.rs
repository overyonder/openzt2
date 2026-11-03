use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::habitat::habitat_types::ContainmentChanged;

use super::aquatic_simulation_types::{
    AquaticAnimal, AquaticHome, Tank, TankCapacity, TankPopulation,
};

pub(super) fn update_aquatic_homes_from_changed_containment(
    mut containment_changes: MessageReader<ContainmentChanged>,
    mut aquatic_animals: Query<Option<&mut AquaticHome>, (With<AquaticAnimal>, With<Animal>)>,
    tanks: Query<(), With<Tank>>,
    mut commands: Commands,
) {
    for containment_change in containment_changes.read() {
        let Ok(current_home) = aquatic_animals.get_mut(containment_change.affected_entity) else {
            continue;
        };
        let requested_tank = containment_change
            .is_contained
            .then_some(containment_change.habitat_entity)
            .flatten()
            .filter(|tank| tanks.contains(*tank));
        match (requested_tank, current_home) {
            (Some(tank), Some(mut home)) => home.0 = tank,
            (Some(tank), None) => {
                commands
                    .entity(containment_change.affected_entity)
                    .insert(AquaticHome(tank));
            }
            (None, Some(_)) => {
                commands
                    .entity(containment_change.affected_entity)
                    .remove::<AquaticHome>();
            }
            (None, None) => {}
        }
    }
}

/// Homes own the count, so an animal leaving by any route, such as release or
/// a tank's demotion, can't leave its space behind.
pub(super) fn recompute_tank_capacity_from_aquatic_homes(
    aquatic_animals: Query<(Entity, &AquaticAnimal, &AquaticHome), With<Animal>>,
    mut tanks: Query<(Entity, &mut TankCapacity, &mut TankPopulation), With<Tank>>,
) {
    let mut residents: Vec<_> = aquatic_animals.iter().collect();
    residents.sort_unstable_by_key(|(entity, ..)| *entity);
    let mut occupancy = HashMap::<Entity, (u32, f32)>::default();
    for (_, aquatic_animal, home) in residents {
        let (population, used) = occupancy.entry(home.0).or_default();
        let space = if *population == 0 {
            aquatic_animal.initial_space
        } else {
            aquatic_animal.additional_space
        };
        *population += 1;
        *used += space.max(0.0);
    }
    for (tank, mut capacity, mut population) in &mut tanks {
        let (occupants, used) = occupancy.get(&tank).copied().unwrap_or_default();
        population.set_if_neq(TankPopulation(occupants));
        capacity.set_if_neq(TankCapacity {
            used,
            required: used,
        });
    }
}
