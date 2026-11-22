use bevy::prelude::*;

use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::simulation_time::simulation_control_queries::simulation_is_running;
use openzt2_zoo_state::animal_health::types::Dead;

use super::{
    animal_age_resolution::resolve_animal_life_stage_from_elapsed_age_ticks,
    types::{Age, Animal, AnimalLifeStage, AuthoredAnimalLifeStageStartTicks},
    GamePhase,
};

#[derive(Component)]
struct RecheckAnimalLifeStage;

#[derive(Component)]
struct NextAnimalLifeStageTick(u64);

#[derive(Resource, Default)]
struct AnimalLifeStageDeadlines {
    earliest: Option<u64>,
    resolved_at: Option<u64>,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ResolveAnimalLifeStages;

pub(super) fn register_animal_ageing(app: &mut App) {
    app.init_resource::<AnimalLifeStageDeadlines>()
        .add_observer(
            |change: On<
                Insert<(
                    Animal,
                    Age,
                    AuthoredAnimalLifeStageStartTicks,
                    AnimalLifeStage,
                    Dead,
                )>,
            >,
             mut commands: Commands| {
                commands
                    .entity(change.entity)
                    .try_insert(RecheckAnimalLifeStage);
            },
        )
        .add_observer(
            |change: On<
                Remove<(
                    Animal,
                    Age,
                    AuthoredAnimalLifeStageStartTicks,
                    AnimalLifeStage,
                    Dead,
                )>,
            >,
             mut commands: Commands| {
                commands
                    .entity(change.entity)
                    .try_insert(RecheckAnimalLifeStage);
            },
        )
        .add_systems(
            FixedUpdate,
            resolve_due_animal_life_stages
                .in_set(FixedGameSet::Think)
                .in_set(ResolveAnimalLifeStages)
                .run_if(in_state(GamePhase::InGame))
                .run_if(simulation_is_running)
                .run_if(animal_life_stage_needs_update),
        );
}

fn animal_life_stage_needs_update(
    clock: Single<&ZooClock>,
    deadlines: Res<AnimalLifeStageDeadlines>,
    rechecks: Query<(), With<RecheckAnimalLifeStage>>,
    mut removed: RemovedComponents<NextAnimalLifeStageTick>,
) -> bool {
    let deadline_removed = removed.read().count() != 0;
    deadline_removed
        || !rechecks.is_empty()
        || deadlines.earliest.is_some_and(|tick| tick <= clock.tick)
        || deadlines.resolved_at.is_some_and(|tick| tick > clock.tick)
}

fn resolve_due_animal_life_stages(
    clock: Single<&ZooClock>,
    mut deadlines: ResMut<AnimalLifeStageDeadlines>,
    scheduled: Query<Entity, Or<(With<NextAnimalLifeStageTick>, With<RecheckAnimalLifeStage>)>>,
    all_animals: Query<Entity, With<Animal>>,
    mut stages: Query<
        (
            &AuthoredAnimalLifeStageStartTicks,
            &Age,
            &mut AnimalLifeStage,
            Option<&mut NextAnimalLifeStageTick>,
            Has<RecheckAnimalLifeStage>,
        ),
        (With<Animal>, Without<Dead>),
    >,
    mut commands: Commands,
) {
    let tick = clock.tick;
    let rewound = deadlines
        .resolved_at
        .is_some_and(|resolved| resolved > tick);
    let mut earliest = None;
    let entities = rewound
        .then(|| all_animals.iter())
        .into_iter()
        .flatten()
        .chain((!rewound).then(|| scheduled.iter()).into_iter().flatten());
    for entity in entities {
        let Ok((history, age, mut stage, deadline, recheck)) = stages.get_mut(entity) else {
            commands
                .entity(entity)
                .try_remove::<(NextAnimalLifeStageTick, RecheckAnimalLifeStage)>();
            continue;
        };
        let next_tick = if !rewound
            && !recheck
            && deadline.as_ref().is_some_and(|next| next.0 > tick)
        {
            deadline.as_ref().map(|next| next.0)
        } else if history.stage_start_ticks[2] == 0 {
            None
        } else {
            let boundaries = history.stage_start_ticks;
            let resolved =
                resolve_animal_life_stage_from_elapsed_age_ticks(age.ticks_at(tick), boundaries);
            stage.set_if_neq(AnimalLifeStage(resolved));
            boundaries[1..]
                .iter()
                .copied()
                .filter_map(|elapsed| {
                    let boundary = age.born_tick.checked_add(elapsed)?;
                    (boundary > tick
                        && resolve_animal_life_stage_from_elapsed_age_ticks(elapsed, boundaries)
                            != resolved)
                        .then_some(boundary)
                })
                .min()
        };
        if recheck {
            commands.entity(entity).remove::<RecheckAnimalLifeStage>();
        }
        match (deadline, next_tick) {
            (Some(mut current), Some(next)) if current.0 != next => current.0 = next,
            (None, Some(next)) => {
                commands
                    .entity(entity)
                    .insert(NextAnimalLifeStageTick(next));
            }
            (Some(_), None) => {
                commands.entity(entity).remove::<NextAnimalLifeStageTick>();
            }
            _ => {}
        }
        if let Some(next) = next_tick {
            earliest = Some(earliest.map_or(next, |previous: u64| previous.min(next)));
        }
    }
    deadlines.earliest = earliest;
    deadlines.resolved_at = Some(tick);
}
