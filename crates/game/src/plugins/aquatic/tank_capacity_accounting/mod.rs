use bevy::prelude::*;

use crate::plugins::{animal_lifecycle::types::Animal, habitat::habitat_types::ContainmentChanged};

use super::aquatic_simulation_types::{
    AquaticAnimal, AquaticHome, Tank, TankCapacity, TankPopulation,
};

pub(super) fn update_tank_capacity_from_changed_aquatic_animal_containment(
    mut containment_changes: MessageReader<ContainmentChanged>,
    mut aquatic_animals: Query<(&AquaticAnimal, Option<&mut AquaticHome>), With<Animal>>,
    mut tanks: Query<(&mut TankCapacity, &mut TankPopulation), With<Tank>>,
    mut commands: Commands,
) {
    for containment_change in containment_changes.read() {
        let Ok((aquatic_animal, current_home)) =
            aquatic_animals.get_mut(containment_change.affected_entity)
        else {
            continue;
        };
        let previous_tank = current_home.as_ref().map(|home| home.0);
        let requested_tank = containment_change
            .is_contained
            .then_some(containment_change.habitat_entity)
            .flatten();
        if previous_tank == requested_tank {
            continue;
        }
        if let Some(previous_tank) = previous_tank {
            if let Ok((mut capacity, mut population)) = tanks.get_mut(previous_tank) {
                let removed_space = if population.0 <= 1 {
                    aquatic_animal.initial_space
                } else {
                    aquatic_animal.additional_space
                };
                population.0 = population.0.saturating_sub(1);
                capacity.used = (capacity.used - removed_space.max(0.0)).max(0.0);
                capacity.required = capacity.used;
            }
        }
        let Some(requested_tank) = requested_tank else {
            commands
                .entity(containment_change.affected_entity)
                .remove::<AquaticHome>();
            continue;
        };
        let Ok((mut capacity, mut population)) = tanks.get_mut(requested_tank) else {
            commands
                .entity(containment_change.affected_entity)
                .remove::<AquaticHome>();
            continue;
        };
        let added_space = if population.0 == 0 {
            aquatic_animal.initial_space
        } else {
            aquatic_animal.additional_space
        };
        population.0 = population.0.saturating_add(1);
        capacity.used += added_space.max(0.0);
        capacity.required = capacity.used;
        if let Some(mut current_home) = current_home {
            current_home.0 = requested_tank;
        } else {
            commands
                .entity(containment_change.affected_entity)
                .insert(AquaticHome(requested_tank));
        }
    }
}
