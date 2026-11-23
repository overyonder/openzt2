use bevy::prelude::*;

use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::simulation_time::simulation_control_queries::simulation_is_running;
use openzt2_zoo_state::animal_health::types::Dead;

use super::{
    GamePhase,
    animal_age_resolution::resolve_animal_life_stage_from_elapsed_age_ticks,
    types::{Age, Animal, AnimalLifeStage, AuthoredAnimalLifeStageStartTicks},
};

use std::collections::BTreeSet;

use bevy::ecs::entity::EntityHashSet;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

#[derive(Component)]
struct RecheckAnimalLifeStage;

#[derive(Component, PartialEq, Eq)]
struct NextAnimalLifeStageTick {
    root: Entity,
    tick: u64,
}

#[derive(Component, Default)]
struct AnimalLifeStageDeadlines {
    due: BTreeSet<(u64, Entity)>,
    resolved_at: Option<u64>,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ResolveAnimalLifeStages;

type AnimalLifeStageInputs = (
    Animal,
    Age,
    AuthoredAnimalLifeStageStartTicks,
    AnimalLifeStage,
    Dead,
    WorldMember,
);

pub(super) fn register_animal_ageing(app: &mut App) {
    app.register_required_components::<WorldRoot, AnimalLifeStageDeadlines>()
        .add_observer(
            |change: On<Insert<AnimalLifeStageInputs>>,
             animals: Query<(), With<Animal>>,
             mut commands: Commands| {
                if animals.contains(change.entity) {
                    commands
                        .entity(change.entity)
                        .try_insert(RecheckAnimalLifeStage);
                }
            },
        )
        .add_observer(
            |change: On<Remove<AnimalLifeStageInputs>>,
             animals: Query<(), With<Animal>>,
             mut commands: Commands| {
                if animals.contains(change.entity) {
                    commands
                        .entity(change.entity)
                        .try_insert(RecheckAnimalLifeStage);
                }
            },
        )
        .add_observer(forget_removed_animal_life_stage_deadline)
        .add_systems(
            FixedUpdate,
            resolve_due_animal_life_stages
                .in_set(FixedGameSet::Think)
                .in_set(ResolveAnimalLifeStages)
                .run_if(in_state(GamePhase::InGame))
                .run_if(simulation_is_running),
        );
}

fn forget_removed_animal_life_stage_deadline(
    removed: On<Remove<NextAnimalLifeStageTick>>,
    scheduled: Query<&NextAnimalLifeStageTick>,
    mut worlds: Query<&mut AnimalLifeStageDeadlines>,
) {
    if let Ok(deadline) = scheduled.get(removed.entity) {
        if let Ok(mut deadlines) = worlds.get_mut(deadline.root) {
            deadlines.due.remove(&(deadline.tick, removed.entity));
        }
    }
}

fn resolve_due_animal_life_stages(
    mut worlds: Query<(Entity, &ZooClock, &mut AnimalLifeStageDeadlines)>,
    clocks: Query<&ZooClock, With<WorldRoot>>,
    mut stages: ParamSet<(
        Query<
            Entity,
            Or<(
                With<RecheckAnimalLifeStage>,
                (
                    With<Animal>,
                    Or<(
                        Changed<Age>,
                        Changed<AuthoredAnimalLifeStageStartTicks>,
                        Changed<AnimalLifeStage>,
                        Changed<WorldMember>,
                    )>,
                ),
            )>,
        >,
        Query<
            (
                &WorldMember,
                &AuthoredAnimalLifeStageStartTicks,
                &Age,
                &mut AnimalLifeStage,
                Option<&mut NextAnimalLifeStageTick>,
            ),
            (With<Animal>, Without<Dead>),
        >,
    )>,
    animals: Query<(Entity, &WorldMember), With<Animal>>,
    existing_deadlines: Query<(), With<NextAnimalLifeStageTick>>,
    rechecks: Query<(), With<RecheckAnimalLifeStage>>,
    mut commands: Commands,
) {
    let mut affected = EntityHashSet::default();
    affected.extend(stages.p0().iter());
    for (root, clock, mut deadlines) in &mut worlds {
        if deadlines
            .resolved_at
            .is_some_and(|previous| previous > clock.tick)
        {
            deadlines.due.clear();
            affected.extend(
                animals
                    .iter()
                    .filter_map(|(entity, member)| (member.root == root).then_some(entity)),
            );
        }
        while deadlines
            .due
            .first()
            .is_some_and(|&(tick, _)| tick <= clock.tick)
        {
            if let Some((_, animal)) = deadlines.due.pop_first() {
                affected.insert(animal);
            }
        }
        if deadlines.resolved_at != Some(clock.tick) {
            deadlines.resolved_at = Some(clock.tick);
        }
    }

    let mut live = stages.p1();
    for entity in affected {
        if rechecks.contains(entity) {
            commands
                .entity(entity)
                .try_remove::<RecheckAnimalLifeStage>();
        }
        let Ok((member, history, age, mut stage, deadline)) = live.get_mut(entity) else {
            if existing_deadlines.contains(entity) {
                commands
                    .entity(entity)
                    .try_remove::<NextAnimalLifeStageTick>();
            }
            continue;
        };
        let Ok(clock) = clocks.get(member.root) else {
            if deadline.is_some() {
                commands
                    .entity(entity)
                    .try_remove::<NextAnimalLifeStageTick>();
            }
            continue;
        };
        let boundaries = history.stage_start_ticks;
        let next_tick = if boundaries[2] == 0 {
            None
        } else {
            let elapsed = age.ticks_at(clock.tick);
            let resolved = resolve_animal_life_stage_from_elapsed_age_ticks(elapsed, boundaries);
            stage.set_if_neq(AnimalLifeStage(resolved));
            boundaries[1..]
                .iter()
                .copied()
                .filter_map(|elapsed| {
                    let boundary = age.born_tick.checked_add(elapsed)?;
                    (boundary > clock.tick
                        && resolve_animal_life_stage_from_elapsed_age_ticks(elapsed, boundaries)
                            != resolved)
                        .then_some(boundary)
                })
                .min()
        };
        if let Some(ref deadline) = deadline {
            if let Ok((_, _, mut previous)) = worlds.get_mut(deadline.root) {
                previous.due.remove(&(deadline.tick, entity));
            }
        }
        if let Some(tick) = next_tick {
            let next = NextAnimalLifeStageTick {
                root: member.root,
                tick,
            };
            if let Ok((_, _, mut deadlines)) = worlds.get_mut(member.root) {
                deadlines.due.insert((tick, entity));
            }
            if let Some(mut deadline) = deadline {
                deadline.set_if_neq(next);
            } else {
                commands.entity(entity).insert(next);
            }
        } else if deadline.is_some() {
            commands.entity(entity).remove::<NextAnimalLifeStageTick>();
        }
    }
}
