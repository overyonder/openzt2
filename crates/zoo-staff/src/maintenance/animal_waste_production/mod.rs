use std::time::Duration;

use bevy::prelude::*;

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::AnimalWasteCycle;

use super::maintenance_types::{AnimalHabitatWaste, AnimalWasteProductionSchedule};

// The serialized deadline allows a loaded cycle to resume. Runtime timing
// belongs to this component, including retries after an allocation failure.
#[derive(Component)]
pub(super) struct AnimalWasteTimer {
    timer: Timer,
    sampled_at_tick: u64,
}

fn duration_for_ticks(step: Duration, ticks: u64) -> Duration {
    let nanos = step.as_nanos().saturating_mul(u128::from(ticks));
    u64::try_from(nanos / 1_000_000_000).map_or(Duration::MAX, |seconds| {
        Duration::new(seconds, (nanos % 1_000_000_000) as u32)
    })
}

#[allow(clippy::type_complexity)]
pub(super) fn bind_animal_waste_timers(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    zoo_clock: Single<&ZooClock>,
    mut removed: RemovedComponents<AnimalWasteCycle>,
    without_cycle: Query<
        (),
        (
            Or<(With<AnimalWasteProductionSchedule>, With<AnimalWasteTimer>)>,
            Without<AnimalWasteCycle>,
        ),
    >,
    changed: Query<Entity, (With<Animal>, Or<(Added<Animal>, Changed<AnimalWasteCycle>)>)>,
    waiting: Query<
        Entity,
        (
            With<Animal>,
            With<AnimalWasteProductionSchedule>,
            Without<AnimalWasteTimer>,
        ),
    >,
    cycles: Query<
        (
            &AnimalWasteCycle,
            Option<&AnimalWasteProductionSchedule>,
            Has<AnimalWasteTimer>,
        ),
        With<Animal>,
    >,
) {
    for entity in removed
        .read()
        .filter(|&entity| without_cycle.contains(entity))
    {
        commands
            .entity(entity)
            .remove::<(AnimalWasteTimer, AnimalWasteProductionSchedule)>();
    }
    let mut visited = bevy::ecs::entity::EntityHashSet::default();
    for entity in changed
        .iter()
        .chain(waiting.iter())
        .filter(|entity| visited.insert(*entity))
    {
        let Ok((cycle, saved, active)) = cycles.get(entity) else {
            continue;
        };
        if cycle.interval_ticks == 0 || cycle.units == 0 {
            if active || saved.is_some() {
                commands
                    .entity(entity)
                    .remove::<(AnimalWasteTimer, AnimalWasteProductionSchedule)>();
            }
            continue;
        }
        if active {
            continue;
        }
        let Some(deadline) = saved
            .map(|saved| saved.next_production_tick)
            .or_else(|| zoo_clock.tick.checked_add(u64::from(cycle.interval_ticks)))
        else {
            continue;
        };
        let remaining = deadline.saturating_sub(zoo_clock.tick);
        commands
            .entity(entity)
            .insert(AnimalWasteTimer {
                timer: Timer::new(
                    duration_for_ticks(time.timestep(), remaining),
                    TimerMode::Once,
                ),
                sampled_at_tick: zoo_clock.tick,
            })
            .insert_if_new(AnimalWasteProductionSchedule {
                next_production_tick: deadline,
            });
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn produce_due_animal_habitat_waste(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    zoo_clock: Single<&ZooClock>,
    mut persistent_identifiers: Query<&mut PersistentIdAllocator>,
    mut animals: Query<
        (
            &AnimalWasteCycle,
            &GlobalTransform,
            &WorldMember,
            &mut AnimalWasteTimer,
            &mut AnimalWasteProductionSchedule,
        ),
        With<Animal>,
    >,
) {
    let step = time.timestep();
    for (cycle, transform, member, mut production, mut saved) in &mut animals {
        let elapsed = zoo_clock.tick.saturating_sub(production.sampled_at_tick);
        production.sampled_at_tick = zoo_clock.tick;
        if !production
            .timer
            .tick(duration_for_ticks(step, elapsed))
            .is_finished()
        {
            continue;
        }
        let interval = u64::from(cycle.interval_ticks);
        let Some(deadline) = zoo_clock.tick.checked_add(interval) else {
            continue;
        };
        let Some(identifier) = persistent_identifiers
            .get_mut(member.root)
            .ok()
            .and_then(|mut allocator| allocator.allocate().ok())
        else {
            continue;
        };
        commands.spawn((
            AnimalHabitatWaste {
                uncontained_waste_units: cycle.units,
            },
            DefinitionId(cycle.definition),
            Transform::from_translation(transform.translation()),
            *member,
            identifier,
        ));
        production
            .timer
            .set_duration(duration_for_ticks(step, interval));
        production.timer.reset();
        saved.next_production_tick = deadline;
    }
}
