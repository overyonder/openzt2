use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
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
    )>,
    stages: Query<Option<&ShowStageOpenState>, With<ShowStage>>,
    slots: Query<(&ScheduledShowTrickPerformance, &ScheduledTrickPerformer)>,
    animals: Query<Option<&AnimalPerformingShowTrick>, With<Animal>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut finished: MessageWriter<ShowRunFinished>,
    mut commands: Commands,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let Some(timing) = definitions.show_scheduling() else {
        return;
    };
    for (entity, stage, slot_order, mut show, performer) in &mut shows {
        if !stages
            .get(stage.0)
            .is_ok_and(|open| open != Some(&ShowStageOpenState::Closed))
        {
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
        show.phase = ShowPhase::Conducting;
        if performer.is_some() {
            commands
                .entity(entity)
                .remove::<(ShowTrickBeingPerformedBy, RunningShowTrick)>();
        }
        let mut scheduled = None;
        while let Some(&slot) = slot_order.0.get(show.current_slot) {
            show.current_slot += 1;
            if let Ok((trick, performer)) = slots.get(slot) {
                if animals
                    .get(performer.0)
                    .is_ok_and(|active| active.is_none_or(|active| active.collection() == &entity))
                {
                    scheduled = Some((slot, trick, performer.0));
                    break;
                }
            }
        }
        if let Some((slot, trick, performer)) = scheduled {
            show.remaining_ticks = trick.duration_ticks.max(1);
            let request = BeginScheduledShowTrick {
                show: entity,
                stage: stage.0,
                performer,
                slot,
                trick: trick.trick,
                duration_ticks: show.remaining_ticks,
            };
            commands.queue(move |world: &mut World| {
                if world.get::<Animal>(performer).is_none()
                    || world
                        .get::<AnimalPerformingShowTrick>(performer)
                        .is_some_and(|active| !active.is_empty())
                    || world.get::<RunningShow>(entity).is_none()
                {
                    return;
                }
                world.entity_mut(entity).insert((
                    ShowTrickBeingPerformedBy(performer),
                    RunningShowTrick {
                        definition: request.trick,
                        slot: request.slot,
                        scored: false,
                    },
                ));
                world.write_message(request);
            });
        } else {
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
