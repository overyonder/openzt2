use bevy::{
    ecs::{
        entity::EntityHashSet, entity_disabling::Disabled, query::Allow,
        relationship::RelationshipTarget,
    },
    prelude::*,
};

use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::Animal,
    habitat::habitat_types::{Containment, HabitatMember, HabitatMembers},
};

use super::aquatic_simulation_types::{AquaticAnimal, Tank, TankCapacity, TankPopulation};

#[derive(Message)]
pub(super) struct TankResidentRequirementsDiscarded(Entity);

pub(super) fn invalidate_discarded_tank_resident_requirements(
    discarded: On<Discard<(AquaticAnimal, Animal)>>,
    members: Query<&HabitatMember>,
    mut affected: MessageWriter<TankResidentRequirementsDiscarded>,
) {
    if let Ok(member) = members.get(discarded.entity) {
        affected.write(TankResidentRequirementsDiscarded(member.habitat_entity));
    }
}

pub(super) fn invalidate_disabled_tank_resident(
    added: On<Add<Disabled>>,
    residents: Query<&HabitatMember, (With<AquaticAnimal>, Allow<Disabled>)>,
    mut affected: MessageWriter<TankResidentRequirementsDiscarded>,
) {
    if let Ok(member) = residents.get(added.entity) {
        affected.write(TankResidentRequirementsDiscarded(member.habitat_entity));
    }
}

pub(super) fn invalidate_reenabled_tank_resident(
    removed: On<Remove<Disabled>>,
    residents: Query<&HabitatMember, (With<AquaticAnimal>, Allow<Disabled>)>,
    mut affected: MessageWriter<TankResidentRequirementsDiscarded>,
) {
    if let Ok(member) = residents.get(removed.entity) {
        affected.write(TankResidentRequirementsDiscarded(member.habitat_entity));
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn recompute_changed_tank_capacity(
    changed_tanks: Query<Entity, (With<Tank>, Or<(Added<Tank>, Changed<HabitatMembers>)>)>,
    changed_residents: Query<
        &HabitatMember,
        (
            With<Animal>,
            With<AquaticAnimal>,
            Or<(
                Changed<Containment>,
                Changed<AquaticAnimal>,
                Added<Animal>,
                Added<Dead>,
            )>,
        ),
    >,
    mut removed_memberships: RemovedComponents<HabitatMembers>,
    mut removed_requirements: MessageReader<TankResidentRequirementsDiscarded>,
    aquatic_animals: Query<&AquaticAnimal, (With<Animal>, Without<Dead>)>,
    containment: Query<&Containment>,
    mut tanks: Query<
        (
            Option<&HabitatMembers>,
            &mut TankCapacity,
            &mut TankPopulation,
        ),
        With<Tank>,
    >,
) {
    let affected: EntityHashSet = changed_tanks
        .iter()
        .chain(changed_residents.iter().map(|member| member.habitat_entity))
        .chain(removed_memberships.read())
        .chain(removed_requirements.read().map(|removed| removed.0))
        .collect();
    for tank in affected {
        let Ok((members, mut capacity, mut population)) = tanks.get_mut(tank) else {
            continue;
        };
        let mut residents: Vec<_> = members
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .filter_map(|member| {
                containment.get(member).ok()?.is_contained.then_some(())?;
                Some((member, aquatic_animals.get(member).ok()?))
            })
            .collect();
        residents.sort_unstable_by_key(|(entity, _)| *entity);
        let mut used = 0.0;
        for (index, (_, animal)) in residents.iter().enumerate() {
            let space = if index == 0 {
                animal.initial_space
            } else {
                animal.additional_space
            };
            used += space.max(0.0);
        }
        population.set_if_neq(TankPopulation(residents.len() as u32));
        capacity.set_if_neq(TankCapacity {
            used,
            required: used,
        });
    }
}
