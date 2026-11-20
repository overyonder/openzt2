use bevy::{
    ecs::{relationship::RelationshipTarget, system::SystemParam},
    prelude::*,
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::shows::show_schedule_types::{
    ScheduledShowBreakRow, ScheduledShowPerformancePlan, ScheduledShowRow,
    ScheduledShowTrickPerformance, ScheduledTrickPerformer, ShowScheduleRowOrder, ShowTrickSlots,
};

use openzt2_zoo_state::shows::show_platform_upgrade_types::ShowPlatformUpgradeKind;
use openzt2_zoo_state::shows::show_schedule_types::ScheduledShowRowActivationState;
use openzt2_zoo_state::shows::show_stage_types::ShowStageOpenState;

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;
use super::{
    show_snapshot_types::{
        ScheduledShowBreakSnapshotRecord, ScheduledShowPerformanceSnapshotRecord,
        ScheduledShowSnapshotRecord, ShowStageSnapshotRecord,
    },
    world_snapshot_capture_system_parameters::WorldSnapshotCaptureQueries,
};

#[derive(SystemParam)]
pub(super) struct ShowScheduleSnapshotQueries<'w, 's> {
    rows: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static ScheduledShowRow,
            &'static ShowScheduleRowOrder,
            Option<&'static ScheduledShowBreakRow>,
            Option<&'static ScheduledShowPerformancePlan>,
            Option<&'static ScheduledShowRowActivationState>,
            Option<&'static ShowTrickSlots>,
        ),
    >,
    slots: Query<
        'w,
        's,
        (
            &'static ScheduledShowTrickPerformance,
            &'static ScheduledTrickPerformer,
        ),
    >,
}

pub(super) fn capture_sorted_show_snapshot_records_from_live_world(
    world_snapshot_capture_queries: &WorldSnapshotCaptureQueries,
    schedule: &ShowScheduleSnapshotQueries,
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
    let mut scheduled_show_snapshot_records = Vec::new();
    let mut scheduled_show_break_snapshot_records = Vec::new();
    for (id, row, order, interval, plan, activation, slots) in &schedule.rows {
        let stage_persistent_identifier = entity_persistent_identifier(row.stage)
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        if !world_snapshot_capture_queries
            .show_stages_with_snapshot_state
            .contains(row.stage)
        {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        if let Some(interval) = interval {
            scheduled_show_break_snapshot_records.push(ScheduledShowBreakSnapshotRecord {
                persistent_identifier: *id,
                stage_persistent_identifier,
                schedule_row_order: order.0,
                is_active: true,
                duration_simulation_ticks: interval.duration_ticks,
            });
            continue;
        }
        let Some(plan) = plan else {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        };
        let slot_count = slots.map_or(0, RelationshipTarget::len);
        if slot_count > usize::from(plan.trick_performance_capacity) {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        let mut performance_records = Vec::with_capacity(slot_count);
        for slot in slots.into_iter().flat_map(RelationshipTarget::iter) {
            let (performance, performer) = schedule
                .slots
                .get(slot)
                .ok()
                .filter(|(performance, _)| performance.duration_ticks > 0)
                .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
            let performer_persistent_identifier = entity_persistent_identifier(performer.0)
                .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
            performance_records.push(ScheduledShowPerformanceSnapshotRecord {
                performer_persistent_identifier,
                trick_definition_identifier: performance.trick,
                duration_simulation_ticks: performance.duration_ticks,
            });
        }
        scheduled_show_snapshot_records.push(ScheduledShowSnapshotRecord {
            persistent_identifier: *id,
            stage_persistent_identifier,
            schedule_row_order: order.0,
            is_active: activation
                .is_none_or(|activation| *activation == ScheduledShowRowActivationState::Enabled),
            performance_capacity: plan.trick_performance_capacity,
            performance_records,
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
