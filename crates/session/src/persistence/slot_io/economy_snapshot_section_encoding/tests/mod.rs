use super::*;
use openzt2_game_data::{
    world_definitions::facilities_and_maintenance::FacilityPaymentTrigger, AssetId,
};

fn economy() -> EconomySnapshotRecords {
    EconomySnapshotRecords {
        zoo_cash: Money(12345),
        unlimited_cash: Some(true),
        admission_price: Money(250),
        admissions_open: true,
        current_calendar_month: 0,
        lifetime_income: Money::ZERO,
        lifetime_expenses: Money::ZERO,
        lifetime_donation_income: Money::ZERO,
        lifetime_users: 0,
        monthly_finance_history_initialized: false,
        monthly_finance_records: vec![],
        entity_economy_records: vec![EntityEconomySnapshotRecord {
            persistent_identifier: PersistentId(42),
            complete_component_state: true,
            opening_zoo_day: Some(3),
            profit: Some((1250, 2)),
            price_index: Some(4),
            maintenance_schedule_index: Some(2),
            wallet: Some(2000),
            price: Some(500),
            service: Some(ServiceFacilitySnapshot {
                definition: AssetId::from_key("test-service"),
                capacity: 3,
                occupied: 2,
                payment_trigger: FacilityPaymentTrigger::TimedTicks,
            }),
            inventory: Some([6, 4, 10]),
            usage: Some([2, 7]),
        }],
    }
}

fn decode(
    bytes: &[u8],
    count: u32,
) -> Result<EconomySnapshotRecords, WorldSnapshotPersistenceFailure> {
    decode_and_validate_economy_snapshot_section(
        bytes,
        WorldSnapshotSectionDirectoryEntry {
            encoded_record_count: count,
            payload_start_byte_offset: 0,
            payload_byte_count: bytes.len() as u64,
        },
    )
}

#[test]
fn mutable_account_capacity_and_inventory_survive_economy_round_trip() {
    let mut bytes = Vec::new();
    append_encoded_economy_snapshot_section(&mut bytes, &economy());
    let restored = decode(&bytes, 2).unwrap();
    assert_eq!(restored.unlimited_cash, Some(true));
    let record = restored.entity_economy_records[0];
    assert_eq!(record.profit, Some((1250, 2)));
    assert_eq!(record.price_index, Some(4));
    assert_eq!(record.maintenance_schedule_index, Some(2));
    assert_eq!(record.wallet, Some(2000));
    assert_eq!(record.price, Some(500));
    assert_eq!(record.service.unwrap().occupied, 2);
    assert_eq!(record.inventory, Some([6, 4, 10]));
    assert_eq!(record.usage, Some([2, 7]));
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        decode(&trailing, 2),
        Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
    ));
    bytes[18..20].copy_from_slice(&2_u16.to_le_bytes());
    assert!(matches!(
        decode(&bytes, 2),
        Err(WorldSnapshotPersistenceFailure::UnsupportedSnapshotFormatVersion)
    ));
}

#[test]
fn revision_zero_facility_records_keep_their_profit_and_age() {
    let mut bytes = Vec::new();
    append_encoded_economy_snapshot_section(&mut bytes, &economy());
    bytes.truncate(60);
    bytes[18..20].copy_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&42_u64.to_le_bytes());
    bytes.extend_from_slice(&3_u32.to_le_bytes());
    bytes.extend_from_slice(&1250_i64.to_le_bytes());
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    let restored = decode(&bytes, 2).unwrap();
    assert_eq!(restored.unlimited_cash, None);
    assert_eq!(restored.entity_economy_records[0].opening_zoo_day, Some(3));
    assert_eq!(restored.entity_economy_records[0].profit, Some((1250, 2)));
    assert!(restored.entity_economy_records[0].inventory.is_none());
    let mut reencoded = Vec::new();
    append_encoded_economy_snapshot_section(&mut reencoded, &restored);
    let reread = decode(&reencoded, 2).unwrap();
    assert_eq!(reread.unlimited_cash, None);
    assert!(!reread.entity_economy_records[0].complete_component_state);
    bytes.pop();
    assert!(matches!(
        decode(&bytes, 2),
        Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
    ));
}

#[test]
fn complete_absence_clears_retained_state_and_legacy_absence_preserves_it() {
    use bevy::{ecs::system::SystemState, prelude::*};
    use openzt2_zoo_state::economy::{
        behavior_transaction_state::EconomyUsage,
        facility_economy_types::*,
        guest_admission_types::{AdmissionPrice, ZooAdmissionsOpen},
        monthly_finance_types::MonthlyFinanceHistory,
        zoo_cash_types::{UnlimitedZooCash, ZooCash},
    };
    let mut world = World::new();
    world.init_resource::<ZooCash>();
    world.init_resource::<AdmissionPrice>();
    world.init_resource::<ZooAdmissionsOpen>();
    world.init_resource::<MonthlyFinanceHistory>();
    world.insert_resource(UnlimitedZooCash);
    let entity = world
        .spawn((
            Wallet(Money(900)),
            FacilityPriceIndex(4),
            ObjectEconomyFactsResolved,
            ServiceFacilityFactsResolved,
        ))
        .id();
    let mut state = SystemState::<(
        Commands,
        ResMut<ZooCash>,
        ResMut<AdmissionPrice>,
        ResMut<ZooAdmissionsOpen>,
        ResMut<MonthlyFinanceHistory>,
    )>::new(&mut world);
    for complete in [false, true] {
        let mut saved = economy();
        saved.unlimited_cash = None;
        saved.entity_economy_records = vec![EntityEconomySnapshotRecord {
            persistent_identifier: PersistentId(42),
            complete_component_state: complete,
            opening_zoo_day: None,
            profit: None,
            price_index: None,
            maintenance_schedule_index: None,
            wallet: None,
            price: None,
            service: None,
            inventory: None,
            usage: None,
        }];
        {
            let (mut commands, mut cash, mut price, mut admissions, mut history) =
                state.get_mut(&mut world).unwrap();
            super::super::economy_snapshot_application::apply_economy_snapshot_records_to_live_economy_resources_and_facilities(
            &mut commands, saved, &std::collections::BTreeMap::from([(42, entity)]), &mut cash, &mut price, &mut admissions, &mut history,
        ).unwrap();
        }
        state.apply(&mut world);
        assert_eq!(world.get::<Wallet>(entity).is_none(), complete);
        assert_eq!(world.get::<FacilityPriceIndex>(entity).is_none(), complete);
        assert_eq!(
            world.get::<ObjectEconomyFactsResolved>(entity).is_none(),
            complete
        );
        assert_eq!(
            world.get::<ServiceFacilityFactsResolved>(entity).is_none(),
            complete
        );
        assert!(world.contains_resource::<UnlimitedZooCash>());
        assert!(world.get::<EconomyUsage>(entity).is_none());
    }
    let mut saved = economy();
    let record = &mut saved.entity_economy_records[0];
    record.opening_zoo_day = None;
    record.profit = None;
    record.price_index = None;
    record.maintenance_schedule_index = None;
    record.wallet = None;
    record.price = None;
    record.service = None;
    record.inventory = None;
    record.usage = None;
    let mut bytes = Vec::new();
    append_encoded_economy_snapshot_section(&mut bytes, &saved);
    assert!(decode(&bytes, 2).unwrap().entity_economy_records[0].complete_component_state);
}
