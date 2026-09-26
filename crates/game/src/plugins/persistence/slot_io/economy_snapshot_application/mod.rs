use bevy::prelude::{Commands, Entity, Query};

use crate::plugins::{
    economy::{
        facility_economy_types::{FacilityProfit, OperatingSinceDay},
        guest_admission_types::{AdmissionPrice, ZooAdmissionsOpen},
        money_types::Money,
        monthly_finance_types::MonthlyFinanceHistory,
        zoo_cash_types::ZooCash,
    },
    world_spawn::persistent_id_types::PersistentId,
};

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    economy_snapshot_types::EconomySnapshotRecords,
};

pub(super) fn apply_economy_snapshot_records_to_live_economy_resources_and_facilities(
    commands: &mut Commands,
    economy_snapshot_records: EconomySnapshotRecords,
    entities_with_persistent_identifiers: &Query<(Entity, &PersistentId)>,
    zoo_cash: &mut ZooCash,
    zoo_admission_price: &mut AdmissionPrice,
    zoo_admissions_open_state: &mut ZooAdmissionsOpen,
    monthly_finance_history: &mut MonthlyFinanceHistory,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    if !monthly_finance_history.restore_saved_monthly_finance_history(
        &economy_snapshot_records.monthly_finance_records,
        economy_snapshot_records.current_calendar_month,
        economy_snapshot_records.lifetime_income,
        economy_snapshot_records.lifetime_expenses,
        economy_snapshot_records.lifetime_donation_income,
        economy_snapshot_records.lifetime_users,
        economy_snapshot_records.monthly_finance_history_initialized,
    ) {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }

    zoo_cash.0 = economy_snapshot_records.zoo_cash;
    zoo_admission_price.0 = economy_snapshot_records.admission_price;
    zoo_admissions_open_state.0 = economy_snapshot_records.admissions_open;

    for facility_economy_snapshot_record in economy_snapshot_records.facility_economy_records {
        let facility_entity = entities_with_persistent_identifiers
            .iter()
            .find_map(|(entity, persistent_identifier)| {
                (*persistent_identifier == facility_economy_snapshot_record.persistent_identifier)
                    .then_some(entity)
            })
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;

        commands.entity(facility_entity).insert((
            OperatingSinceDay(facility_economy_snapshot_record.opening_zoo_day),
            FacilityProfit {
                total: Money(facility_economy_snapshot_record.total_profit_cents),
                transactions: facility_economy_snapshot_record.transaction_count,
            },
        ));
    }

    Ok(())
}
