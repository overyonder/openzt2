use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow, relationship::RelationshipTarget},
    prelude::*,
};

use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::Animal,
    habitat::habitat_types::{
        Containment, ContainmentChanged, HabitatMember, HabitatMembers, HabitatRegion,
    },
};

use super::aquatic_simulation_types::{AquaticAnimal, Tank};

pub(super) fn notify_residents_of_tank_demotion(
    removed: On<Remove<(Tank, HabitatRegion)>>,
    tanks: Query<&HabitatMembers, (With<Tank>, Allow<Disabled>)>,
    residents: Query<&Containment, (With<Animal>, With<AquaticAnimal>, Without<Dead>)>,
    mut changes: MessageWriter<ContainmentChanged>,
) {
    let Ok(members) = tanks.get(removed.entity) else {
        return;
    };
    for entity in members.iter() {
        if residents
            .get(entity)
            .is_ok_and(|containment| containment.is_contained)
        {
            changes.write(ContainmentChanged {
                affected_entity: entity,
                habitat_entity: None,
                is_contained: false,
            });
        }
    }
}

pub(super) fn notify_terminal_aquatic_animal_departures(
    animals: Query<
        (Entity, &HabitatMember, &Containment),
        (With<Animal>, With<AquaticAnimal>, Added<Dead>),
    >,
    tanks: Query<(), With<Tank>>,
    mut changes: MessageWriter<ContainmentChanged>,
) {
    for (entity, member, containment) in &animals {
        if containment.is_contained && tanks.contains(member.habitat_entity) {
            changes.write(ContainmentChanged {
                affected_entity: entity,
                habitat_entity: Some(member.habitat_entity),
                is_contained: false,
            });
        }
    }
}
