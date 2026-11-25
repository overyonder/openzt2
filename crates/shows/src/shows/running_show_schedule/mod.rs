use bevy::{ecs::relationship::RelationshipTarget, platform::collections::HashSet, prelude::*};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId, world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::Animal,
    shows::{
        running_show_types::{
            AnimalPerformingShowTrick, BeginScheduledShowTrick, RunningShow, RunningShowFromRow,
            RunningShowOnStage, RunningShowTrick, ShowPhase, ShowRunFinished,
            ShowTrickBeingPerformedBy, StageRunningShow,
        },
        show_schedule_types::{
            ScheduledShowBreakRow, ScheduledShowRowActivationState, ScheduledShowTrickPerformance,
            ScheduledTrickPerformer, ShowScheduleRowOrder, ShowStageRows, ShowTrickSlots,
        },
        show_stage_types::{ShowStage, ShowStageOpenState},
    },
};

use super::show_timing_operations::{
    RunningShowPhaseTimer, convert_authored_nanoseconds_to_fixed_simulation_ticks,
};
use super::{ShowMixerTimeSlot, ShowMixerWait, scheduled_trick_execution::ShowTrickTokens};

#[derive(Component)]
pub(super) struct ShowRunSlotOrder(Vec<Vec<Entity>>);

impl ShowRunSlotOrder {
    pub(super) fn slots(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0.iter().flatten().copied()
    }
}

#[derive(Component)]
#[relationship(relationship_target = ActiveShowTricks)]
pub(super) struct ActiveShowTrickOf(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = ActiveShowTrickOf, linked_spawn)]
pub(super) struct ActiveShowTricks(Vec<Entity>);

#[derive(Component)]
pub(super) struct ShowColumnWait(Timer);

#[derive(Component, Default)]
pub(super) struct ShowScheduleCursor(Option<u16>);

#[derive(Message)]
pub(super) struct ShowPhaseStarted {
    pub(super) show: Entity,
    pub(super) stage: Entity,
    pub(super) phase: ShowPhase,
}

#[derive(Message)]
pub(super) struct ShowRunCancelled {
    pub(super) show: Entity,
    pub(super) stage: Entity,
}

pub(super) fn initialize_show_schedule_cursors(
    stages: Query<Entity, (With<ShowStage>, Without<ShowScheduleCursor>)>,
    mut commands: Commands,
) {
    for stage in &stages {
        commands.entity(stage).insert(ShowScheduleCursor::default());
    }
}

pub(super) fn start_next_stage_show_or_break(
    mut stages: Query<
        (
            Entity,
            &WorldMember,
            &ShowStageRows,
            &mut ShowScheduleCursor,
            Option<&ShowStageOpenState>,
        ),
        (With<ShowStage>, Without<StageRunningShow>),
    >,
    rows: Query<(
        &ShowScheduleRowOrder,
        Option<&ScheduledShowRowActivationState>,
        Option<&ScheduledShowBreakRow>,
        Option<&ShowTrickSlots>,
    )>,
    time_slots: Query<&ShowMixerTimeSlot>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    time: Res<Time<Fixed>>,
    mut phases: MessageWriter<ShowPhaseStarted>,
    mut commands: Commands,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let Some(timing) = definitions.show_scheduling() else {
        return;
    };
    for (stage, world, schedule, mut position, open) in &mut stages {
        if open == Some(&ShowStageOpenState::Closed) {
            continue;
        }
        let Some((row, order, interval, slots)) = schedule
            .iter()
            .filter_map(|row| {
                let (order, activation, interval, slots) = rows.get(row).ok()?;
                (activation != Some(&ScheduledShowRowActivationState::Disabled)
                    && (interval.is_some() || slots.is_some_and(|slots| !slots.is_empty())))
                .then_some((row, order.0, interval, slots))
            })
            .min_by_key(|(_, order, ..)| {
                (
                    position.0.is_some_and(|previous| *order <= previous),
                    *order,
                )
            })
        else {
            continue;
        };
        position.0 = Some(order);
        let (phase, remaining_ticks) = interval.map_or_else(
            || {
                (
                    ShowPhase::Preshow,
                    convert_authored_nanoseconds_to_fixed_simulation_ticks(
                        timing.pre_show_ns,
                        definitions,
                    ),
                )
            },
            |interval| (ShowPhase::BetweenShows, interval.duration_ticks),
        );
        let mut columns = BTreeMap::<u16, Vec<Entity>>::new();
        for (index, slot) in slots
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .enumerate()
        {
            let time = time_slots.get(slot).map_or(index as u16, |slot| slot.0);
            columns.entry(time).or_default().push(slot);
        }
        let show = commands
            .spawn((
                *world,
                RunningShowOnStage(stage),
                RunningShowFromRow(row),
                ShowRunSlotOrder(columns.into_values().collect()),
                RunningShowPhaseTimer::new(time.timestep(), remaining_ticks),
                ShowColumnWait(Timer::new(std::time::Duration::ZERO, TimerMode::Once)),
                RunningShow {
                    phase,
                    remaining_ticks: 0,
                    current_slot: 0,
                    trick_score: 0.0,
                    stars: 0.0,
                    completed_tricks: 0,
                    failed_tricks: 0,
                },
            ))
            .id();
        if phase == ShowPhase::Preshow {
            phases.write(ShowPhaseStarted { show, stage, phase });
        }
    }
}

pub(super) fn advance_running_stage_shows(
    mut shows: Query<(
        Entity,
        &RunningShowOnStage,
        &ShowRunSlotOrder,
        &mut RunningShowPhaseTimer,
        &mut RunningShow,
        Option<&ActiveShowTricks>,
        &mut ShowColumnWait,
    )>,
    stages: Query<(Option<&ShowStageOpenState>, Option<&PersistentId>), With<ShowStage>>,
    slots: Query<(&ScheduledShowTrickPerformance, &ScheduledTrickPerformer)>,
    waits: Query<&ShowMixerWait>,
    tokens: Query<&ShowTrickTokens>,
    animals: Query<Option<&AnimalPerformingShowTrick>, (With<Animal>, Without<Dead>)>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut finished: MessageWriter<ShowRunFinished>,
    mut cancelled: MessageWriter<ShowRunCancelled>,
    mut started: MessageWriter<BeginScheduledShowTrick>,
    time: Res<Time<Fixed>>,
    mut phases: MessageWriter<ShowPhaseStarted>,
    mut commands: Commands,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let Some(timing) = definitions.show_scheduling() else {
        return;
    };
    let mut releasing_actions = HashSet::new();
    let mut advancing_runs = Vec::new();
    for (entity, stage, _, mut timer, mut show, active, mut wait) in &mut shows {
        let stage_facts = stages.get(stage.0).ok();
        let stage_is_open =
            stage_facts.is_some_and(|(open, _)| open != Some(&ShowStageOpenState::Closed));
        if !stage_is_open {
            if show.phase != ShowPhase::BetweenShows {
                cancelled.write(ShowRunCancelled {
                    show: entity,
                    stage: stage.0,
                });
            }
            releasing_actions.extend(active.into_iter().flat_map(RelationshipTarget::iter));
            commands.entity(entity).try_despawn();
            continue;
        }
        let expired = timer.0.is_finished() || timer.0.tick(time.timestep()).is_finished();
        let column_finished = show.phase == ShowPhase::Conducting
            && wait.0.tick(time.timestep()).is_finished()
            && !tokens
                .iter_many(active.into_iter().flat_map(RelationshipTarget::iter))
                .flatten()
                .any(|tokens| !tokens.is_empty());
        if !expired && !column_finished {
            continue;
        }
        if show.phase == ShowPhase::BetweenShows {
            commands.entity(entity).try_despawn();
            continue;
        }
        if show.phase == ShowPhase::Postshow {
            finished.write(ShowRunFinished {
                show: entity,
                stage: stage.0,
            });
            show.phase = ShowPhase::BetweenShows;
            phases.write(ShowPhaseStarted {
                show: entity,
                stage: stage.0,
                phase: show.phase,
            });
            *timer = RunningShowPhaseTimer::new(
                time.timestep(),
                convert_authored_nanoseconds_to_fixed_simulation_ticks(
                    timing.between_show_ns,
                    definitions,
                ),
            );
            continue;
        }
        for action in active.into_iter().flat_map(RelationshipTarget::iter) {
            releasing_actions.insert(action);
            commands.entity(action).try_despawn();
        }
        let stage_id = stage_facts
            .and_then(|(_, id)| id)
            .map_or(u64::MAX, |id| id.0);
        advancing_runs.push((stage_id, stage.0, entity));
    }

    advancing_runs.sort_unstable();
    let mut accepted_performers = HashSet::new();
    for (_, stage, entity) in advancing_runs {
        let Ok((_, _, slot_order, mut timer, mut show, _, mut column_wait)) = shows.get_mut(entity)
        else {
            continue;
        };
        let mut claimed = false;
        while let Some(column) = slot_order.0.get(show.current_slot) {
            let tricks: Vec<_> = column
                .iter()
                .filter_map(|&slot| {
                    let (trick, performer) = slots.get(slot).ok()?;
                    let active = animals.get(performer.0).ok()?;
                    Some((slot, trick, performer.0, active))
                })
                .collect();
            if tricks.iter().any(|(_, _, performer, active)| {
                accepted_performers.contains(performer)
                    || active.is_some_and(|owner| !releasing_actions.contains(owner.collection()))
            }) {
                claimed = true;
                break;
            }
            show.current_slot += 1;
            let wait = waits
                .iter_many(column.iter().copied())
                .flatten()
                .map(|wait| wait.0)
                .max();
            if tricks.is_empty() && wait.is_none() {
                continue;
            }
            if show.phase != ShowPhase::Conducting {
                show.phase = ShowPhase::Conducting;
                phases.write(ShowPhaseStarted {
                    show: entity,
                    stage,
                    phase: show.phase,
                });
            }
            let mut duration = wait.unwrap_or_default();
            column_wait.0 = Timer::new(time.timestep().saturating_mul(duration), TimerMode::Once);
            for (slot, trick, performer, _) in tricks {
                if !accepted_performers.insert(performer) {
                    continue;
                }
                let duration_ticks = trick.duration_ticks.max(1);
                duration = duration.max(duration_ticks);
                commands.spawn((
                    ActiveShowTrickOf(entity),
                    ShowTrickBeingPerformedBy(performer),
                    RunningShowTrick {
                        definition: trick.trick,
                        slot,
                        scored: false,
                    },
                ));
                started.write(BeginScheduledShowTrick {
                    show: entity,
                    stage,
                    performer,
                    slot,
                    trick: trick.trick,
                    duration_ticks,
                });
            }
            *timer = RunningShowPhaseTimer::new(time.timestep(), duration);
            claimed = true;
            break;
        }
        if !claimed {
            show.phase = ShowPhase::Postshow;
            phases.write(ShowPhaseStarted {
                show: entity,
                stage,
                phase: show.phase,
            });
            *timer = RunningShowPhaseTimer::new(
                time.timestep(),
                convert_authored_nanoseconds_to_fixed_simulation_ticks(
                    timing.post_show_ns,
                    definitions,
                ),
            );
        }
    }
}
use std::collections::BTreeMap;
