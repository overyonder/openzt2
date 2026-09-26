use bevy::prelude::{Quat, Vec3};
use openzt2_game_data::AssetId;

use crate::plugins::world_spawn::persistent_id_types::PersistentId;

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use super::photo_snapshot_types::{PhotoChallengeProgressSnapshotRecord, PhotoSnapshotRecord};

const PHOTO_TAG: u8 = 1;
const PHOTO_CHALLENGE_TAG: u8 = 2;

pub(super) fn append_encoded_photo_snapshot_section(
    bytes: &mut Vec<u8>,
    photos: &[PhotoSnapshotRecord],
    challenges: &[PhotoChallengeProgressSnapshotRecord],
) {
    for photo in photos {
        bytes.push(PHOTO_TAG);
        bytes.extend_from_slice(&photo.persistent_identifier.0.to_le_bytes());
        bytes.extend_from_slice(&photo.album_persistent_identifier.0.to_le_bytes());
        bytes.extend_from_slice(&photo.album_order.to_le_bytes());
        bytes.extend_from_slice(&photo.captured_simulation_tick.to_le_bytes());
        for value in photo.camera_world_position.to_array() {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        for value in photo.camera_world_rotation.to_array() {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        bytes.extend_from_slice(&photo.score_millipoints.to_le_bytes());
        bytes.extend_from_slice(&photo.image_width_pixels.to_le_bytes());
        bytes.extend_from_slice(&photo.image_height_pixels.to_le_bytes());
        bytes.extend_from_slice(&(photo.rgba8_pixel_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&photo.rgba8_pixel_bytes);
        bytes.extend_from_slice(
            &(photo.challenge_photo_file_identifiers.len() as u16).to_le_bytes(),
        );
        photo
            .challenge_photo_file_identifiers
            .iter()
            .for_each(|id| bytes.extend_from_slice(&id.0));
    }
    for challenge in challenges {
        bytes.push(PHOTO_CHALLENGE_TAG);
        bytes.extend_from_slice(&challenge.challenge_row_index.to_le_bytes());
        bytes.extend_from_slice(
            &challenge
                .completed_by_photo_persistent_identifier
                .unwrap_or(PersistentId(0))
                .0
                .to_le_bytes(),
        );
        bytes.push(challenge.rating_stars);
    }
}

pub(super) fn decode_and_validate_photo_snapshot_section(
    bytes: &[u8],
    range: WorldSnapshotSectionDirectoryEntry,
) -> Result<
    (
        Vec<PhotoSnapshotRecord>,
        Vec<PhotoChallengeProgressSnapshotRecord>,
    ),
    WorldSnapshotPersistenceFailure,
> {
    let payload = &bytes[range.payload_byte_range_within_container(bytes.len())?];
    let mut cursor = 0usize;
    let mut photos = Vec::new();
    let mut challenges = Vec::new();
    let mut last_tag = PHOTO_TAG;
    for _ in 0..range.encoded_record_count {
        let tag = *payload
            .get(cursor)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        if tag < last_tag {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        last_tag = tag;
        cursor += 1;
        if tag == PHOTO_TAG {
            let header = payload
                .get(cursor..cursor.saturating_add(72))
                .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
            let u64_at = |offset: usize| {
                u64::from_le_bytes(header[offset..offset + 8].try_into().unwrap_or([0; 8]))
            };
            let u32_at = |offset: usize| {
                u32::from_le_bytes(header[offset..offset + 4].try_into().unwrap_or([0; 4]))
            };
            let f32_at = |offset: usize| f32::from_bits(u32_at(offset));
            let id = PersistentId(u64_at(0));
            let album = PersistentId(u64_at(8));
            let order = u64_at(16);
            let captured_tick = u64_at(24);
            let camera_position = Vec3::new(f32_at(32), f32_at(36), f32_at(40));
            let camera_rotation = Quat::from_xyzw(f32_at(44), f32_at(48), f32_at(52), f32_at(56));
            let score_milli = i32::from_le_bytes(header[60..64].try_into().unwrap_or([0; 4]));
            let width = u32_at(64);
            let height = u32_at(68);
            cursor += 72;
            let len_bytes = payload
                .get(cursor..cursor + 4)
                .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
            let len = u32::from_le_bytes(len_bytes.try_into().unwrap_or([0; 4])) as usize;
            cursor += 4;
            let pixels = payload
                .get(cursor..cursor.saturating_add(len))
                .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
                .to_vec();
            cursor += len;
            let count = u16::from_le_bytes(
                payload
                    .get(cursor..cursor.saturating_add(2))
                    .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
                    .try_into()
                    .unwrap_or([0; 2]),
            ) as usize;
            cursor += 2;
            let challenge_bytes = payload
                .get(cursor..cursor.saturating_add(count.saturating_mul(16)))
                .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
            let challenge_files = challenge_bytes
                .chunks_exact(16)
                .map(|bytes| {
                    let mut id = [0; 16];
                    id.copy_from_slice(bytes);
                    AssetId(id)
                })
                .collect::<Vec<_>>();
            cursor += count.saturating_mul(16);
            let expected = width
                .checked_mul(height)
                .and_then(|n| n.checked_mul(4))
                .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?
                as usize;
            if id.0 == 0
                || album.0 == 0
                || width == 0
                || height == 0
                || len != expected
                || !camera_position.is_finite()
                || !camera_rotation.is_finite()
                || photos.last().is_some_and(|prior: &PhotoSnapshotRecord| {
                    prior.persistent_identifier.0 >= id.0
                })
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            photos.push(PhotoSnapshotRecord {
                persistent_identifier: id,
                album_persistent_identifier: album,
                album_order: order,
                captured_simulation_tick: captured_tick,
                camera_world_position: camera_position,
                camera_world_rotation: camera_rotation,
                score_millipoints: score_milli,
                image_width_pixels: width,
                image_height_pixels: height,
                rgba8_pixel_bytes: pixels,
                challenge_photo_file_identifiers: challenge_files,
            });
        } else if tag == PHOTO_CHALLENGE_TAG {
            let record = payload
                .get(cursor..cursor.saturating_add(13))
                .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
            let row = u32::from_le_bytes(record[0..4].try_into().unwrap_or([0; 4]));
            let completed = PersistentId(u64::from_le_bytes(
                record[4..12].try_into().unwrap_or([0; 8]),
            ));
            let rating_stars = record[12];
            cursor += 13;
            if rating_stars > 5
                || challenges
                    .last()
                    .is_some_and(|prior: &PhotoChallengeProgressSnapshotRecord| {
                        prior.challenge_row_index >= row
                    })
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            challenges.push(PhotoChallengeProgressSnapshotRecord {
                challenge_row_index: row,
                completed_by_photo_persistent_identifier: (completed.0 != 0).then_some(completed),
                rating_stars,
            });
        } else {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
    }
    (cursor == payload.len())
        .then_some((photos, challenges))
        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
}
