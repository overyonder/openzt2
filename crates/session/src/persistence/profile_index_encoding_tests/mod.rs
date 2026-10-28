use openzt2_game_data::AssetId;

use openzt2_interface_state::persistence::profile_types::ProfileFailure;
use openzt2_interface_state::persistence::profile_types::ProfileRecord;

use super::profile_index_encoding::{decode_profile_index_for_test, encode_profile_index_for_test};

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
fn older_profile_versions_decode_without_scenario_variables() {
    use super::profile_index_encoding::decode_stored_profile_index;
    use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
    use openzt2_interface_state::{
        input::input_types::GameActionInputBindings,
        settings::{audio_settings_types::AudioSettings, display_settings_types::DisplaySettings},
    };
    for version in 1_u32..=3 {
        let id = create_test_profile_identifier(1);
        let mut bytes = b"OZT2PROF".to_vec();
        bytes.extend_from_slice(&version.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&id.0);
        bytes.extend_from_slice(&[0; 32]);
        bytes.extend_from_slice(&id.0);
        bytes.extend_from_slice(&10_u64.to_le_bytes());
        bytes.extend_from_slice(&[0; 16]);
        bytes.extend_from_slice(&[1, 0]);
        if version >= 2 {
            for count in [7_u32, 2, 1, 3, 1] {
                bytes.extend_from_slice(&count.to_le_bytes());
            }
        }
        if version >= 3 {
            for settings in [
                serde_json::to_vec(&AudioSettings::default()).unwrap(),
                serde_json::to_vec(&GameActionInputBindings::default()).unwrap(),
                serde_json::to_vec(&DisplaySettings::default()).unwrap(),
                serde_json::to_vec(&GraphicsSettings::default()).unwrap(),
            ] {
                bytes.extend_from_slice(&(settings.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&settings);
            }
        }
        bytes.extend_from_slice(&3_u16.to_le_bytes());
        bytes.extend_from_slice(b"Ava");
        let mut checksum = blake3::Hasher::new();
        checksum.update(&bytes[..32]);
        checksum.update(&bytes[64..]);
        bytes[32..64].copy_from_slice(checksum.finalize().as_bytes());
        let (profiles, selected) = decode_stored_profile_index(&bytes)
            .unwrap_or_else(|error| panic!("profile version {version}: {error:?}"));
        assert_eq!(selected, Some(id));
        assert_eq!(profiles[0].record.profile_display_name, "Ava");
        assert_eq!(profiles[0].scenario_variables.iter().len(), 0);
        assert_eq!(
            profiles[0].challenge_progress.all,
            if version == 1 { 0 } else { 7 }
        );
    }
}

#[test]
fn independently_bounded_variables_round_trip_across_profiles() {
    use super::profile_index_encoding::{decode_stored_profile_index, encode_stored_profile_index};
    let records: Vec<_> = (1..=3)
        .map(|id| ProfileRecord {
            profile_identifier: create_test_profile_identifier(id),
            profile_display_name: format!("profile {id}"),
            last_played_unix_timestamp_milliseconds: 0,
        })
        .collect();
    let bytes = encode_profile_index_for_test(&records, None).unwrap();
    let (mut profiles, _) = decode_stored_profile_index(&bytes).unwrap();
    let value = "v".repeat(1024);
    for profile in &mut profiles {
        let mut count = 0;
        while profile
            .scenario_variables
            .set(AssetId::from_key(&count.to_string()), Some(&value))
            .is_ok()
        {
            count += 1;
        }
        assert_eq!(count, 62);
        // Replacing a value does not consume an additional entry's byte budget.
        profile
            .scenario_variables
            .set(AssetId::from_key("0"), Some(&value))
            .unwrap();
    }
    let bytes = encode_stored_profile_index(&profiles, None).unwrap();
    assert!(bytes.len() > 128 * 1024);
    let (decoded, _) = decode_stored_profile_index(&bytes).unwrap();
    for (expected, actual) in profiles.iter().zip(decoded) {
        assert_eq!(actual.scenario_variables, expected.scenario_variables);
    }
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
                options: openzt2_interface_state::persistence::profile_types::ProfileOptions {
                    profile_identifier,
                    locale_identifier: AssetId::default(),
                    controller_input_enabled: true,
                },
                challenge_progress: Default::default(),
                scenario_variables: Default::default(),
                audio: Default::default(),
                bindings: Default::default(),
                display: Default::default(),
                graphics: Default::default(),
                online_messages:
                    openzt2_interface_state::settings::online_message_policy_types::OnlineMessagePolicy {
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
