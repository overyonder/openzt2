use bevy::{
    ecs::{entity::EntityHashSet, relationship::RelationshipTarget},
    prelude::*,
};

use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::Animal,
    animal_welfare::types::HabitatSuitability,
    habitat::habitat_types::{Containment, ContainmentChanged, HabitatMember, HabitatMembers},
};

use super::{
    aquatic_simulation_types::{
        AquaticAnimal, AquaticSuitability, AquaticWaterRequirement, LandWaterRequirement, Tank,
        TankCapacity, TankEnvironment, TankGeometry, TankWater, WaterQuality,
    },
    aquatic_suitability_and_placement_calculations::calculate_aquatic_suitability,
};

#[allow(clippy::type_complexity)]
pub(super) fn project_changed_tank_facts_into_live_aquatic_animal_suitability(
    changed_tanks: Query<
        &HabitatMembers,
        (
            With<Tank>,
            Or<(
                Added<Tank>,
                Changed<TankGeometry>,
                Changed<WaterQuality>,
                Changed<TankCapacity>,
                Changed<TankWater>,
                Changed<TankEnvironment>,
            )>,
        ),
    >,
    tanks: Query<
        (
            &TankGeometry,
            &WaterQuality,
            &TankCapacity,
            &TankWater,
            &TankEnvironment,
        ),
        With<Tank>,
    >,
    changed_animals: Query<
        Entity,
        (
            With<Animal>,
            With<AquaticAnimal>,
            Without<Dead>,
            Or<(
                Changed<AquaticAnimal>,
                Changed<AquaticWaterRequirement>,
                Changed<LandWaterRequirement>,
                Changed<HabitatMember>,
                Changed<Containment>,
                Added<AquaticSuitability>,
                Added<HabitatSuitability>,
            )>,
        ),
    >,
    mut containment_changes: MessageReader<ContainmentChanged>,
    mut removed_members: RemovedComponents<HabitatMember>,
    mut aquatic_animals: Query<
        (
            &AquaticAnimal,
            &AquaticWaterRequirement,
            &LandWaterRequirement,
            Option<&HabitatMember>,
            Option<&Containment>,
            &mut AquaticSuitability,
            &mut HabitatSuitability,
        ),
        (With<Animal>, Without<Dead>),
    >,
) {
    let affected: EntityHashSet = changed_animals
        .iter()
        .chain(changed_tanks.iter().flat_map(RelationshipTarget::iter))
        .chain(
            containment_changes
                .read()
                .map(|change| change.affected_entity),
        )
        .chain(removed_members.read())
        .collect();
    for entity in affected {
        let Ok((
            animal,
            water_requirement,
            land_requirement,
            member,
            containment,
            mut aquatic,
            mut habitat,
        )) = aquatic_animals.get_mut(entity)
        else {
            continue;
        };
        let tank = member
            .filter(|_| containment.is_some_and(|containment| containment.is_contained))
            .and_then(|member| tanks.get(member.habitat_entity).ok());
        let next = tank.map_or_else(
            AquaticSuitability::default,
            |(geometry, quality, capacity, water, environment)| {
                calculate_aquatic_suitability(
                    *animal,
                    *water_requirement,
                    *land_requirement,
                    *geometry,
                    *capacity,
                    *water,
                    *quality,
                    *environment,
                )
            },
        );
        aquatic.set_if_neq(next);
        habitat.set_if_neq(HabitatSuitability {
            space: next.depth.min(next.space),
            biome: next.water.min(next.land_water),
            overall: next.overall,
        });
    }
}
