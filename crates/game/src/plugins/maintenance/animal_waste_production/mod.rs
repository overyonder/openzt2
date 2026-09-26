use bevy::prelude::*;

use crate::plugins::{
    animal_lifecycle::types::{Animal, AnimalWasteCycle},
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::{
        persistent_id_types::PersistentIdAllocator, world_membership_types::DefinitionId,
        world_membership_types::WorldMember,
    },
};

use super::maintenance_types::{
    AnimalHabitatWaste, AnimalWasteProductionSchedule,
    AnimalWasteProductionScheduleResolutionMarker,
};

pub(super) fn initialize_animal_waste_production_schedule(
    mut commands: Commands,
    zoo_clock: Res<ZooClock>,
    animals_without_waste_production_schedule: Query<
        (Entity, &AnimalWasteCycle),
        (
            With<Animal>,
            Without<AnimalWasteProductionSchedule>,
            Without<AnimalWasteProductionScheduleResolutionMarker>,
        ),
    >,
) {
    for (animal_entity, animal_waste_cycle) in &animals_without_waste_production_schedule {
        let waste_production_interval_ticks = u64::from(animal_waste_cycle.interval_ticks);
        if waste_production_interval_ticks == 0 {
            commands
                .entity(animal_entity)
                .insert(AnimalWasteProductionScheduleResolutionMarker);
            continue;
        }
        let Some(next_waste_production_tick) =
            zoo_clock.tick.checked_add(waste_production_interval_ticks)
        else {
            continue;
        };
        commands.entity(animal_entity).insert((
            AnimalWasteProductionSchedule {
                next_production_tick: next_waste_production_tick,
            },
            AnimalWasteProductionScheduleResolutionMarker,
        ));
    }
}

pub(super) fn produce_due_animal_habitat_waste(
    mut commands: Commands,
    zoo_clock: Res<ZooClock>,
    mut persistent_identifier_allocator: ResMut<PersistentIdAllocator>,
    mut animals_with_waste_production_schedule: Query<
        (
            &AnimalWasteCycle,
            &GlobalTransform,
            &WorldMember,
            &mut AnimalWasteProductionSchedule,
        ),
        With<Animal>,
    >,
) {
    for (animal_waste_cycle, animal_global_transform, world_member, mut waste_production) in
        &mut animals_with_waste_production_schedule
    {
        if zoo_clock.tick < waste_production.next_production_tick {
            continue;
        }
        let waste_production_interval_ticks = u64::from(animal_waste_cycle.interval_ticks);
        let produced_waste_units = animal_waste_cycle.units;
        if waste_production_interval_ticks == 0 || produced_waste_units == 0 {
            continue;
        }
        let Some(next_waste_production_tick) =
            zoo_clock.tick.checked_add(waste_production_interval_ticks)
        else {
            continue;
        };
        let Ok(persistent_identifier) = persistent_identifier_allocator.allocate(world_member.root)
        else {
            continue;
        };
        commands.spawn((
            AnimalHabitatWaste {
                uncontained_waste_units: produced_waste_units,
            },
            DefinitionId(animal_waste_cycle.definition),
            Transform::from_translation(animal_global_transform.translation()),
            *world_member,
            persistent_identifier,
        ));
        waste_production.next_production_tick = next_waste_production_tick;
    }
}
