use bevy::{ecs::query::QueryData, prelude::*};
use openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndex;
use openzt2_zoo_state::economy::{
    behavior_transaction_state::EconomyUsage, facility_economy_types::*,
};

use openzt2_zoo_state::economy::guest_admission_types::AdmissionPrice;
use openzt2_zoo_state::economy::guest_admission_types::ZooAdmissionsOpen;
use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory;
use openzt2_zoo_state::economy::zoo_cash_types::{UnlimitedZooCash, ZooCash};

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

#[derive(QueryData)]
#[query_data(mutable)]
pub(super) struct RestoredZooEconomy {
    cash: &'static mut ZooCash,
    admission_price: &'static mut AdmissionPrice,
    admissions_open: &'static mut ZooAdmissionsOpen,
    history: &'static mut MonthlyFinanceHistory,
    unlimited_cash: Option<&'static mut UnlimitedZooCash>,
}

pub(super) fn apply_economy_snapshot_records(
    commands: &mut Commands,
    economy_snapshot_records: EconomySnapshotRecords,
    identities: &PersistentIdIndex,
    root: Entity,
    facilities: &mut Query<RestoredFacilityEconomy>,
    roots: &mut Query<RestoredZooEconomy>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let mut zoo = roots
        .get_mut(root)
        .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
    let mut history = *zoo.history;
    if !history.restore_saved_monthly_finance_history(
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

    zoo.history.set_if_neq(history);
    zoo.cash
        .set_if_neq(ZooCash(economy_snapshot_records.zoo_cash));
    zoo.admission_price.set_if_neq(AdmissionPrice::from_adult(
        economy_snapshot_records.admission_price,
    ));
    zoo.admissions_open
        .set_if_neq(ZooAdmissionsOpen(economy_snapshot_records.admissions_open));
    if let Some(unlimited) = economy_snapshot_records.unlimited_cash {
        if let Some(mut policy) = zoo.unlimited_cash {
            policy.set_if_neq(UnlimitedZooCash(unlimited));
        } else {
            commands.entity(root).insert(UnlimitedZooCash(unlimited));
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
            total: profit,
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
        restore_optional_component(&mut entity, live.wallet, record.wallet.map(Wallet), remove);
        restore_optional_component(&mut entity, live.price, record.price.map(Price), remove);
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
