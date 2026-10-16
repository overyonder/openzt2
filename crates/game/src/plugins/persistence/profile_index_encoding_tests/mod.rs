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

#[test]
fn disabled_message_of_the_day_survives_the_profile_index() {
    let profile_identifier = create_test_profile_identifier(1);
    let encoded_profile_index_bytes = super::profile_index_encoding::encode_stored_profile_index(
        &[
            super::profile_persistence_types::StoredProfileRecordAndSettings {
                record: ProfileRecord {
                    profile_identifier,
                    profile_display_name: "Ava".to_owned(),
                    last_played_unix_timestamp_milliseconds: 10,
                },
                options: super::profile_types::ProfileOptions {
                    profile_identifier,
                    locale_identifier: AssetId::default(),
                    controller_input_enabled: true,
                },
                challenge_progress: Default::default(),
                audio: Default::default(),
                bindings: Default::default(),
                display: Default::default(),
                graphics: Default::default(),
                online_messages:
                    crate::plugins::settings::online_message_policy_types::OnlineMessagePolicy {
                        enabled: false,
                    },
            },
        ],
        None,
    )
    .unwrap();
    let (decoded, _) =
        super::profile_index_encoding::decode_stored_profile_index(&encoded_profile_index_bytes)
            .unwrap();
    assert!(!decoded[0].online_messages.enabled);
    // Profiles written before the flag carry zero there and keep it enabled.
    let (legacy, _) = super::profile_index_encoding::decode_stored_profile_index(
        &super::profile_index_encoding::encode_profile_index_for_test(
            &[decoded[0].record.clone()],
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(legacy[0].online_messages.enabled);
}
