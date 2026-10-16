use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::donations::donation_payment_types::{DonationCategoryTotals, DonationTotal};
use openzt2_zoo_state::economy::money_types::Money;

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};

#[cfg(test)]
mod tests;

const MAXIMUM_DONATION_ROWS: usize = 65_536;
const MAXIMUM_DONATION_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct DonationSnapshotRecord {
    identifier: PersistentId,
    total: DonationTotal,
    categories: DonationCategoryTotals,
}

pub(super) type DonationSnapshotEntities<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static PersistentId>,
        Option<&'static DonationTotal>,
        Option<&'static DonationCategoryTotals>,
    ),
    Or<(With<DonationTotal>, With<DonationCategoryTotals>)>,
>;

pub(super) fn capture_donation_records(
    entities: &DonationSnapshotEntities,
) -> Result<Vec<DonationSnapshotRecord>, Failure> {
    let mut records = Vec::new();
    for (identifier, total, categories) in entities {
        records.push(DonationSnapshotRecord {
            identifier: *identifier.ok_or(Failure::BrokenPersistentEntityReference)?,
            total: total.copied().unwrap_or_default(),
            categories: categories.cloned().unwrap_or_default(),
        });
    }
    records.sort_unstable_by_key(|record| record.identifier.0);
    validate_donation_records(&records)?;
    Ok(records)
}

fn validate_donation_records(records: &[DonationSnapshotRecord]) -> Result<(), Failure> {
    if records.len() > MAXIMUM_DONATION_ROWS {
        return Err(Failure::CapacityExceeded);
    }
    let mut category_count = 0_usize;
    for (index, record) in records.iter().enumerate() {
        category_count = category_count.saturating_add(record.categories.iter().count());
        if category_count > MAXIMUM_DONATION_ROWS {
            return Err(Failure::CapacityExceeded);
        }
        if matches!(record.identifier.0, 0 | u64::MAX)
            || index > 0 && records[index - 1].identifier.0 >= record.identifier.0
            || record.total.amount.0 < 0
            || (record.total.amount.0 == 0) != (record.total.count == 0)
            || record.categories.aggregate_for_category(None) != Some(record.total)
        {
            return Err(Failure::CorruptSnapshotSection);
        }
    }
    Ok(())
}

pub(super) fn encode_donation_records(
    records: &[DonationSnapshotRecord],
) -> Result<Vec<u8>, Failure> {
    validate_donation_records(records)?;
    let mut bytes = Vec::new();
    // Little endian acceptor rows: id:u64, cents:i64, count:u32, categories:u32;
    // followed by category rows: id:[u8;16], cents:i64, count:u32.
    // The legacy empty Donations section remains an empty byte slice.
    for record in records {
        bytes.extend_from_slice(&record.identifier.0.to_le_bytes());
        bytes.extend_from_slice(&record.total.amount.0.to_le_bytes());
        bytes.extend_from_slice(&record.total.count.to_le_bytes());
        let mut categories: Vec<_> = record.categories.iter().collect();
        categories.sort_unstable_by_key(|(category, _)| *category);
        bytes.extend_from_slice(&(categories.len() as u32).to_le_bytes());
        for (category, total) in categories {
            bytes.extend_from_slice(&category.0);
            bytes.extend_from_slice(&total.amount.0.to_le_bytes());
            bytes.extend_from_slice(&total.count.to_le_bytes());
        }
    }
    Ok(bytes)
}

pub(super) fn decode_donation_records(
    bytes: &[u8],
    section: WorldSnapshotSectionDirectoryEntry,
) -> Result<Vec<DonationSnapshotRecord>, Failure> {
    if section.payload_byte_count > MAXIMUM_DONATION_BYTES as u64
        || section.encoded_record_count as usize > MAXIMUM_DONATION_ROWS
    {
        return Err(Failure::CapacityExceeded);
    }
    let mut payload = &bytes[section.payload_byte_range_within_container(bytes.len())?];
    let mut records = Vec::new();
    let mut category_rows = 0_usize;
    for _ in 0..section.encoded_record_count {
        let (header, rest) = payload
            .split_first_chunk::<24>()
            .ok_or(Failure::CorruptSnapshotSection)?;
        payload = rest;
        let identifier = PersistentId(u64::from_le_bytes(header[0..8].try_into().unwrap()));
        let total = DonationTotal {
            amount: Money(i64::from_le_bytes(header[8..16].try_into().unwrap())),
            count: u32::from_le_bytes(header[16..20].try_into().unwrap()),
        };
        let count = u32::from_le_bytes(header[20..24].try_into().unwrap()) as usize;
        category_rows = category_rows.saturating_add(count);
        if category_rows > MAXIMUM_DONATION_ROWS {
            return Err(Failure::CapacityExceeded);
        }
        let mut categories = Vec::new();
        for _ in 0..count {
            let (row, rest) = payload
                .split_first_chunk::<28>()
                .ok_or(Failure::CorruptSnapshotSection)?;
            payload = rest;
            categories.push((
                AssetId(row[0..16].try_into().unwrap()),
                DonationTotal {
                    amount: Money(i64::from_le_bytes(row[16..24].try_into().unwrap())),
                    count: u32::from_le_bytes(row[24..28].try_into().unwrap()),
                },
            ));
        }
        records.push(DonationSnapshotRecord {
            identifier,
            total,
            categories: DonationCategoryTotals::from_completed_totals(categories)
                .ok_or(Failure::CorruptSnapshotSection)?,
        });
    }
    if !payload.is_empty() {
        return Err(Failure::CorruptSnapshotSection);
    }
    validate_donation_records(&records)?;
    Ok(records)
}

pub(super) fn resolve_donation_records(
    records: Vec<DonationSnapshotRecord>,
    entities: &Query<(Entity, &PersistentId)>,
) -> Result<Vec<(Entity, DonationTotal, DonationCategoryTotals)>, Failure> {
    records
        .into_iter()
        .map(|record| {
            let mut matches = entities
                .iter()
                .filter(|(_, identifier)| **identifier == record.identifier);
            let entity = matches
                .next()
                .ok_or(Failure::BrokenPersistentEntityReference)?
                .0;
            if matches.next().is_some() {
                return Err(Failure::DuplicatePersistentEntityIdentifier);
            }
            Ok((entity, record.total, record.categories))
        })
        .collect()
}

pub(super) fn apply_donation_records(
    commands: &mut Commands,
    records: Vec<(Entity, DonationTotal, DonationCategoryTotals)>,
) {
    for (entity, total, categories) in records {
        commands.entity(entity).insert((total, categories));
    }
}
