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

use super::show_timing_operations::convert_authored_nanoseconds_to_fixed_simulation_ticks;

#[derive(Component)]
pub(super) struct ShowRunSlotOrder(pub(super) Box<[Entity]>);

#[derive(Component, Default)]
pub(super) struct ShowScheduleCursor(Option<u16>);

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
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
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
        commands.spawn((
            *world,
            RunningShowOnStage(stage),
            RunningShowFromRow(row),
            ShowRunSlotOrder(slots.map_or_else(Box::default, |slots| {
                slots.collection().clone().into_boxed_slice()
            })),
            RunningShow {
                phase,
                remaining_ticks,
                current_slot: 0,
                trick_score: 0.0,
                stars: 0.0,
                completed_tricks: 0,
                failed_tricks: 0,
            },
        ));
    }
}

pub(super) fn advance_running_stage_shows(
    mut shows: Query<(
        Entity,
        &RunningShowOnStage,
        &ShowRunSlotOrder,
        &mut RunningShow,
        Option<&ShowTrickBeingPerformedBy>,
        Has<RunningShowTrick>,
    )>,
    stages: Query<(Option<&ShowStageOpenState>, Option<&PersistentId>), With<ShowStage>>,
    slots: Query<(&ScheduledShowTrickPerformance, &ScheduledTrickPerformer)>,
    animals: Query<Option<&AnimalPerformingShowTrick>, (With<Animal>, Without<Dead>)>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut finished: MessageWriter<ShowRunFinished>,
    mut cancelled: MessageWriter<ShowRunCancelled>,
    mut started: MessageWriter<BeginScheduledShowTrick>,
    mut commands: Commands,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let Some(timing) = definitions.show_scheduling() else {
        return;
    };
    let mut releasing_runs = HashSet::new();
    let mut advancing_runs = Vec::new();
    for (entity, stage, _, mut show, performer, has_trick) in &mut shows {
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
            if performer.is_some() {
                releasing_runs.insert(entity);
            }
            commands.entity(entity).try_despawn();
            continue;
        }
        if show.remaining_ticks > 0 {
            show.remaining_ticks -= 1;
            if show.remaining_ticks > 0 {
                continue;
            }
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
            show.remaining_ticks = convert_authored_nanoseconds_to_fixed_simulation_ticks(
                timing.between_show_ns,
                definitions,
            );
            continue;
        }
        if performer.is_some() {
            releasing_runs.insert(entity);
            commands
                .entity(entity)
                .try_remove::<ShowTrickBeingPerformedBy>();
        }
        if has_trick {
            commands.entity(entity).try_remove::<RunningShowTrick>();
        }
        let stage_id = stage_facts
            .and_then(|(_, id)| id)
            .map_or(u64::MAX, |id| id.0);
        advancing_runs.push((stage_id, stage.0, entity));
    }

    advancing_runs.sort_unstable();
    let mut accepted_performers = HashSet::new();
    for (_, stage, entity) in advancing_runs {
        let Ok((_, _, slot_order, mut show, ..)) = shows.get_mut(entity) else {
            continue;
        };
        let mut claimed = false;
        while let Some(&slot) = slot_order.0.get(show.current_slot) {
            let Ok((trick, performer)) = slots.get(slot) else {
                show.current_slot += 1;
                continue;
            };
            let Ok(active) = animals.get(performer.0) else {
                show.current_slot += 1;
                continue;
            };
            if accepted_performers.contains(&performer.0)
                || active.is_some_and(|owner| !releasing_runs.contains(owner.collection()))
            {
                // Keep the slot at the head of the run until its performer is free.
                claimed = true;
                break;
            }
            accepted_performers.insert(performer.0);
            show.current_slot += 1;
            show.phase = ShowPhase::Conducting;
            show.remaining_ticks = trick.duration_ticks.max(1);
            commands.entity(entity).insert((
                ShowTrickBeingPerformedBy(performer.0),
                RunningShowTrick {
                    definition: trick.trick,
                    slot,
                    scored: false,
                },
            ));
            started.write(BeginScheduledShowTrick {
                show: entity,
                stage,
                performer: performer.0,
                slot,
                trick: trick.trick,
                duration_ticks: show.remaining_ticks,
            });
            claimed = true;
            break;
        }
        if !claimed {
            begin_postshow(&mut show, timing.post_show_ns, definitions);
        }
    }
}

fn begin_postshow(
    show: &mut RunningShow,
    postshow_ns: u64,
    definitions: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
) {
    show.phase = ShowPhase::Postshow;
    show.remaining_ticks =
        convert_authored_nanoseconds_to_fixed_simulation_ticks(postshow_ns, definitions);
}
