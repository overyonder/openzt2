use bevy::{ecs::query::QueryData, prelude::*};
use openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndex;
use openzt2_zoo_state::economy::{
    behavior_transaction_state::EconomyUsage, facility_economy_types::*,
};

use openzt2_zoo_state::economy::guest_admission_types::AdmissionPrice;
use openzt2_zoo_state::economy::guest_admission_types::ZooAdmissionsOpen;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory;
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    economy_snapshot_types::EconomySnapshotRecords,
    restored_component_values::restore_optional_component,
};

#[derive(QueryData)]
#[query_data(mutable)]
pub(super) struct RestoredFacilityEconomy {
    opening_day: Option<&'static mut OperatingSinceDay>,
    profit: Option<&'static mut FacilityProfit>,
    price_index: Option<&'static mut FacilityPriceIndex>,
    schedule_index: Option<&'static mut MaintenanceScheduleIndex>,
    wallet: Option<&'static mut Wallet>,
    price: Option<&'static mut Price>,
    service: Option<&'static mut ServiceFacility>,
    inventory: Option<&'static mut Inventory>,
    usage: Option<&'static mut EconomyUsage>,
}

pub(super) fn apply_economy_snapshot_records_to_live_economy_resources_and_facilities(
    commands: &mut Commands,
    economy_snapshot_records: EconomySnapshotRecords,
    identities: &PersistentIdIndex,
    root: Entity,
    facilities: &mut Query<RestoredFacilityEconomy>,
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
    for record in economy_snapshot_records.entity_economy_records {
        let entity = identities
            .get(root, record.persistent_identifier)
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let live = facilities
            .get_mut(entity)
            .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let mut entity = commands.entity(entity);
        let remove = record.complete_component_state;
        if remove {
            entity.remove::<(ObjectEconomyFactsResolved, ServiceFacilityFactsResolved)>();
        }
        let profit = record.profit.map(|(profit, transactions)| FacilityProfit {
            total: Money(profit),
            transactions,
        });
        let service = record.service.map(|service| ServiceFacility {
            definition: service.definition,
            capacity: service.capacity,
            payment_trigger: service.payment_trigger,
        });
        let inventory = record
            .inventory
            .map(|[available, reserved, capacity]| Inventory {
                available: available.saturating_add(reserved).min(capacity),
                reserved: 0,
                capacity,
            });
        restore_optional_component(
            &mut entity,
            live.opening_day,
            record.opening_zoo_day.map(OperatingSinceDay),
            remove,
        );
        restore_optional_component(&mut entity, live.profit, profit, remove);
        restore_optional_component(
            &mut entity,
            live.price_index,
            record.price_index.map(FacilityPriceIndex),
            remove,
        );
        restore_optional_component(
            &mut entity,
            live.schedule_index,
            record
                .maintenance_schedule_index
                .map(MaintenanceScheduleIndex),
            remove,
        );
        restore_optional_component(
            &mut entity,
            live.wallet,
            record.wallet.map(|wallet| Wallet(Money(wallet))),
            remove,
        );
        restore_optional_component(
            &mut entity,
            live.price,
            record.price.map(|price| Price(Money(price))),
            remove,
        );
        restore_optional_component(&mut entity, live.service, service, remove);
        restore_optional_component(&mut entity, live.inventory, inventory, remove);
        restore_optional_component(
            &mut entity,
            live.usage,
            record
                .usage
                .map(|[active, total]| EconomyUsage::from_counts(active, total)),
            remove,
        );
    }

    Ok(())
}
