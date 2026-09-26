use crate::plugins::{
    economy::{money_types::Money, monthly_finance_types::MonthlyFinance},
    world_spawn::persistent_id_types::PersistentId,
};

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use super::economy_snapshot_types::{EconomySnapshotRecords, FacilityEconomySnapshotRecord};

const FACILITY_ECONOMY_SNAPSHOT_RECORD_BYTE_COUNT: usize = 24;

pub(super) fn append_encoded_economy_snapshot_section(
    bytes: &mut Vec<u8>,
    economy: &EconomySnapshotRecords,
) {
    bytes.extend_from_slice(&economy.zoo_cash.0.to_le_bytes());
    bytes.extend_from_slice(&economy.admission_price.0.to_le_bytes());
    bytes.push(u8::from(economy.admissions_open));
    bytes.push(u8::from(economy.monthly_finance_history_initialized));
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&economy.current_calendar_month.to_le_bytes());
    bytes.extend_from_slice(&economy.lifetime_income.0.to_le_bytes());
    bytes.extend_from_slice(&economy.lifetime_expenses.0.to_le_bytes());
    bytes.extend_from_slice(&economy.lifetime_donation_income.0.to_le_bytes());
    bytes.extend_from_slice(&economy.lifetime_users.to_le_bytes());
    bytes.extend_from_slice(&(economy.monthly_finance_records.len() as u32).to_le_bytes());
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
    for record in &economy.facility_economy_records {
        bytes.extend_from_slice(&record.persistent_identifier.0.to_le_bytes());
        bytes.extend_from_slice(&record.opening_zoo_day.to_le_bytes());
        bytes.extend_from_slice(&record.total_profit_cents.to_le_bytes());
        bytes.extend_from_slice(&record.transaction_count.to_le_bytes());
    }
}

pub(super) fn decode_and_validate_economy_snapshot_section(
    bytes: &[u8],
    range: WorldSnapshotSectionDirectoryEntry,
) -> Result<EconomySnapshotRecords, WorldSnapshotPersistenceFailure> {
    const HEADER_BYTES: usize = 60;
    const MONTH_BYTES: usize = 140;
    let payload = &bytes[range.payload_byte_range_within_container(bytes.len())?];
    if payload.len() < HEADER_BYTES || range.encoded_record_count == 0 {
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
    if payload[18..20] != [0, 0] {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let month_count = read_unsigned_32(56)? as usize;
    if month_count > 120 {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    let months_end = HEADER_BYTES
        .checked_add(
            month_count
                .checked_mul(MONTH_BYTES)
                .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?,
        )
        .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?;
    let facility_count = (range.encoded_record_count as usize)
        .checked_sub(1 + month_count)
        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    let expected = months_end
        .checked_add(
            facility_count
                .checked_mul(FACILITY_ECONOMY_SNAPSHOT_RECORD_BYTE_COUNT)
                .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?,
        )
        .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?;
    if expected != payload.len() {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let mut months = Vec::with_capacity(month_count);
    for index in 0..month_count {
        let offset = HEADER_BYTES + index * MONTH_BYTES;
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
    let mut facilities = Vec::with_capacity(facility_count);
    for record in payload[months_end..].chunks_exact(FACILITY_ECONOMY_SNAPSHOT_RECORD_BYTE_COUNT) {
        let id =
            PersistentId(u64::from_le_bytes(record[0..8].try_into().map_err(
                |_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection,
            )?));
        let opened_day = u32::from_le_bytes(
            record[8..12]
                .try_into()
                .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
        );
        let total_profit_cents = i64::from_le_bytes(
            record[12..20]
                .try_into()
                .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
        );
        let transactions = u32::from_le_bytes(
            record[20..24]
                .try_into()
                .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
        );
        if id.0 == 0
            || facilities
                .last()
                .is_some_and(|prior: &FacilityEconomySnapshotRecord| {
                    prior.persistent_identifier.0 >= id.0
                })
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        facilities.push(FacilityEconomySnapshotRecord {
            persistent_identifier: id,
            opening_zoo_day: opened_day,
            total_profit_cents,
            transaction_count: transactions,
        });
    }
    let saved = EconomySnapshotRecords {
        zoo_cash: Money(read_signed_64(0)?),
        admission_price: Money(read_signed_64(8)?),
        admissions_open,
        current_calendar_month: read_unsigned_32(20)?,
        lifetime_income: Money(read_signed_64(24)?),
        lifetime_expenses: Money(read_signed_64(32)?),
        lifetime_donation_income: Money(read_signed_64(40)?),
        lifetime_users: read_signed_64(48)?,
        monthly_finance_history_initialized: history_initialized,
        monthly_finance_records: months,
        facility_economy_records: facilities,
    };
    if saved.admission_price.0 < 0 {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    Ok(saved)
}
