use openzt2_game_data::AssetId;

use super::profile_types::ProfileRecord;

pub(super) fn generate_unique_profile_identifier_and_timestamp_from_display_name_and_existing_profiles(
    profile_display_name: &str,
    existing_profile_records: &[ProfileRecord],
) -> (AssetId, u64) {
    let current_unix_time_milliseconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64);
    let mut collision_nonce = 0_u32;

    loop {
        let mut profile_identifier_hasher = blake3::Hasher::new();
        profile_identifier_hasher.update(profile_display_name.as_bytes());
        profile_identifier_hasher.update(&current_unix_time_milliseconds.to_le_bytes());
        profile_identifier_hasher.update(&collision_nonce.to_le_bytes());
        let profile_identifier_hash = profile_identifier_hasher.finalize();
        let mut profile_identifier_bytes = [0; 16];
        profile_identifier_bytes.copy_from_slice(&profile_identifier_hash.as_bytes()[..16]);
        let profile_identifier = AssetId(profile_identifier_bytes);

        if profile_identifier != AssetId::default()
            && !existing_profile_records
                .iter()
                .any(|profile_record| profile_record.profile_identifier == profile_identifier)
        {
            return (profile_identifier, current_unix_time_milliseconds);
        }
        collision_nonce = collision_nonce.wrapping_add(1);
    }
}
