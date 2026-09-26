use openzt2_game_data::AssetId;

use crate::plugins::{
    input::input_types::{GameActionInputBindings, InputChord},
    progression::profile_challenge_types::ProfileChallengeCompletionCounts,
    settings::{
        audio_settings_types::AudioSettings, display_settings_types::DisplaySettings,
        graphics_settings_types::GraphicsSettings,
    },
};

use super::{
    durable_filesystem_operations::read_file_with_maximum_byte_count,
    profile_persistence_types::StoredProfileRecordAndSettings,
    profile_types::{ProfileFailure, ProfileOptions, ProfileRecord},
};

pub(super) const MAXIMUM_PROFILE_COUNT: usize = 1024;
pub(super) const MAXIMUM_PROFILE_DISPLAY_NAME_BYTE_COUNT: usize = 64;
const PROFILE_INDEX_MAGIC_BYTES: [u8; 8] = *b"OZT2PROF";
const PROFILE_INDEX_FORMAT_VERSION: u32 = 3;
pub(super) const MAXIMUM_PROFILE_INDEX_BYTE_COUNT: usize = 128 * 1024;
const MAXIMUM_ENCODED_PROFILE_SETTINGS_BYTE_COUNT: usize = 64 * 1024;

#[cfg(test)]
pub(super) fn encode_profile_index_for_test(
    profile_records: &[ProfileRecord],
    selected_profile_identifier: Option<AssetId>,
) -> Result<Vec<u8>, ProfileFailure> {
    let stored_profile_records_and_settings: Vec<_> = profile_records
        .iter()
        .cloned()
        .map(|profile_record| StoredProfileRecordAndSettings {
            options: ProfileOptions {
                profile_identifier: profile_record.profile_identifier,
                locale_identifier: AssetId::default(),
                controller_input_enabled: true,
            },
            challenge_progress: ProfileChallengeCompletionCounts::default(),
            audio: AudioSettings::default(),
            bindings: GameActionInputBindings::default(),
            display: DisplaySettings::default(),
            graphics: GraphicsSettings::default(),
            record: profile_record,
        })
        .collect();
    encode_stored_profile_index(
        &stored_profile_records_and_settings,
        selected_profile_identifier,
    )
}

pub(super) fn encode_stored_profile_index(
    stored_profile_records_and_settings: &[StoredProfileRecordAndSettings],
    selected_profile_identifier: Option<AssetId>,
) -> Result<Vec<u8>, ProfileFailure> {
    if stored_profile_records_and_settings.len() > MAXIMUM_PROFILE_COUNT {
        return Err(ProfileFailure::CapacityExceeded);
    }
    let total_profile_display_name_byte_count =
        stored_profile_records_and_settings.iter().try_fold(
            0_usize,
            |accumulated_profile_display_name_byte_count, stored_profile_record_and_settings| {
                let profile_display_name_byte_count = stored_profile_record_and_settings
                    .record
                    .profile_display_name
                    .len();
                if profile_display_name_byte_count == 0
                    || profile_display_name_byte_count > MAXIMUM_PROFILE_DISPLAY_NAME_BYTE_COUNT
                {
                    return Err(ProfileFailure::InvalidName);
                }
                accumulated_profile_display_name_byte_count
                    .checked_add(profile_display_name_byte_count)
                    .ok_or(ProfileFailure::CapacityExceeded)
            },
        )?;
    let required_profile_index_byte_capacity = 8
        + 4
        + 4
        + 16
        + 32
        + stored_profile_records_and_settings.len() * (16 + 8 + 16 + 1 + 1 + 20 + 16 + 2)
        + total_profile_display_name_byte_count;
    if required_profile_index_byte_capacity > MAXIMUM_PROFILE_INDEX_BYTE_COUNT {
        return Err(ProfileFailure::CapacityExceeded);
    }
    let mut profile_index_bytes = Vec::with_capacity(required_profile_index_byte_capacity);
    profile_index_bytes.extend_from_slice(&PROFILE_INDEX_MAGIC_BYTES);
    profile_index_bytes.extend_from_slice(&PROFILE_INDEX_FORMAT_VERSION.to_le_bytes());
    profile_index_bytes
        .extend_from_slice(&(stored_profile_records_and_settings.len() as u32).to_le_bytes());
    profile_index_bytes.extend_from_slice(&selected_profile_identifier.unwrap_or_default().0);
    let profile_index_checksum_byte_offset = profile_index_bytes.len();
    profile_index_bytes.extend_from_slice(&[0; 32]);
    let mut previous_profile_identifier_bytes = None;
    for stored_profile_record_and_settings in stored_profile_records_and_settings {
        if stored_profile_record_and_settings
            .options
            .profile_identifier
            != stored_profile_record_and_settings.record.profile_identifier
            || previous_profile_identifier_bytes.is_some_and(
                |previous_profile_identifier_bytes: [u8; 16]| {
                    previous_profile_identifier_bytes
                        >= stored_profile_record_and_settings
                            .record
                            .profile_identifier
                            .0
                },
            )
        {
            return Err(ProfileFailure::CorruptIndex);
        }
        validate_profile_settings_ranges_and_input_binding_uniqueness(
            stored_profile_record_and_settings.audio,
            &stored_profile_record_and_settings.bindings,
            stored_profile_record_and_settings.display,
            stored_profile_record_and_settings.graphics,
        )?;
        previous_profile_identifier_bytes = Some(
            stored_profile_record_and_settings
                .record
                .profile_identifier
                .0,
        );
        profile_index_bytes.extend_from_slice(
            &stored_profile_record_and_settings
                .record
                .profile_identifier
                .0,
        );
        profile_index_bytes.extend_from_slice(
            &stored_profile_record_and_settings
                .record
                .last_played_unix_timestamp_milliseconds
                .to_le_bytes(),
        );
        profile_index_bytes.extend_from_slice(
            &stored_profile_record_and_settings
                .options
                .locale_identifier
                .0,
        );
        profile_index_bytes.push(u8::from(
            stored_profile_record_and_settings
                .options
                .controller_input_enabled,
        ));
        profile_index_bytes.push(0);
        for challenge_completion_count in [
            stored_profile_record_and_settings.challenge_progress.all,
            stored_profile_record_and_settings.challenge_progress.photo,
            stored_profile_record_and_settings
                .challenge_progress
                .marine_animal,
            stored_profile_record_and_settings
                .challenge_progress
                .marine_show,
            stored_profile_record_and_settings
                .challenge_progress
                .endangered,
        ] {
            profile_index_bytes.extend_from_slice(&challenge_completion_count.to_le_bytes());
        }
        append_length_prefixed_canonical_json_profile_settings_value(
            &mut profile_index_bytes,
            &stored_profile_record_and_settings.audio,
        )?;
        append_length_prefixed_canonical_json_profile_settings_value(
            &mut profile_index_bytes,
            &stored_profile_record_and_settings.bindings,
        )?;
        append_length_prefixed_canonical_json_profile_settings_value(
            &mut profile_index_bytes,
            &stored_profile_record_and_settings.display,
        )?;
        append_length_prefixed_canonical_json_profile_settings_value(
            &mut profile_index_bytes,
            &stored_profile_record_and_settings.graphics,
        )?;
        profile_index_bytes.extend_from_slice(
            &(stored_profile_record_and_settings
                .record
                .profile_display_name
                .len() as u16)
                .to_le_bytes(),
        );
        profile_index_bytes.extend_from_slice(
            stored_profile_record_and_settings
                .record
                .profile_display_name
                .as_bytes(),
        );
        if profile_index_bytes.len() > MAXIMUM_PROFILE_INDEX_BYTE_COUNT {
            return Err(ProfileFailure::CapacityExceeded);
        }
    }
    let profile_index_checksum = calculate_profile_index_checksum(&profile_index_bytes);
    profile_index_bytes
        [profile_index_checksum_byte_offset..profile_index_checksum_byte_offset + 32]
        .copy_from_slice(&profile_index_checksum);
    Ok(profile_index_bytes)
}

#[cfg(test)]
pub(super) fn decode_profile_index_for_test(
    profile_index_bytes: &[u8],
) -> Result<(Vec<ProfileRecord>, Option<AssetId>), ProfileFailure> {
    let (stored_profile_records_and_settings, selected_profile_identifier) =
        decode_stored_profile_index(profile_index_bytes)?;
    Ok((
        stored_profile_records_and_settings
            .into_iter()
            .map(|stored_profile_record_and_settings| stored_profile_record_and_settings.record)
            .collect(),
        selected_profile_identifier,
    ))
}

pub(super) fn decode_stored_profile_index(
    profile_index_bytes: &[u8],
) -> Result<(Vec<StoredProfileRecordAndSettings>, Option<AssetId>), ProfileFailure> {
    const PROFILE_INDEX_HEADER_BYTE_COUNT: usize = 8 + 4 + 4 + 16 + 32;
    if profile_index_bytes.len() < PROFILE_INDEX_HEADER_BYTE_COUNT
        || profile_index_bytes[0..8] != PROFILE_INDEX_MAGIC_BYTES
    {
        return Err(ProfileFailure::CorruptIndex);
    }
    let profile_index_format_version =
        read_little_endian_u32_from_profile_index(profile_index_bytes, 8)?;
    if !(1..=PROFILE_INDEX_FORMAT_VERSION).contains(&profile_index_format_version) {
        return Err(ProfileFailure::CorruptIndex);
    }
    let stored_profile_count =
        read_little_endian_u32_from_profile_index(profile_index_bytes, 12)? as usize;
    if stored_profile_count > MAXIMUM_PROFILE_COUNT {
        return Err(ProfileFailure::CapacityExceeded);
    }
    let mut selected_profile_identifier_bytes = [0; 16];
    selected_profile_identifier_bytes.copy_from_slice(&profile_index_bytes[16..32]);
    let stored_profile_index_checksum_bytes = &profile_index_bytes[32..64];
    if calculate_profile_index_checksum(profile_index_bytes) != stored_profile_index_checksum_bytes
    {
        return Err(ProfileFailure::CorruptIndex);
    }
    let mut profile_index_byte_cursor = PROFILE_INDEX_HEADER_BYTE_COUNT;
    let mut decoded_stored_profile_records_and_settings = Vec::with_capacity(stored_profile_count);
    let mut previous_profile_identifier_bytes = None;
    for _ in 0..stored_profile_count {
        const FIXED_PROFILE_RECORD_BYTE_COUNT: usize = 42;
        let fixed_profile_record_end_byte_offset = profile_index_byte_cursor
            .checked_add(FIXED_PROFILE_RECORD_BYTE_COUNT)
            .ok_or(ProfileFailure::CapacityExceeded)?;
        if fixed_profile_record_end_byte_offset > profile_index_bytes.len() {
            return Err(ProfileFailure::CorruptIndex);
        }
        let mut profile_identifier_bytes = [0; 16];
        profile_identifier_bytes.copy_from_slice(
            &profile_index_bytes[profile_index_byte_cursor..profile_index_byte_cursor + 16],
        );
        profile_index_byte_cursor += 16;
        if profile_identifier_bytes == [0; 16]
            || previous_profile_identifier_bytes.is_some_and(
                |prior_profile_identifier_bytes: [u8; 16]| {
                    prior_profile_identifier_bytes >= profile_identifier_bytes
                },
            )
        {
            return Err(ProfileFailure::CorruptIndex);
        }
        previous_profile_identifier_bytes = Some(profile_identifier_bytes);
        let last_played_unix_timestamp_milliseconds = read_little_endian_u64_from_profile_index(
            profile_index_bytes,
            profile_index_byte_cursor,
        )?;
        profile_index_byte_cursor += 8;
        let mut locale_identifier_bytes = [0; 16];
        locale_identifier_bytes.copy_from_slice(
            &profile_index_bytes[profile_index_byte_cursor..profile_index_byte_cursor + 16],
        );
        profile_index_byte_cursor += 16;
        let controller_input_enabled = match profile_index_bytes[profile_index_byte_cursor] {
            0 => false,
            1 => true,
            _ => return Err(ProfileFailure::CorruptIndex),
        };
        profile_index_byte_cursor += 1;
        if profile_index_bytes[profile_index_byte_cursor] != 0 {
            return Err(ProfileFailure::CorruptIndex);
        }
        profile_index_byte_cursor += 1;
        let profile_challenge_completion_counts = if profile_index_format_version == 1 {
            ProfileChallengeCompletionCounts::default()
        } else {
            let decoded_profile_challenge_completion_counts = ProfileChallengeCompletionCounts {
                all: read_little_endian_u32_from_profile_index(
                    profile_index_bytes,
                    profile_index_byte_cursor,
                )?,
                photo: read_little_endian_u32_from_profile_index(
                    profile_index_bytes,
                    profile_index_byte_cursor + 4,
                )?,
                marine_animal: read_little_endian_u32_from_profile_index(
                    profile_index_bytes,
                    profile_index_byte_cursor + 8,
                )?,
                marine_show: read_little_endian_u32_from_profile_index(
                    profile_index_bytes,
                    profile_index_byte_cursor + 12,
                )?,
                endangered: read_little_endian_u32_from_profile_index(
                    profile_index_bytes,
                    profile_index_byte_cursor + 16,
                )?,
            };
            profile_index_byte_cursor += 20;
            decoded_profile_challenge_completion_counts
        };
        let (audio_settings, mut game_action_input_bindings, display_settings, graphics_settings) =
            if profile_index_format_version < 3 {
                (
                    AudioSettings::default(),
                    GameActionInputBindings::default(),
                    DisplaySettings::default(),
                    GraphicsSettings::default(),
                )
            } else {
                (
                    decode_length_prefixed_canonical_json_profile_settings_value(
                        profile_index_bytes,
                        &mut profile_index_byte_cursor,
                    )?,
                    decode_length_prefixed_canonical_json_profile_settings_value(
                        profile_index_bytes,
                        &mut profile_index_byte_cursor,
                    )?,
                    decode_length_prefixed_canonical_json_profile_settings_value(
                        profile_index_bytes,
                        &mut profile_index_byte_cursor,
                    )?,
                    decode_length_prefixed_canonical_json_profile_settings_value(
                        profile_index_bytes,
                        &mut profile_index_byte_cursor,
                    )?,
                )
            };
        validate_profile_settings_ranges_and_input_binding_uniqueness(
            audio_settings,
            &game_action_input_bindings,
            display_settings,
            graphics_settings,
        )?;
        migrate_legacy_controller_defaults(&mut game_action_input_bindings);
        let profile_display_name_byte_count = read_little_endian_u16_from_profile_index(
            profile_index_bytes,
            profile_index_byte_cursor,
        )? as usize;
        profile_index_byte_cursor += 2;
        if profile_display_name_byte_count == 0
            || profile_display_name_byte_count > MAXIMUM_PROFILE_DISPLAY_NAME_BYTE_COUNT
        {
            return Err(ProfileFailure::CorruptIndex);
        }
        let profile_display_name_end_byte_offset = profile_index_byte_cursor
            .checked_add(profile_display_name_byte_count)
            .ok_or(ProfileFailure::CapacityExceeded)?;
        let profile_display_name = std::str::from_utf8(
            profile_index_bytes
                .get(profile_index_byte_cursor..profile_display_name_end_byte_offset)
                .ok_or(ProfileFailure::CorruptIndex)?,
        )
        .map_err(|_| ProfileFailure::CorruptIndex)?;
        if profile_display_name.chars().any(char::is_control) {
            return Err(ProfileFailure::CorruptIndex);
        }
        let profile_identifier = AssetId(profile_identifier_bytes);
        decoded_stored_profile_records_and_settings.push(StoredProfileRecordAndSettings {
            record: ProfileRecord {
                profile_identifier,
                profile_display_name: profile_display_name.to_owned(),
                last_played_unix_timestamp_milliseconds,
            },
            options: ProfileOptions {
                profile_identifier,
                locale_identifier: AssetId(locale_identifier_bytes),
                controller_input_enabled,
            },
            challenge_progress: profile_challenge_completion_counts,
            audio: audio_settings,
            bindings: game_action_input_bindings,
            display: display_settings,
            graphics: graphics_settings,
        });
        profile_index_byte_cursor = profile_display_name_end_byte_offset;
    }
    if profile_index_byte_cursor != profile_index_bytes.len() {
        return Err(ProfileFailure::CorruptIndex);
    }
    let selected_profile_identifier = (selected_profile_identifier_bytes != [0; 16])
        .then_some(AssetId(selected_profile_identifier_bytes));
    if selected_profile_identifier.is_some_and(|selected_profile_identifier| {
        !decoded_stored_profile_records_and_settings.iter().any(
            |stored_profile_record_and_settings| {
                stored_profile_record_and_settings.record.profile_identifier
                    == selected_profile_identifier
            },
        )
    }) {
        return Err(ProfileFailure::CorruptIndex);
    }
    Ok((
        decoded_stored_profile_records_and_settings,
        selected_profile_identifier,
    ))
}

fn append_length_prefixed_canonical_json_profile_settings_value(
    profile_index_bytes: &mut Vec<u8>,
    profile_settings_value: &impl serde::Serialize,
) -> Result<(), ProfileFailure> {
    let canonical_json_profile_settings_bytes =
        serde_json::to_vec(profile_settings_value).map_err(|_| ProfileFailure::CorruptIndex)?;
    if canonical_json_profile_settings_bytes.len() > MAXIMUM_ENCODED_PROFILE_SETTINGS_BYTE_COUNT {
        return Err(ProfileFailure::CapacityExceeded);
    }
    let canonical_json_profile_settings_byte_count =
        u32::try_from(canonical_json_profile_settings_bytes.len())
            .map_err(|_| ProfileFailure::CapacityExceeded)?;
    profile_index_bytes
        .extend_from_slice(&canonical_json_profile_settings_byte_count.to_le_bytes());
    profile_index_bytes.extend_from_slice(&canonical_json_profile_settings_bytes);
    Ok(())
}

fn decode_length_prefixed_canonical_json_profile_settings_value<
    T: serde::de::DeserializeOwned + serde::Serialize,
>(
    profile_index_bytes: &[u8],
    profile_index_byte_cursor: &mut usize,
) -> Result<T, ProfileFailure> {
    let canonical_json_profile_settings_byte_count =
        read_little_endian_u32_from_profile_index(profile_index_bytes, *profile_index_byte_cursor)?
            as usize;
    *profile_index_byte_cursor = (*profile_index_byte_cursor)
        .checked_add(4)
        .ok_or(ProfileFailure::CapacityExceeded)?;
    if canonical_json_profile_settings_byte_count > MAXIMUM_ENCODED_PROFILE_SETTINGS_BYTE_COUNT {
        return Err(ProfileFailure::CapacityExceeded);
    }
    let canonical_json_profile_settings_end_byte_offset = (*profile_index_byte_cursor)
        .checked_add(canonical_json_profile_settings_byte_count)
        .ok_or(ProfileFailure::CapacityExceeded)?;
    let canonical_json_profile_settings_bytes = profile_index_bytes
        .get(*profile_index_byte_cursor..canonical_json_profile_settings_end_byte_offset)
        .ok_or(ProfileFailure::CorruptIndex)?;
    let profile_settings_value = serde_json::from_slice(canonical_json_profile_settings_bytes)
        .map_err(|_| ProfileFailure::CorruptIndex)?;
    if serde_json::to_vec(&profile_settings_value).map_err(|_| ProfileFailure::CorruptIndex)?
        != canonical_json_profile_settings_bytes
    {
        return Err(ProfileFailure::CorruptIndex);
    }
    *profile_index_byte_cursor = canonical_json_profile_settings_end_byte_offset;
    Ok(profile_settings_value)
}

fn validate_profile_settings_ranges_and_input_binding_uniqueness(
    audio_settings: AudioSettings,
    game_action_input_bindings: &GameActionInputBindings,
    display_settings: DisplaySettings,
    graphics_settings: GraphicsSettings,
) -> Result<(), ProfileFailure> {
    if [
        audio_settings.master_volume_percent,
        audio_settings.music_volume_percent,
        audio_settings.two_dimensional_effect_volume_percent,
        audio_settings.three_dimensional_effect_volume_percent,
    ]
    .into_iter()
    .any(|volume_percent| volume_percent > 100)
        || display_settings.width == 0
        || display_settings.height == 0
        || !(500..=2000).contains(&display_settings.ui_scale_permille)
        || !matches!(graphics_settings.multisample_count, 1 | 2 | 4 | 8)
        || graphics_settings.skybox_quality > 2
    {
        return Err(ProfileFailure::CorruptIndex);
    }

    let defaults = GameActionInputBindings::default();
    let len = game_action_input_bindings.entries.len();
    if (len != 14 && len != defaults.entries.len())
        || game_action_input_bindings
            .entries
            .iter()
            .zip(defaults.entries.iter())
            .any(|(binding, default)| binding.action != default.action)
    {
        return Err(ProfileFailure::CorruptIndex);
    }
    for (input_binding_index, input_binding) in
        game_action_input_bindings.entries.iter().enumerate()
    {
        if input_binding.alternate.is_some_and(|alternate_input_chord| {
            crate::plugins::input::input_binding_reconfiguration_operations::game_action_input_bindings_conflict(
                input_binding.action,
                input_binding.primary,
                input_binding.action,
                alternate_input_chord,
            )
        }) {
            return Err(ProfileFailure::CorruptIndex);
        }
        let input_chords = [Some(input_binding.primary), input_binding.alternate]
            .into_iter()
            .flatten();
        for input_chord in input_chords {
            if game_action_input_bindings.entries[input_binding_index + 1..]
                .iter()
                .any(|other_input_binding| {
                    crate::plugins::input::input_binding_reconfiguration_operations::game_action_input_bindings_conflict(
                        input_binding.action,
                        input_chord,
                        other_input_binding.action,
                        other_input_binding.primary,
                    )
                        || other_input_binding.alternate.is_some_and(
                            |alternate_input_chord| {
                                crate::plugins::input::input_binding_reconfiguration_operations::game_action_input_bindings_conflict(
                                    input_binding.action,
                                    input_chord,
                                    other_input_binding.action,
                                    alternate_input_chord,
                                )
                            },
                        )
                })
            {
                return Err(ProfileFailure::CorruptIndex);
            }
        }
    }
    Ok(())
}

fn calculate_profile_index_checksum(profile_index_bytes: &[u8]) -> [u8; 32] {
    let mut profile_index_checksum_hasher = blake3::Hasher::new();
    profile_index_checksum_hasher.update(&profile_index_bytes[..32]);
    profile_index_checksum_hasher.update(&profile_index_bytes[64..]);
    *profile_index_checksum_hasher.finalize().as_bytes()
}

pub(super) fn read_stored_profile_index_or_empty(
    profile_index_file_path: &std::path::Path,
) -> Result<(Vec<StoredProfileRecordAndSettings>, Option<AssetId>), ProfileFailure> {
    match read_file_with_maximum_byte_count(
        profile_index_file_path,
        MAXIMUM_PROFILE_INDEX_BYTE_COUNT,
    ) {
        Ok(profile_index_bytes) => decode_stored_profile_index(&profile_index_bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok((Vec::new(), None)),
        Err(_) => Err(ProfileFailure::Io),
    }
}

fn read_little_endian_u16_from_profile_index(
    profile_index_bytes: &[u8],
    profile_index_byte_offset: usize,
) -> Result<u16, ProfileFailure> {
    let little_endian_integer_bytes = profile_index_bytes
        .get(profile_index_byte_offset..profile_index_byte_offset + 2)
        .ok_or(ProfileFailure::CorruptIndex)?;
    Ok(u16::from_le_bytes([
        little_endian_integer_bytes[0],
        little_endian_integer_bytes[1],
    ]))
}

fn read_little_endian_u32_from_profile_index(
    profile_index_bytes: &[u8],
    profile_index_byte_offset: usize,
) -> Result<u32, ProfileFailure> {
    let little_endian_integer_bytes = profile_index_bytes
        .get(profile_index_byte_offset..profile_index_byte_offset + 4)
        .ok_or(ProfileFailure::CorruptIndex)?;
    Ok(u32::from_le_bytes(
        little_endian_integer_bytes
            .try_into()
            .map_err(|_| ProfileFailure::CorruptIndex)?,
    ))
}

fn read_little_endian_u64_from_profile_index(
    profile_index_bytes: &[u8],
    profile_index_byte_offset: usize,
) -> Result<u64, ProfileFailure> {
    let little_endian_integer_bytes = profile_index_bytes
        .get(profile_index_byte_offset..profile_index_byte_offset + 8)
        .ok_or(ProfileFailure::CorruptIndex)?;
    Ok(u64::from_le_bytes(
        little_endian_integer_bytes
            .try_into()
            .map_err(|_| ProfileFailure::CorruptIndex)?,
    ))
}

/// Upgrade the retired default controller layout while retaining customized
/// chords. A new default that conflicts with a customization stays unbound.
fn migrate_legacy_controller_defaults(bindings: &mut GameActionInputBindings) {
    if bindings.entries.len() != 14 {
        return;
    }
    use bevy::prelude::GamepadButton as Pad;
    let legacy = [
        Pad::DPadUp,
        Pad::DPadDown,
        Pad::DPadLeft,
        Pad::DPadRight,
        Pad::South,
        Pad::East,
        Pad::Start,
        Pad::Select,
        Pad::West,
        Pad::North,
        Pad::LeftTrigger,
        Pad::RightTrigger,
        Pad::RightTrigger2,
        Pad::LeftTrigger2,
    ];
    let defaults = GameActionInputBindings::default();
    let mut entries = bindings.entries.to_vec();
    let replace: Vec<bool> = entries
        .iter()
        .zip(legacy)
        .map(|(binding, old)| binding.alternate == Some(InputChord::Gamepad(old)))
        .collect();
    for (binding, replace) in entries.iter_mut().zip(&replace) {
        if *replace {
            binding.alternate = None;
        }
    }
    for (index, default) in defaults.entries.iter().enumerate() {
        let candidate = if index < 14 {
            if !replace[index] {
                continue;
            }
            default.alternate
        } else {
            Some(default.primary)
        };
        let available = candidate.filter(|chord| !entries.iter().any(|other| {
            crate::plugins::input::input_binding_reconfiguration_operations::game_action_input_bindings_conflict(default.action, *chord, other.action, other.primary)
                || other.alternate.is_some_and(|alternate| crate::plugins::input::input_binding_reconfiguration_operations::game_action_input_bindings_conflict(default.action, *chord, other.action, alternate))
        }));
        if index < 14 {
            entries[index].alternate = available;
        } else {
            entries.push(crate::plugins::input::input_types::GameActionInputBinding {
                primary: available.unwrap_or(InputChord::Unbound),
                ..*default
            });
        }
    }
    bindings.entries = entries.into_boxed_slice();
}
