use openzt2_game_data::AssetId;

use super::{
    profile_index_encoding::{decode_profile_index_for_test, encode_profile_index_for_test},
    profile_types::{ProfileFailure, ProfileRecord},
};

fn create_test_profile_identifier(repeated_identifier_byte: u8) -> AssetId {
    AssetId([repeated_identifier_byte; 16])
}

#[test]
fn profile_index_and_options_round_trip_without_ambiguous_identifiers() {
    let profile_records = vec![
        ProfileRecord {
            profile_identifier: create_test_profile_identifier(1),
            profile_display_name: "Ava".to_owned(),
            last_played_unix_timestamp_milliseconds: 10,
        },
        ProfileRecord {
            profile_identifier: create_test_profile_identifier(2),
            profile_display_name: "Morgan".to_owned(),
            last_played_unix_timestamp_milliseconds: 20,
        },
    ];
    let encoded_profile_index_bytes =
        encode_profile_index_for_test(&profile_records, Some(create_test_profile_identifier(2)))
            .unwrap();
    let (decoded_profile_records, selected_profile_identifier) =
        decode_profile_index_for_test(&encoded_profile_index_bytes).unwrap();
    assert_eq!(decoded_profile_records, profile_records);
    assert_eq!(
        selected_profile_identifier,
        Some(create_test_profile_identifier(2))
    );
}

#[test]
fn corrupt_profile_records_are_rejected() {
    let profile_records = vec![ProfileRecord {
        profile_identifier: create_test_profile_identifier(1),
        profile_display_name: "Ava".to_owned(),
        last_played_unix_timestamp_milliseconds: 10,
    }];
    let mut encoded_profile_index_bytes =
        encode_profile_index_for_test(&profile_records, None).unwrap();
    *encoded_profile_index_bytes.last_mut().unwrap() ^= 1;
    assert_eq!(
        decode_profile_index_for_test(&encoded_profile_index_bytes),
        Err(ProfileFailure::CorruptIndex)
    );
}
