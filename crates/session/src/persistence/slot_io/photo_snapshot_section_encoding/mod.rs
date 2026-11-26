use bevy::prelude::{Quat, Vec3};
use openzt2_game_data::AssetId;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;

use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use super::photo_snapshot_types::{PhotoChallengeProgressSnapshotRecord, PhotoSnapshotRecord};

const PHOTO_CAPTION_PRESENT: u8 = 1 << 7;
const CHALLENGE_HALF_STARS: u8 = 1 << 6;
const PHOTO_TAG: u8 = 1;
const PHOTO_CHALLENGE_TAG: u8 = 2;
const CHALLENGE_COMPLETED_WITHOUT_ALBUM_PHOTO: u8 = 1 << 7;

pub(super) fn append_encoded_photo_snapshot_section(
    bytes: &mut Vec<u8>,
    photos: &[PhotoSnapshotRecord],
    challenges: &[PhotoChallengeProgressSnapshotRecord],
) {
    for photo in photos {
        bytes.push(
            PHOTO_TAG
                | if photo.caption.is_some() {
                    PHOTO_CAPTION_PRESENT
                } else {
                    0
                },
        );
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
        if let Some(caption) = &photo.caption {
            bytes.extend_from_slice(&(caption.len() as u32).to_le_bytes());
            bytes.extend_from_slice(caption.as_bytes());
        }
    }
    for challenge in challenges {
        bytes.push(PHOTO_CHALLENGE_TAG);
        bytes.extend_from_slice(&challenge.challenge_definition_identifier.0);
        bytes.extend_from_slice(
            &challenge
                .completed_by_photo_persistent_identifier
                .unwrap_or(PersistentId(0))
                .0
                .to_le_bytes(),
        );
        let mut rating = if challenge.rating_half_stars % 2 == 0 {
            challenge.rating_half_stars / 2
        } else {
            challenge.rating_half_stars | CHALLENGE_HALF_STARS
        };
        if challenge.completed && challenge.completed_by_photo_persistent_identifier.is_none() {
            rating |= CHALLENGE_COMPLETED_WITHOUT_ALBUM_PHOTO;
        }
        bytes.push(rating);
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
        let encoded_tag = *payload
            .get(cursor)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        let tag = encoded_tag & !PHOTO_CAPTION_PRESENT;
        let has_caption = encoded_tag != tag;
        if tag < last_tag || (has_caption && tag != PHOTO_TAG) {
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
            let caption = if has_caption {
                let length = u32::from_le_bytes(
                    payload
                        .get(cursor..cursor.saturating_add(4))
                        .and_then(|bytes| bytes.try_into().ok())
                        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
                ) as usize;
                cursor += 4;
                let caption = payload
                    .get(cursor..cursor.saturating_add(length))
                    .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
                cursor += length;
                Some(
                    std::str::from_utf8(caption)
                        .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
                        .to_owned(),
                )
            } else {
                None
            };
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
                caption,
                challenge_photo_file_identifiers: challenge_files,
            });
        } else if tag == PHOTO_CHALLENGE_TAG {
            let record = payload
                .get(cursor..cursor.saturating_add(25))
                .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
            let challenge_definition_identifier = AssetId(
                record[0..16]
                    .try_into()
                    .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
            );
            let completed = PersistentId(u64::from_le_bytes(
                record[16..24].try_into().unwrap_or([0; 8]),
            ));
            let completed_without_photo = record[24] & CHALLENGE_COMPLETED_WITHOUT_ALBUM_PHOTO != 0;
            let rating =
                record[24] & !(CHALLENGE_COMPLETED_WITHOUT_ALBUM_PHOTO | CHALLENGE_HALF_STARS);
            let rating_half_stars = if record[24] & CHALLENGE_HALF_STARS == 0 {
                rating * 2
            } else {
                rating
            };
            cursor += 25;
            if rating_half_stars > 10
                || (completed_without_photo && completed.0 != 0)
                || challenges
                    .last()
                    .is_some_and(|prior: &PhotoChallengeProgressSnapshotRecord| {
                        prior.challenge_definition_identifier >= challenge_definition_identifier
                    })
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            challenges.push(PhotoChallengeProgressSnapshotRecord {
                challenge_definition_identifier,
                completed: completed.0 != 0 || completed_without_photo,
                completed_by_photo_persistent_identifier: (completed.0 != 0).then_some(completed),
                rating_half_stars,
            });
        } else {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
    }
    (cursor == payload.len())
        .then_some((photos, challenges))
        .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
}
