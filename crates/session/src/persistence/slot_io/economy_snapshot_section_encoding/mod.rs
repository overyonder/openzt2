use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinance;

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use super::economy_snapshot_types::{
    EconomySnapshotRecords, EntityEconomySnapshotRecord, ServiceFacilitySnapshot,
};

const FACILITY_ECONOMY_SNAPSHOT_RECORD_BYTE_COUNT: usize = 24;

pub(super) fn append_encoded_economy_snapshot_section(
    bytes: &mut Vec<u8>,
    economy: &EconomySnapshotRecords,
) {
    bytes.extend_from_slice(&economy.zoo_cash.0.to_le_bytes());
    bytes.extend_from_slice(&economy.admission_price.0.to_le_bytes());
    bytes.push(u8::from(economy.admissions_open));
    bytes.push(u8::from(economy.monthly_finance_history_initialized));
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&economy.current_calendar_month.to_le_bytes());
    bytes.extend_from_slice(&economy.lifetime_income.0.to_le_bytes());
    bytes.extend_from_slice(&economy.lifetime_expenses.0.to_le_bytes());
    bytes.extend_from_slice(&economy.lifetime_donation_income.0.to_le_bytes());
    bytes.extend_from_slice(&economy.lifetime_users.to_le_bytes());
    bytes.extend_from_slice(&(economy.monthly_finance_records.len() as u32).to_le_bytes());
    bytes.push(match economy.unlimited_cash {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    });
    for month in &economy.monthly_finance_records {
        bytes.extend_from_slice(&month.month_ordinal.to_le_bytes());
        bytes.extend_from_slice(&month.opening_cash.0.to_le_bytes());
        bytes.extend_from_slice(&month.closing_cash.0.to_le_bytes());
        bytes.extend_from_slice(&month.income.0.to_le_bytes());
        bytes.extend_from_slice(&month.expenses.0.to_le_bytes());
        bytes.extend_from_slice(&month.admission_income.0.to_le_bytes());
        bytes.extend_from_slice(&month.cash_grants.0.to_le_bytes());
        bytes.extend_from_slice(&month.donation_income.0.to_le_bytes());
        bytes.extend_from_slice(&month.food_drink_sales.0.to_le_bytes());
        bytes.extend_from_slice(&month.recycling_income.0.to_le_bytes());
        bytes.extend_from_slice(&month.gift_sales.0.to_le_bytes());
        bytes.extend_from_slice(&month.animal_adoption.0.to_le_bytes());
        bytes.extend_from_slice(&month.animal_upkeep.0.to_le_bytes());
        bytes.extend_from_slice(&month.construction.0.to_le_bytes());
        bytes.extend_from_slice(&month.research.0.to_le_bytes());
        bytes.extend_from_slice(&month.staff_salaries.0.to_le_bytes());
        bytes.extend_from_slice(&month.upkeep.0.to_le_bytes());
        bytes.extend_from_slice(&month.active_users.to_le_bytes());
        bytes.extend_from_slice(&month.total_users.to_le_bytes());
    }
    for record in &economy.entity_economy_records {
        encode_entity_economy_record(bytes, record);
    }
}

pub(super) fn decode_and_validate_economy_snapshot_section(
    bytes: &[u8],
    range: WorldSnapshotSectionDirectoryEntry,
) -> Result<EconomySnapshotRecords, WorldSnapshotPersistenceFailure> {
    const LEGACY_HEADER_BYTES: usize = 60;
    const MONTH_BYTES: usize = 140;
    let payload = &bytes[range.payload_byte_range_within_container(bytes.len())?];
    if payload.len() < LEGACY_HEADER_BYTES || range.encoded_record_count == 0 {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let read_signed_64 = |offset: usize| {
        payload
            .get(offset..offset + 8)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
            .try_into()
            .map(i64::from_le_bytes)
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
    };
    let read_unsigned_32 = |offset: usize| {
        payload
            .get(offset..offset + 4)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
            .try_into()
            .map(u32::from_le_bytes)
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
    };
    let read_i32 = |offset: usize| {
        payload
            .get(offset..offset + 4)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
            .try_into()
            .map(i32::from_le_bytes)
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
    };
    let admissions_open = match payload[16] {
        0 => false,
        1 => true,
        _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
    };
    let history_initialized = match payload[17] {
        0 => false,
        1 => true,
        _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
    };
    let revision = u16::from_le_bytes([payload[18], payload[19]]);
    let (header_bytes, unlimited_cash) = match revision {
        0 => (LEGACY_HEADER_BYTES, None),
        1 => match payload.get(60) {
            Some(0) => (61, None),
            Some(1) => (61, Some(false)),
            Some(2) => (61, Some(true)),
            _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
        },
        _ => return Err(WorldSnapshotPersistenceFailure::UnsupportedSnapshotFormatVersion),
    };
    let month_count = read_unsigned_32(56)? as usize;
    if month_count > 120 {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    let months_end = header_bytes
        .checked_add(
            month_count
                .checked_mul(MONTH_BYTES)
                .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?,
        )
        .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?;
    let facility_count = (range.encoded_record_count as usize)
        .checked_sub(1 + month_count)
        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    if facility_count > 65_536 {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    if months_end > payload.len() {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let expected = months_end
        .checked_add(
            facility_count
                .checked_mul(FACILITY_ECONOMY_SNAPSHOT_RECORD_BYTE_COUNT)
                .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?,
        )
        .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?;
    if revision == 0 && expected != payload.len() {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let mut months = Vec::with_capacity(month_count);
    for index in 0..month_count {
        let offset = header_bytes + index * MONTH_BYTES;
        months.push(MonthlyFinance {
            month_ordinal: read_unsigned_32(offset)?,
            opening_cash: Money(read_signed_64(offset + 4)?),
            closing_cash: Money(read_signed_64(offset + 12)?),
            income: Money(read_signed_64(offset + 20)?),
            expenses: Money(read_signed_64(offset + 28)?),
            admission_income: Money(read_signed_64(offset + 36)?),
            cash_grants: Money(read_signed_64(offset + 44)?),
            donation_income: Money(read_signed_64(offset + 52)?),
            food_drink_sales: Money(read_signed_64(offset + 60)?),
            recycling_income: Money(read_signed_64(offset + 68)?),
            gift_sales: Money(read_signed_64(offset + 76)?),
            animal_adoption: Money(read_signed_64(offset + 84)?),
            animal_upkeep: Money(read_signed_64(offset + 92)?),
            construction: Money(read_signed_64(offset + 100)?),
            research: Money(read_signed_64(offset + 108)?),
            staff_salaries: Money(read_signed_64(offset + 116)?),
            upkeep: Money(read_signed_64(offset + 124)?),
            active_users: read_i32(offset + 132)?,
            total_users: read_i32(offset + 136)?,
        });
    }
    let facilities = if revision == 0 {
        let mut records = Vec::with_capacity(facility_count);
        for record in
            payload[months_end..].chunks_exact(FACILITY_ECONOMY_SNAPSHOT_RECORD_BYTE_COUNT)
        {
            records.push(EntityEconomySnapshotRecord {
                persistent_identifier: PersistentId(u64::from_le_bytes(
                    record[0..8].try_into().expect("fixed record width"),
                )),
                complete_component_state: false,
                opening_zoo_day: Some(u32::from_le_bytes(
                    record[8..12].try_into().expect("fixed record width"),
                )),
                profit: Some((
                    i64::from_le_bytes(record[12..20].try_into().expect("fixed record width")),
                    u32::from_le_bytes(record[20..24].try_into().expect("fixed record width")),
                )),
                price_index: None,
                maintenance_schedule_index: None,
                wallet: None,
                price: None,
                service: None,
                inventory: None,
                usage: None,
            });
        }
        records
    } else {
        decode_entity_economy_records(&payload[months_end..], facility_count)?
    };
    if facilities
        .iter()
        .any(|record| matches!(record.persistent_identifier.0, 0 | u64::MAX))
        || facilities
            .windows(2)
            .any(|pair| pair[0].persistent_identifier.0 >= pair[1].persistent_identifier.0)
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let saved = EconomySnapshotRecords {
        zoo_cash: Money(read_signed_64(0)?),
        unlimited_cash,
        admission_price: Money(read_signed_64(8)?),
        admissions_open,
        current_calendar_month: read_unsigned_32(20)?,
        lifetime_income: Money(read_signed_64(24)?),
        lifetime_expenses: Money(read_signed_64(32)?),
        lifetime_donation_income: Money(read_signed_64(40)?),
        lifetime_users: read_signed_64(48)?,
        monthly_finance_history_initialized: history_initialized,
        monthly_finance_records: months,
        entity_economy_records: facilities,
    };
    if saved.admission_price.0 < 0 {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    Ok(saved)
}

fn encode_entity_economy_record(bytes: &mut Vec<u8>, record: &EntityEconomySnapshotRecord) {
    let flags = [
        record.opening_zoo_day.is_some(),
        record.profit.is_some(),
        record.price_index.is_some(),
        record.maintenance_schedule_index.is_some(),
        record.wallet.is_some(),
        record.price.is_some(),
        record.service.is_some(),
        record.inventory.is_some(),
        record.usage.is_some(),
    ]
    .into_iter()
    .enumerate()
    .fold(
        u16::from(record.complete_component_state) << 9,
        |flags, (bit, present)| flags | (u16::from(present) << bit),
    );
    bytes.extend_from_slice(&record.persistent_identifier.0.to_le_bytes());
    bytes.extend_from_slice(&flags.to_le_bytes());
    if let Some(day) = record.opening_zoo_day {
        bytes.extend_from_slice(&day.to_le_bytes());
    }
    if let Some((profit, count)) = record.profit {
        bytes.extend_from_slice(&profit.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
    }
    if let Some(index) = record.price_index {
        bytes.push(index);
    }
    if let Some(index) = record.maintenance_schedule_index {
        bytes.push(index);
    }
    if let Some(wallet) = record.wallet {
        bytes.extend_from_slice(&wallet.to_le_bytes());
    }
    if let Some(price) = record.price {
        bytes.extend_from_slice(&price.to_le_bytes());
    }
    if let Some(service) = record.service {
        bytes.extend_from_slice(&service.definition.0);
        bytes.extend_from_slice(&service.capacity.to_le_bytes());
        bytes.extend_from_slice(&service.occupied.to_le_bytes());
        bytes.push(match service.payment_trigger {
            openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityPaymentTrigger::AuthoredBehavior => 0,
            openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityPaymentTrigger::TimedTicks => 1,
        });
    }
    if let Some(stock) = record.inventory {
        for count in stock {
            bytes.extend_from_slice(&count.to_le_bytes());
        }
    }
    if let Some(usage) = record.usage {
        for count in usage {
            bytes.extend_from_slice(&count.to_le_bytes());
        }
    }
}

fn read_economy_bytes<const N: usize>(
    reader: &mut std::io::Cursor<&[u8]>,
) -> Result<[u8; N], WorldSnapshotPersistenceFailure> {
    use std::io::Read;
    let mut bytes = [0; N];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    Ok(bytes)
}

fn decode_entity_economy_records(
    payload: &[u8],
    count: usize,
) -> Result<Vec<EntityEconomySnapshotRecord>, WorldSnapshotPersistenceFailure> {
    let mut reader = std::io::Cursor::new(payload);
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let persistent_identifier =
            PersistentId(u64::from_le_bytes(read_economy_bytes(&mut reader)?));
        let flags = u16::from_le_bytes(read_economy_bytes(&mut reader)?);
        if flags == 0 || flags & !0x3ff != 0 {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        let present = |bit: u32| flags & (1_u16 << bit) != 0;
        let opening_zoo_day = if present(0) {
            Some(u32::from_le_bytes(read_economy_bytes(&mut reader)?))
        } else {
            None
        };
        let profit = if present(1) {
            Some((
                i64::from_le_bytes(read_economy_bytes(&mut reader)?),
                u32::from_le_bytes(read_economy_bytes(&mut reader)?),
            ))
        } else {
            None
        };
        let price_index = if present(2) {
            Some(read_economy_bytes::<1>(&mut reader)?[0])
        } else {
            None
        };
        let maintenance_schedule_index = if present(3) {
            Some(read_economy_bytes::<1>(&mut reader)?[0])
        } else {
            None
        };
        let wallet = if present(4) {
            Some(i64::from_le_bytes(read_economy_bytes(&mut reader)?))
        } else {
            None
        };
        let price = if present(5) {
            Some(i64::from_le_bytes(read_economy_bytes(&mut reader)?))
        } else {
            None
        };
        let service = if present(6) {
            let definition = openzt2_game_data::AssetId(read_economy_bytes(&mut reader)?);
            let capacity = u16::from_le_bytes(read_economy_bytes(&mut reader)?);
            let occupied = u16::from_le_bytes(read_economy_bytes(&mut reader)?);
            let payment_trigger = match read_economy_bytes::<1>(&mut reader)?[0] {
                0 => openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityPaymentTrigger::AuthoredBehavior,
                1 => openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityPaymentTrigger::TimedTicks,
                _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
            };
            if occupied > capacity {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            Some(ServiceFacilitySnapshot {
                definition,
                capacity,
                occupied,
                payment_trigger,
            })
        } else {
            None
        };
        let inventory = if present(7) {
            let stock = [
                u16::from_le_bytes(read_economy_bytes(&mut reader)?),
                u16::from_le_bytes(read_economy_bytes(&mut reader)?),
                u16::from_le_bytes(read_economy_bytes(&mut reader)?),
            ];
            if u32::from(stock[0]) + u32::from(stock[1]) > u32::from(stock[2]) {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            Some(stock)
        } else {
            None
        };
        let usage = if present(8) {
            Some([
                i32::from_le_bytes(read_economy_bytes(&mut reader)?),
                i32::from_le_bytes(read_economy_bytes(&mut reader)?),
            ])
        } else {
            None
        };
        records.push(EntityEconomySnapshotRecord {
            persistent_identifier,
            complete_component_state: flags & (1 << 9) != 0,
            opening_zoo_day,
            profit,
            price_index,
            maintenance_schedule_index,
            wallet,
            price,
            service,
            inventory,
            usage,
        });
    }
    if reader.position() != payload.len() as u64 {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    Ok(records)
}

#[cfg(test)]
mod tests;
