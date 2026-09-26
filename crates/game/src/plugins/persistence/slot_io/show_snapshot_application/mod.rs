use bevy::prelude::{Commands, Entity, MessageWriter, Query, With};

use crate::plugins::{
    shows::{
        show_platform_upgrade_types::{RestoreShowPlatformUpgrade, ShowPlatformUpgradeKind},
        show_schedule_types::{
            ScheduledShowBreakRow, ScheduledShowPerformancePlan, ScheduledShowRow,
            ScheduledShowRowActivationState, ScheduledShowTrickPerformance, ShowScheduleRowOrder,
        },
        show_stage_types::{ShowName, ShowStage, ShowStageOpenState},
    },
    world_spawn::{
        persistent_id_types::PersistentId, persistent_id_types::PersistentIdAllocator,
        world_membership_types::WorldMember,
    },
};

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    show_snapshot_types::ShowSnapshotRecords,
};

pub(super) fn apply_show_snapshot_records_to_live_show_stages_and_schedule(
    commands: &mut Commands,
    show_snapshot_records: ShowSnapshotRecords,
    world_root_entity: Entity,
    entities_with_persistent_identifiers: &Query<(Entity, &PersistentId)>,
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
    for show_stage_snapshot_record in show_snapshot_records.stage_records {
        let (show_stage_entity, _, show_stage_open_state, show_name) =
            show_stages_with_mutable_snapshot_state
                .iter_mut()
                .find(|(_, persistent_identifier, ..)| {
                    **persistent_identifier == show_stage_snapshot_record.persistent_identifier
                })
                .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;

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
                .reserve_imported(world_root_entity, upgrade_persistent_identifier)
                .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
            show_platform_upgrade_restore_requests.write(RestoreShowPlatformUpgrade {
                stage: show_stage_entity,
                kind: upgrade_kind,
                persistent_id: upgrade_persistent_identifier,
            });
        }
    }

    for scheduled_show_snapshot_record in show_snapshot_records.scheduled_show_records {
        if entities_with_persistent_identifiers
            .iter()
            .any(|(_, persistent_identifier)| {
                *persistent_identifier == scheduled_show_snapshot_record.persistent_identifier
            })
        {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }

        let show_stage_entity = entities_with_persistent_identifiers
            .iter()
            .find_map(|(entity, persistent_identifier)| {
                (*persistent_identifier
                    == scheduled_show_snapshot_record.stage_persistent_identifier)
                    .then_some(entity)
            })
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let scheduled_trick_performances = scheduled_show_snapshot_record
            .performance_records
            .iter()
            .map(|performance_snapshot_record| {
                entities_with_persistent_identifiers
                    .iter()
                    .find_map(|(entity, persistent_identifier)| {
                        (*persistent_identifier
                            == performance_snapshot_record.performer_persistent_identifier)
                            .then_some(entity)
                    })
                    .map(|performer_entity| ScheduledShowTrickPerformance {
                        performer: performer_entity,
                        trick: performance_snapshot_record.trick_definition_identifier,
                        duration_ticks: performance_snapshot_record.duration_simulation_ticks,
                    })
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)
            })
            .collect::<Result<Vec<_>, _>>()?;

        persistent_identifier_allocator
            .reserve_imported(
                world_root_entity,
                scheduled_show_snapshot_record.persistent_identifier,
            )
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        commands.spawn((
            scheduled_show_snapshot_record.persistent_identifier,
            ScheduledShowRow {
                stage: show_stage_entity,
            },
            ShowScheduleRowOrder(scheduled_show_snapshot_record.schedule_row_order),
            ScheduledShowPerformancePlan {
                trick_performances: scheduled_trick_performances,
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
        ));
    }

    for scheduled_break_snapshot_record in show_snapshot_records.scheduled_break_records {
        if entities_with_persistent_identifiers
            .iter()
            .any(|(_, persistent_identifier)| {
                *persistent_identifier == scheduled_break_snapshot_record.persistent_identifier
            })
        {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }

        let show_stage_entity = entities_with_persistent_identifiers
            .iter()
            .find_map(|(entity, persistent_identifier)| {
                (*persistent_identifier
                    == scheduled_break_snapshot_record.stage_persistent_identifier)
                    .then_some(entity)
            })
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        persistent_identifier_allocator
            .reserve_imported(
                world_root_entity,
                scheduled_break_snapshot_record.persistent_identifier,
            )
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        commands.spawn((
            scheduled_break_snapshot_record.persistent_identifier,
            ScheduledShowBreakRow {
                stage: show_stage_entity,
                duration_ticks: scheduled_break_snapshot_record.duration_simulation_ticks,
            },
            ShowScheduleRowOrder(scheduled_break_snapshot_record.schedule_row_order),
            WorldMember {
                root: world_root_entity,
            },
        ));
    }

    Ok(())
}
