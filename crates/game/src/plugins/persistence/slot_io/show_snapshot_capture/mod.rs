use crate::plugins::shows::{
    show_platform_upgrade_types::ShowPlatformUpgradeKind,
    show_schedule_types::ScheduledShowRowActivationState, show_stage_types::ShowStageOpenState,
};

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;
use super::{
    show_snapshot_types::{
        ScheduledShowBreakSnapshotRecord, ScheduledShowPerformanceSnapshotRecord,
        ScheduledShowSnapshotRecord, ShowStageSnapshotRecord,
    },
    world_snapshot_capture_system_parameters::WorldSnapshotCaptureQueries,
};

pub(super) fn capture_sorted_show_snapshot_records_from_live_world(
    world_snapshot_capture_queries: &WorldSnapshotCaptureQueries,
) -> Result<
    (
        Vec<ShowStageSnapshotRecord>,
        Vec<ScheduledShowSnapshotRecord>,
        Vec<ScheduledShowBreakSnapshotRecord>,
    ),
    WorldSnapshotPersistenceFailure,
> {
    let invalid_show_platform_upgrade_relationship = world_snapshot_capture_queries
        .installed_show_platform_upgrades
        .iter()
        .any(
            |(_upgrade_persistent_identifier, upgrade_kind, owning_show_stage)| {
                world_snapshot_capture_queries
                    .show_stages_with_snapshot_state
                    .get(owning_show_stage.0)
                    .is_err()
                    || world_snapshot_capture_queries
                        .installed_show_platform_upgrades
                        .iter()
                        .filter(|(_other_identifier, other_kind, other_owner)| {
                            other_owner.0 == owning_show_stage.0 && other_kind.0 == upgrade_kind.0
                        })
                        .count()
                        != 1
            },
        );
    if invalid_show_platform_upgrade_relationship {
        return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
    }

    let mut show_stage_snapshot_records = world_snapshot_capture_queries
        .show_stages_with_snapshot_state
        .iter()
        .map(
            |(
                show_stage_entity,
                persistent_identifier,
                optional_open_state,
                optional_show_name,
            )| ShowStageSnapshotRecord {
                persistent_identifier: *persistent_identifier,
                is_open: optional_open_state
                    .is_none_or(|open_state| *open_state == ShowStageOpenState::Open),
                display_name: optional_show_name
                    .map_or_else(String::new, |show_name| show_name.0.clone()),
                canopy_upgrade_persistent_identifier: world_snapshot_capture_queries
                    .installed_show_platform_upgrades
                    .iter()
                    .find_map(|(upgrade_identifier, upgrade_kind, owning_show_stage)| {
                        (owning_show_stage.0 == show_stage_entity
                            && upgrade_kind.0 == ShowPlatformUpgradeKind::Canopy)
                            .then_some(*upgrade_identifier)
                    }),
                television_upgrade_persistent_identifier: world_snapshot_capture_queries
                    .installed_show_platform_upgrades
                    .iter()
                    .find_map(|(upgrade_identifier, upgrade_kind, owning_show_stage)| {
                        (owning_show_stage.0 == show_stage_entity
                            && upgrade_kind.0 == ShowPlatformUpgradeKind::Television)
                            .then_some(*upgrade_identifier)
                    }),
            },
        )
        .collect::<Vec<_>>();
    show_stage_snapshot_records.sort_unstable_by_key(|record| record.persistent_identifier.0);

    let entity_persistent_identifier = |entity| {
        world_snapshot_capture_queries
            .entities_with_persistent_identifiers
            .get(entity)
            .ok()
            .copied()
    };
    let mut scheduled_show_snapshot_records = Vec::with_capacity(
        world_snapshot_capture_queries
            .scheduled_show_rows_with_snapshot_state
            .iter()
            .len(),
    );
    for (
        persistent_identifier,
        scheduled_show_row,
        schedule_row_order,
        stored_performance_plan,
        optional_activation_state,
    ) in &world_snapshot_capture_queries.scheduled_show_rows_with_snapshot_state
    {
        let performance_plan = stored_performance_plan;
        if !performance_plan.is_valid()
            || u16::try_from(performance_plan.trick_performances.len()).is_err()
        {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        let stage_persistent_identifier = entity_persistent_identifier(scheduled_show_row.stage)
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let mut performance_snapshot_records =
            Vec::with_capacity(performance_plan.trick_performances.len());
        for trick_performance in &performance_plan.trick_performances {
            let performer_persistent_identifier =
                entity_persistent_identifier(trick_performance.performer)
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
            performance_snapshot_records.push(ScheduledShowPerformanceSnapshotRecord {
                performer_persistent_identifier,
                trick_definition_identifier: trick_performance.trick,
                duration_simulation_ticks: trick_performance.duration_ticks,
            });
        }
        scheduled_show_snapshot_records.push(ScheduledShowSnapshotRecord {
            persistent_identifier: *persistent_identifier,
            stage_persistent_identifier,
            schedule_row_order: schedule_row_order.0,
            is_active: optional_activation_state.is_none_or(|activation_state| {
                *activation_state == ScheduledShowRowActivationState::Enabled
            }),
            performance_capacity: performance_plan.trick_performance_capacity,
            performance_records: performance_snapshot_records,
        });
    }
    let mut scheduled_show_break_snapshot_records = Vec::with_capacity(
        world_snapshot_capture_queries
            .scheduled_show_break_rows
            .iter()
            .len(),
    );
    for (persistent_identifier, scheduled_show_break, schedule_row_order) in
        &world_snapshot_capture_queries.scheduled_show_break_rows
    {
        let stage_persistent_identifier = entity_persistent_identifier(scheduled_show_break.stage)
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        scheduled_show_break_snapshot_records.push(ScheduledShowBreakSnapshotRecord {
            persistent_identifier: *persistent_identifier,
            stage_persistent_identifier,
            schedule_row_order: schedule_row_order.0,
            is_active: true,
            duration_simulation_ticks: scheduled_show_break.duration_ticks,
        });
    }

    scheduled_show_snapshot_records.sort_unstable_by_key(|record| record.persistent_identifier.0);
    scheduled_show_break_snapshot_records
        .sort_unstable_by_key(|record| record.persistent_identifier.0);
    Ok((
        show_stage_snapshot_records,
        scheduled_show_snapshot_records,
        scheduled_show_break_snapshot_records,
    ))
}
