use bevy::prelude::*;

use crate::plugins::{
    animal_health::types::Dead,
    animal_lifecycle::types::Animal,
    habitat::habitat_types::{ContainmentChanged, HabitatRegion},
};

use super::aquatic_simulation_types::{AquaticHome, Tank};

pub(super) fn invalidate_aquatic_homes_after_tank_or_habitat_region_removal(
    mut removed_tanks: RemovedComponents<Tank>,
    mut removed_regions: RemovedComponents<HabitatRegion>,
    live_tanks: Query<(), With<Tank>>,
    aquatic_animals: Query<(Entity, &AquaticHome), With<Animal>>,
    mut commands: Commands,
    mut containment_changes: MessageWriter<ContainmentChanged>,
) {
    for removed_tank in removed_tanks.read() {
        invalidate_aquatic_home_for_removed_tank(
            removed_tank,
            &aquatic_animals,
            &mut commands,
            &mut containment_changes,
        );
    }
    for removed_region in removed_regions.read() {
        if live_tanks.get(removed_region).is_err() {
            continue;
        }
        invalidate_aquatic_home_for_removed_tank(
            removed_region,
            &aquatic_animals,
            &mut commands,
            &mut containment_changes,
        );
    }
}

pub(super) fn invalidate_terminal_aquatic_animal_homes(
    aquatic_animals: Query<(Entity, &AquaticHome), (With<Animal>, Added<Dead>)>,
    mut containment_changes: MessageWriter<ContainmentChanged>,
) {
    for (aquatic_animal, home) in &aquatic_animals {
        containment_changes.write(ContainmentChanged {
            affected_entity: aquatic_animal,
            habitat_entity: Some(home.0),
            is_contained: false,
        });
    }
}

fn invalidate_aquatic_home_for_removed_tank(
    removed_tank: Entity,
    aquatic_animals: &Query<(Entity, &AquaticHome), With<Animal>>,
    commands: &mut Commands,
    containment_changes: &mut MessageWriter<ContainmentChanged>,
) {
    for (aquatic_animal, home) in aquatic_animals.iter() {
        if home.0 != removed_tank {
            continue;
        }
        commands.entity(aquatic_animal).remove::<AquaticHome>();
        containment_changes.write(ContainmentChanged {
            affected_entity: aquatic_animal,
            habitat_entity: None,
            is_contained: false,
        });
    }
}
