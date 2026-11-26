use openzt2_zoo_state::economy::money_types::Money;

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;

pub(super) fn encode_money(amount: Money) -> [u8; 8] {
    f64::from(amount.dollars()).to_le_bytes()
}

pub(super) fn decode_money(
    bytes: [u8; 8],
    dollars: bool,
) -> Result<Money, WorldSnapshotPersistenceFailure> {
    if !dollars {
        return Ok(Money::from_cents(i64::from_le_bytes(bytes)));
    }
    let wire = f64::from_le_bytes(bytes);
    let number = wire as f32;
    if f64::from(number) != wire {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    Money::from_dollars(number).ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
}
