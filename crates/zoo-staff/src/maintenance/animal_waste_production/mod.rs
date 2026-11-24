use bevy::prelude::*;

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::AnimalWasteCycle;

use super::maintenance_types::{AnimalHabitatWaste, AnimalWasteProductionSchedule};

pub(super) fn synchronize_animal_waste_production_deadlines(
    mut commands: Commands,
    zoo_clock: Single<&ZooClock>,
    changed_cycles: Query<
        (
            Entity,
            &AnimalWasteCycle,
            Has<AnimalWasteProductionSchedule>,
        ),
        (With<Animal>, Or<(Added<Animal>, Changed<AnimalWasteCycle>)>),
    >,
    mut removed_cycles: RemovedComponents<AnimalWasteCycle>,
    animals_without_cycle: Query<
        (),
        (
            With<AnimalWasteProductionSchedule>,
            Without<AnimalWasteCycle>,
        ),
    >,
) {
    for animal in removed_cycles
        .read()
        .filter(|&entity| animals_without_cycle.contains(entity))
    {
        commands
            .entity(animal)
            .remove::<AnimalWasteProductionSchedule>();
    }
    for (animal, cycle, scheduled) in &changed_cycles {
        if cycle.interval_ticks == 0 || cycle.units == 0 {
            if scheduled {
                commands
                    .entity(animal)
                    .remove::<AnimalWasteProductionSchedule>();
            }
            continue;
        }
        if scheduled {
            continue;
        }
        let Some(next_production_tick) =
            zoo_clock.tick.checked_add(u64::from(cycle.interval_ticks))
        else {
            continue;
        };
        commands
            .entity(animal)
            .insert(AnimalWasteProductionSchedule {
                next_production_tick,
            });
    }
}

pub(super) fn produce_due_animal_habitat_waste(
    mut commands: Commands,
    zoo_clock: Single<&ZooClock>,
    mut persistent_identifiers: Query<&mut PersistentIdAllocator>,
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
        let Some(persistent_identifier) = persistent_identifiers
            .get_mut(world_member.root)
            .ok()
            .and_then(|mut allocator| allocator.allocate().ok())
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
