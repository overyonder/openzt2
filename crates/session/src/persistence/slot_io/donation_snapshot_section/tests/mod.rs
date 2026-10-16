use super::*;
use bevy::ecs::system::SystemState;

fn section(bytes: &[u8], count: u32) -> WorldSnapshotSectionDirectoryEntry {
    WorldSnapshotSectionDirectoryEntry {
        encoded_record_count: count,
        payload_start_byte_offset: 0,
        payload_byte_count: bytes.len() as u64,
    }
}

#[test]
fn legacy_empty_donations_section_decodes_without_inventing_history() {
    assert!(decode_donation_records(&[], section(&[], 0))
        .unwrap()
        .is_empty());
}

#[test]
fn completed_categories_survive_capture_binary_restore_and_next_payment() {
    let lion = AssetId([1; 16]);
    let hippo = AssetId([2; 16]);
    let mut categories = DonationCategoryTotals::default();
    categories
        .accumulate_completed_donation(lion, Money(499_000))
        .unwrap();
    categories
        .accumulate_completed_donation(hippo, Money(123_400))
        .unwrap();
    let mut source = World::new();
    source.spawn((
        PersistentId(42),
        categories.aggregate_for_category(None).unwrap(),
        categories,
    ));
    source.spawn((
        PersistentId(43),
        DonationTotal::default(),
        DonationCategoryTotals::default(),
    ));
    let mut capture = SystemState::<DonationSnapshotEntities>::new(&mut source);
    let expected = capture_donation_records(&capture.get(&source).unwrap()).unwrap();
    let bytes = encode_donation_records(&expected).unwrap();
    let decoded = decode_donation_records(&bytes, section(&bytes, 2)).unwrap();
    assert_eq!(decoded, expected);

    let mut destination = World::new();
    destination.spawn_empty();
    let acceptor = destination.spawn(PersistentId(42)).id();
    destination.spawn(PersistentId(43));
    let mut state =
        SystemState::<(Commands, Query<(Entity, &PersistentId)>)>::new(&mut destination);
    let (mut commands, entities) = state.get_mut(&mut destination).unwrap();
    let identities = entities.iter().map(|(entity, id)| (id.0, entity)).collect();
    let resolved = resolve_donation_records(decoded, &identities).unwrap();
    apply_donation_records(&mut commands, resolved);
    state.apply(&mut destination);
    assert_eq!(
        destination.get::<DonationTotal>(acceptor).unwrap().amount,
        Money(622_400)
    );
    let mut restored = destination
        .get_mut::<DonationCategoryTotals>(acceptor)
        .unwrap();
    restored
        .accumulate_completed_donation(lion, Money(1_000))
        .unwrap();
    assert_eq!(
        restored.aggregate_for_category(Some(lion)).unwrap(),
        DonationTotal {
            amount: Money(500_000),
            count: 2
        }
    );
    assert_eq!(
        restored.aggregate_for_category(Some(hippo)).unwrap().amount,
        Money(123_400)
    );
}

#[test]
fn malformed_external_category_rows_cannot_duplicate_completed_donations() {
    let mut categories = DonationCategoryTotals::default();
    categories
        .accumulate_completed_donation(AssetId([1; 16]), Money(100))
        .unwrap();
    let records = [DonationSnapshotRecord {
        identifier: PersistentId(1),
        total: categories.aggregate_for_category(None).unwrap(),
        categories,
    }];
    let mut bytes = encode_donation_records(&records).unwrap();
    bytes[20..24].copy_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_within(24..52);
    assert_eq!(
        decode_donation_records(&bytes, section(&bytes, 1)),
        Err(Failure::CorruptSnapshotSection)
    );
}
