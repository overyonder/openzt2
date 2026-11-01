use std::io::{Cursor, Read};

use openzt2_zoo_state::economy::{
    money_types::Money,
    monthly_finance_types::{FinanceMetric, FinancePeriod, MonthlyFinance, MonthlyFinanceHistory},
};

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;

pub(super) const MONTHLY_FINANCE_RECORD_BYTE_COUNT: usize = 140;
const HISTORY_HEADER_BYTE_COUNT: usize = 41;
const MAX_RETAINED_MONTHS: usize =
    openzt2_zoo_state::economy::monthly_finance_types::MONTHLY_FINANCE_HISTORY_CAPACITY;

pub(super) fn append_monthly_finance_record(bytes: &mut Vec<u8>, month: &MonthlyFinance) {
    bytes.extend_from_slice(&month.month_ordinal.to_le_bytes());
    for value in [
        month.opening_cash,
        month.closing_cash,
        month.income,
        month.expenses,
        month.admission_income,
        month.cash_grants,
        month.donation_income,
        month.food_drink_sales,
        month.recycling_income,
        month.gift_sales,
        month.animal_adoption,
        month.animal_upkeep,
        month.construction,
        month.research,
        month.staff_salaries,
        month.upkeep,
    ] {
        bytes.extend_from_slice(&value.0.to_le_bytes());
    }
    bytes.extend_from_slice(&month.active_users.to_le_bytes());
    bytes.extend_from_slice(&month.total_users.to_le_bytes());
}

pub(super) fn decode_monthly_finance_record(
    bytes: &[u8],
) -> Result<MonthlyFinance, WorldSnapshotPersistenceFailure> {
    if bytes.len() != MONTHLY_FINANCE_RECORD_BYTE_COUNT {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let mut reader = Cursor::new(bytes);
    Ok(MonthlyFinance {
        month_ordinal: u32::from_le_bytes(read_bytes(&mut reader)?),
        opening_cash: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        closing_cash: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        income: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        expenses: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        admission_income: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        cash_grants: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        donation_income: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        food_drink_sales: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        recycling_income: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        gift_sales: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        animal_adoption: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        animal_upkeep: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        construction: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        research: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        staff_salaries: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        upkeep: Money(i64::from_le_bytes(read_bytes(&mut reader)?)),
        active_users: i32::from_le_bytes(read_bytes(&mut reader)?),
        total_users: i32::from_le_bytes(read_bytes(&mut reader)?),
    })
}

pub(super) fn encode_monthly_finance_history(history: &MonthlyFinanceHistory) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(
        HISTORY_HEADER_BYTE_COUNT
            + history.retained_month_finance_records().len() * MONTHLY_FINANCE_RECORD_BYTE_COUNT,
    );
    bytes.push(u8::from(history.has_initialized_month()));
    bytes.extend_from_slice(&history.current_calendar_month_ordinal().to_le_bytes());
    for metric in [
        FinanceMetric::Income,
        FinanceMetric::Expenses,
        FinanceMetric::DonationIncome,
        FinanceMetric::TotalUsers,
    ] {
        bytes.extend_from_slice(
            &history
                .finance_metric_value(metric, FinancePeriod::Lifetime)
                .to_le_bytes(),
        );
    }
    let months = history.retained_month_finance_records();
    bytes.extend_from_slice(&(months.len() as u32).to_le_bytes());
    for month in months {
        append_monthly_finance_record(&mut bytes, month);
    }
    bytes
}

pub(super) fn decode_monthly_finance_history(
    bytes: &[u8],
) -> Result<MonthlyFinanceHistory, WorldSnapshotPersistenceFailure> {
    if bytes.len() < HISTORY_HEADER_BYTE_COUNT {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let mut reader = Cursor::new(bytes);
    let initialized = match read_bytes::<1>(&mut reader)?[0] {
        0 => false,
        1 => true,
        _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
    };
    let calendar_month = u32::from_le_bytes(read_bytes(&mut reader)?);
    let lifetime_income = Money(i64::from_le_bytes(read_bytes(&mut reader)?));
    let lifetime_expenses = Money(i64::from_le_bytes(read_bytes(&mut reader)?));
    let lifetime_donation_income = Money(i64::from_le_bytes(read_bytes(&mut reader)?));
    let lifetime_users = i64::from_le_bytes(read_bytes(&mut reader)?);
    let month_count = u32::from_le_bytes(read_bytes(&mut reader)?) as usize;
    if month_count > MAX_RETAINED_MONTHS {
        return Err(WorldSnapshotPersistenceFailure::CapacityExceeded);
    }
    if bytes.len() != HISTORY_HEADER_BYTE_COUNT + month_count * MONTHLY_FINANCE_RECORD_BYTE_COUNT {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let months = bytes[HISTORY_HEADER_BYTE_COUNT..]
        .chunks_exact(MONTHLY_FINANCE_RECORD_BYTE_COUNT)
        .map(decode_monthly_finance_record)
        .collect::<Result<Vec<_>, _>>()?;
    let mut history = MonthlyFinanceHistory::default();
    if !history.restore_saved_monthly_finance_history(
        &months,
        calendar_month,
        lifetime_income,
        lifetime_expenses,
        lifetime_donation_income,
        lifetime_users,
        initialized,
    ) {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    Ok(history)
}

fn read_bytes<const N: usize>(
    reader: &mut Cursor<&[u8]>,
) -> Result<[u8; N], WorldSnapshotPersistenceFailure> {
    let mut bytes = [0; N];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_zoo_state::economy::account_transaction_types::TransactionKind;

    #[test]
    fn logical_history_round_trip_preserves_rotated_months_totals_and_presence() {
        let mut history = MonthlyFinanceHistory::default();
        let mut cash = Money(10_000);
        for month in 0..126 {
            history.enter_calendar_month(month, cash);
            history.record_guest_arrival();
            cash.0 += 125;
            history.record_completed_transaction(TransactionKind::Reward, 125, cash);
            if month % 2 == 0 {
                history.record_guest_departure();
            }
        }
        for source in [MonthlyFinanceHistory::default(), history] {
            let encoded = encode_monthly_finance_history(&source);
            let restored = decode_monthly_finance_history(&encoded).unwrap();
            assert_eq!(encode_monthly_finance_history(&restored), encoded);
            assert_eq!(
                restored.has_initialized_month(),
                source.has_initialized_month()
            );
            assert_eq!(
                restored.current_calendar_month_ordinal(),
                source.current_calendar_month_ordinal()
            );
            assert_eq!(
                restored
                    .retained_month_finance_records()
                    .collect::<Vec<_>>(),
                source.retained_month_finance_records().collect::<Vec<_>>()
            );
            for metric in [
                FinanceMetric::Income,
                FinanceMetric::Expenses,
                FinanceMetric::DonationIncome,
                FinanceMetric::TotalUsers,
            ] {
                assert_eq!(
                    restored.finance_metric_value(metric, FinancePeriod::Lifetime),
                    source.finance_metric_value(metric, FinancePeriod::Lifetime)
                );
            }
            let mut trailing = encoded.clone();
            trailing.push(0);
            assert!(decode_monthly_finance_history(&trailing).is_err());
            assert!(decode_monthly_finance_history(&encoded[..encoded.len() - 1]).is_err());
        }
        let mut invalid = encode_monthly_finance_history(&history);
        invalid[HISTORY_HEADER_BYTE_COUNT + 52..HISTORY_HEADER_BYTE_COUNT + 60]
            .copy_from_slice(&126_i64.to_le_bytes());
        assert!(decode_monthly_finance_history(&invalid).is_err());
    }
}
