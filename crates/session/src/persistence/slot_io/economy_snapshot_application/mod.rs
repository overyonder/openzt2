use bevy::prelude::{Commands, Entity};

use openzt2_zoo_state::economy::guest_admission_types::AdmissionPrice;
use openzt2_zoo_state::economy::guest_admission_types::ZooAdmissionsOpen;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory;
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    economy_snapshot_types::EconomySnapshotRecords,
};

pub(super) fn apply_economy_snapshot_records_to_live_economy_resources_and_facilities(
    commands: &mut Commands,
    economy_snapshot_records: EconomySnapshotRecords,
    entities_with_persistent_identifiers: &std::collections::BTreeMap<u64, Entity>,
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

    if let Some(unlimited) = economy_snapshot_records.unlimited_cash {
        if unlimited {
            commands.insert_resource(openzt2_zoo_state::economy::zoo_cash_types::UnlimitedZooCash);
        } else {
            commands
                .remove_resource::<openzt2_zoo_state::economy::zoo_cash_types::UnlimitedZooCash>();
        }
    }
    use openzt2_zoo_state::economy::{
        behavior_transaction_state::EconomyUsage, facility_economy_types::*,
    };
    for record in economy_snapshot_records.entity_economy_records {
        let entity = entities_with_persistent_identifiers
            .get(&record.persistent_identifier.0)
            .copied()
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let mut entity = commands.entity(entity);
        if record.complete_component_state {
            entity.remove::<(
                OperatingSinceDay,
                FacilityProfit,
                FacilityPriceIndex,
                MaintenanceScheduleIndex,
                Wallet,
                Price,
                ServiceFacility,
                Inventory,
                EconomyUsage,
                ObjectEconomyFactsResolved,
                ServiceFacilityFactsResolved,
            )>();
        }
        if let Some(day) = record.opening_zoo_day {
            entity.insert(OperatingSinceDay(day));
        }
        if let Some((profit, transactions)) = record.profit {
            entity.insert(FacilityProfit {
                total: Money(profit),
                transactions,
            });
        }
        if let Some(index) = record.price_index {
            entity.insert(FacilityPriceIndex(index));
        }
        if let Some(index) = record.maintenance_schedule_index {
            entity.insert(MaintenanceScheduleIndex(index));
        }
        if let Some(wallet) = record.wallet {
            entity.insert(Wallet(Money(wallet)));
        }
        if let Some(price) = record.price {
            entity.insert(Price(Money(price)));
        }
        if let Some(service) = record.service {
            entity.insert(ServiceFacility {
                definition: service.definition,
                capacity: service.capacity,
                occupied: service.occupied,
                payment_trigger: service.payment_trigger,
            });
        }
        if let Some([available, reserved, capacity]) = record.inventory {
            entity.insert(Inventory {
                available,
                reserved,
                capacity,
            });
        }
        if let Some([active, total]) = record.usage {
            entity.insert(EconomyUsage::from_counts(active, total));
        }
    }

    Ok(())
}
