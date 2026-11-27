use bevy::prelude::*;
use openzt2_shows::shows::{
    show_timing_operations::RunningShowPhaseTimer, ActiveShowTrickOf, ScheduledTrickToken,
    ShowColumnWait, ShowMixerTimeSlot, ShowMixerWait, ShowRunSlotOrder, ShowScoreSamples,
    TrickTokenForShow,
};
use openzt2_zoo_state::animal_behavior::behavior_token_types::{
    BehaviorSelfToken, BehaviorTrickDestination, ClaimedBehaviorSelfToken,
};
use openzt2_zoo_state::shows::running_show_types::{
    RunningShow, RunningShowFromRow, RunningShowOnStage, RunningShowTrick,
    ShowTrickBeingPerformedBy,
};

use openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndex;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::shows::show_platform_upgrade_types::RestoreShowPlatformUpgrade;
use openzt2_zoo_state::shows::show_platform_upgrade_types::ShowPlatformUpgradeKind;
use openzt2_zoo_state::shows::show_schedule_types::ScheduledShowBreakRow;
use openzt2_zoo_state::shows::show_schedule_types::ScheduledShowPerformancePlan;
use openzt2_zoo_state::shows::show_schedule_types::ScheduledShowRow;
use openzt2_zoo_state::shows::show_schedule_types::ScheduledShowRowActivationState;
use openzt2_zoo_state::shows::show_schedule_types::ShowScheduleRowOrder;
use openzt2_zoo_state::shows::show_schedule_types::{
    ScheduledShowTrickPerformance, ScheduledTrickPerformer, ShowTrickSlotOf,
};
use openzt2_zoo_state::shows::show_stage_types::ShowName;
use openzt2_zoo_state::shows::show_stage_types::ShowStage;
use openzt2_zoo_state::shows::show_stage_types::ShowStageOpenState;

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    show_snapshot_types::ShowSnapshotRecords,
};

pub(super) fn apply_show_snapshot_records_to_live_show_stages_and_schedule(
    commands: &mut Commands,
    show_snapshot_records: ShowSnapshotRecords,
    world_root_entity: Entity,
    identities: &PersistentIdIndex,
    show_stages_with_mutable_snapshot_state: &mut Query<
        (
            Entity,
            &PersistentId,
            Option<&mut ShowStageOpenState>,
            Option<&mut ShowName>,
        ),
        With<ShowStage>,
    >,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
    show_platform_upgrade_restore_requests: &mut MessageWriter<RestoreShowPlatformUpgrade>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let mut running = std::collections::BTreeMap::new();
    for show_stage_snapshot_record in show_snapshot_records.stage_records {
        let show_stage_entity = identities
            .get(
                world_root_entity,
                show_stage_snapshot_record.persistent_identifier,
            )
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        if let Some(saved) = show_stage_snapshot_record.running {
            running.insert(saved.row_id, saved);
        }
        let (_, _, show_stage_open_state, show_name) = show_stages_with_mutable_snapshot_state
            .get_mut(show_stage_entity)
            .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;

        let restored_show_stage_open_state = if show_stage_snapshot_record.is_open {
            ShowStageOpenState::Open
        } else {
            ShowStageOpenState::Closed
        };
        if let Some(mut show_stage_open_state) = show_stage_open_state {
            *show_stage_open_state = restored_show_stage_open_state;
        } else {
            commands
                .entity(show_stage_entity)
                .insert(restored_show_stage_open_state);
        }

        if let Some(mut show_name) = show_name {
            show_name.0 = show_stage_snapshot_record.display_name;
        } else if !show_stage_snapshot_record.display_name.is_empty() {
            commands
                .entity(show_stage_entity)
                .insert(ShowName(show_stage_snapshot_record.display_name));
        }

        for (upgrade_persistent_identifier, upgrade_kind) in [
            (
                show_stage_snapshot_record.canopy_upgrade_persistent_identifier,
                ShowPlatformUpgradeKind::Canopy,
            ),
            (
                show_stage_snapshot_record.television_upgrade_persistent_identifier,
                ShowPlatformUpgradeKind::Television,
            ),
        ] {
            let Some(upgrade_persistent_identifier) = upgrade_persistent_identifier else {
                continue;
            };
            persistent_identifier_allocator
                .reserve_imported(upgrade_persistent_identifier)
                .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
            show_platform_upgrade_restore_requests.write(RestoreShowPlatformUpgrade {
                stage: show_stage_entity,
                kind: upgrade_kind,
                persistent_id: upgrade_persistent_identifier,
            });
        }
    }

    for scheduled_show_snapshot_record in show_snapshot_records.scheduled_show_records {
        if identities
            .get(
                world_root_entity,
                scheduled_show_snapshot_record.persistent_identifier,
            )
            .is_some()
        {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        let show_stage_entity = identities
            .get(
                world_root_entity,
                scheduled_show_snapshot_record.stage_persistent_identifier,
            )
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let scheduled_trick_performances = scheduled_show_snapshot_record
            .performance_records
            .iter()
            .map(|performance_snapshot_record| {
                identities
                    .get(
                        world_root_entity,
                        performance_snapshot_record.performer_persistent_identifier,
                    )
                    .map(|performer| {
                        (
                            ScheduledTrickPerformer(performer),
                            *performance_snapshot_record,
                        )
                    })
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)
            })
            .collect::<Result<Vec<_>, _>>()?;

        persistent_identifier_allocator
            .reserve_imported(scheduled_show_snapshot_record.persistent_identifier)
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        let row = commands
            .spawn((
                scheduled_show_snapshot_record.persistent_identifier,
                ScheduledShowRow {
                    stage: show_stage_entity,
                },
                ShowScheduleRowOrder(scheduled_show_snapshot_record.schedule_row_order),
                openzt2_zoo_state::shows::show_schedule_types::ScheduledShowRating(
                    scheduled_show_snapshot_record.rating,
                ),
                ScheduledShowPerformancePlan {
                    trick_performance_capacity: scheduled_show_snapshot_record.performance_capacity,
                },
                if scheduled_show_snapshot_record.is_active {
                    ScheduledShowRowActivationState::Enabled
                } else {
                    ScheduledShowRowActivationState::Disabled
                },
                WorldMember {
                    root: world_root_entity,
                },
            ))
            .id();
        let mut slots = Vec::with_capacity(scheduled_trick_performances.len());
        for (performer, cell) in scheduled_trick_performances {
            let mut slot = commands.spawn((ShowTrickSlotOf(row), performer));
            if let Some(time_slot) = cell.time_slot {
                slot.insert(ShowMixerTimeSlot(time_slot));
            }
            if let Some(trick) = cell.trick_definition_identifier {
                slot.insert(ScheduledShowTrickPerformance {
                    trick,
                    duration_ticks: cell.duration_simulation_ticks,
                });
            } else {
                slot.insert(ShowMixerWait(cell.duration_simulation_ticks));
            }
            slots.push(slot.id());
        }
        if let Some(saved) = running.remove(&scheduled_show_snapshot_record.persistent_identifier.0)
        {
            restore_show_phase(
                commands,
                row,
                show_stage_entity,
                world_root_entity,
                saved,
                &slots,
                identities,
            )?;
        }
    }

    for scheduled_break_snapshot_record in show_snapshot_records.scheduled_break_records {
        if identities
            .get(
                world_root_entity,
                scheduled_break_snapshot_record.persistent_identifier,
            )
            .is_some()
        {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        let show_stage_entity = identities
            .get(
                world_root_entity,
                scheduled_break_snapshot_record.stage_persistent_identifier,
            )
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        persistent_identifier_allocator
            .reserve_imported(scheduled_break_snapshot_record.persistent_identifier)
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        let row = commands
            .spawn((
                scheduled_break_snapshot_record.persistent_identifier,
                ScheduledShowRow {
                    stage: show_stage_entity,
                },
                ScheduledShowBreakRow {
                    duration_ticks: scheduled_break_snapshot_record.duration_simulation_ticks,
                },
                ShowScheduleRowOrder(scheduled_break_snapshot_record.schedule_row_order),
                WorldMember {
                    root: world_root_entity,
                },
            ))
            .id();
        if let Some(saved) =
            running.remove(&scheduled_break_snapshot_record.persistent_identifier.0)
        {
            restore_show_phase(
                commands,
                row,
                show_stage_entity,
                world_root_entity,
                saved,
                &[],
                identities,
            )?;
        }
    }

    if !running.is_empty() {
        return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
    }
    Ok(())
}

fn restore_show_phase(
    commands: &mut Commands,
    row: Entity,
    stage: Entity,
    root: Entity,
    saved: super::show_snapshot_types::RunningShowSnapshot,
    slots: &[Entity],
    identities: &PersistentIdIndex,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let phase = saved
        .phase()
        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    let mut show = commands.spawn((
        WorldMember { root },
        RunningShowOnStage(stage),
        RunningShowFromRow(row),
        RunningShow {
            phase,
            current_slot: saved.current_slot as usize,
            trick_score: saved.trick_score,
            stars: saved.stars,
            completed_tricks: saved.completed_tricks,
            failed_tricks: saved.failed_tricks,
        },
        RunningShowPhaseTimer::from_remaining(std::time::Duration::from_nanos(saved.remaining_ns)),
    ));
    let show_entity = show.id();
    if let Some(continuation) = saved.continuation {
        let slot = |index: u32| slots[index as usize];
        show.insert((
            ShowRunSlotOrder(
                continuation
                    .columns
                    .into_iter()
                    .map(|column| column.into_iter().map(slot).collect())
                    .collect(),
            ),
            ShowColumnWait::from_remaining(std::time::Duration::from_nanos(continuation.wait_ns)),
        ));
        if let Some((predicted, actual)) = continuation.scores {
            let samples = |rows: Vec<(u32, f32)>| {
                rows.into_iter()
                    .map(|(key, score)| (slot(key), score))
                    .collect()
            };
            show.insert(ShowScoreSamples {
                predicted: samples(predicted),
                actual: samples(actual),
            });
        }
        for saved in continuation.tricks {
            let performer = identities
                .get(root, PersistentId(saved.performer))
                .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
            let slot = slot(saved.slot);
            let mut trick = commands.spawn((
                ActiveShowTrickOf(show_entity),
                ShowTrickBeingPerformedBy(performer),
                RunningShowTrick {
                    definition: saved.definition,
                    slot,
                    scored: saved.scored,
                },
            ));
            if let Some(duration) = saved.pending_token_duration {
                trick.insert(openzt2_shows::shows::PendingShowTrickToken(duration));
            }
            if !saved.tokens.is_empty() {
                trick.insert(RestoredShowTrickTokens {
                    performer,
                    slot,
                    tokens: saved.tokens,
                });
            }
        }
    }
    Ok(())
}

#[derive(Component)]
pub(super) struct RestoredShowTrickTokens {
    performer: Entity,
    slot: Entity,
    tokens: Vec<super::show_snapshot_types::ShowTrickTokenSnapshot>,
}

pub(super) fn bind_restored_show_trick_tokens(
    mut commands: Commands,
    operations: Query<(
        Entity,
        &super::pending_snapshot_application_types::SnapshotRestored,
    )>,
    tricks: Query<(Entity, &RestoredShowTrickTokens, &ActiveShowTrickOf)>,
    shows: Query<&WorldMember, With<RunningShow>>,
    children: Query<&Children>,
    mut tokens: Query<
        (
            &mut BehaviorSelfToken,
            Option<&ClaimedBehaviorSelfToken>,
            Option<&mut BehaviorTrickDestination>,
        ),
        Without<TrickTokenForShow>,
    >,
    identities: Res<PersistentIdIndex>,
) {
    use super::pending_snapshot_application_types::{SnapshotRestoreFailed, SnapshotRestored};
    let Ok((operation, restored)) = operations.single() else {
        return;
    };
    let root = restored.0;
    let mut bound = bevy::ecs::entity::EntityHashSet::new();
    for (trick, pending, show) in &tricks {
        if !shows.get(show.0).is_ok_and(|member| member.root == root) {
            continue;
        }
        for saved in &pending.tokens {
            let token = children
                .get(pending.performer)
                .into_iter()
                .flat_map(|children| children.iter())
                .find(|&token| {
                    !bound.contains(&token)
                        && tokens.get(token).is_ok_and(|(token, claim, _)| {
                            token.name == saved.name
                                && token.available_at == saved.available_at
                                && token.expires_at == saved.expires_at
                                && claim.map(|claim| claim.execution_id) == saved.execution
                        })
                });
            let Some(token) = token else {
                commands
                    .entity(operation)
                    .remove::<SnapshotRestored>()
                    .insert(SnapshotRestoreFailed::DuringReplacement {
                        reason: WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference,
                        root_to_unload: Some(root),
                    });
                return;
            };
            let Ok((mut token_state, _, destination)) = tokens.get_mut(token) else {
                continue;
            };
            bound.insert(token);
            if let Some(payload) = saved
                .payload
                .and_then(|id| identities.get(root, PersistentId(id)))
            {
                token_state.payload = Some(payload);
            }
            let mut token = commands.entity(token);
            token.insert((
                TrickTokenForShow(trick),
                ScheduledTrickToken { slot: pending.slot },
            ));
            super::restored_component_values::restore_optional_component(
                &mut token,
                destination,
                saved
                    .destination
                    .map(|position| BehaviorTrickDestination(Vec3::from_array(position))),
                true,
            );
        }
        commands.entity(trick).remove::<RestoredShowTrickTokens>();
    }
}
