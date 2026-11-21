use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_interface_state::{
    persistence::generated_photo_persistence_types::DurableChallengePhotoFileIdentifiers,
    photos::{
        photo_album_types::{AlbumPhotos, CameraRollPhoto, PhotoAlbum, PhotoAlbumOrder},
        photo_capture_types::Photo,
        photo_challenge_types::{PhotoChallenge, PhotoChallengeProgress},
    },
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use std::collections::BTreeMap;

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;
use super::photo_snapshot_types::{PhotoChallengeProgressSnapshotRecord, PhotoSnapshotRecord};

#[derive(SystemParam)]
pub(in crate::persistence) struct PhotoSnapshotCaptureQueries<'w, 's> {
    albums: Query<'w, 's, (&'static PersistentId, &'static AlbumPhotos), With<PhotoAlbum>>,
    photos: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static Photo,
            Option<&'static PhotoAlbumOrder>,
            Option<&'static DurableChallengePhotoFileIdentifiers>,
        ),
        Without<CameraRollPhoto>,
    >,
    challenges: Query<'w, 's, (&'static PhotoChallenge, &'static PhotoChallengeProgress)>,
    images: Res<'w, Assets<Image>>,
}

pub(super) fn capture_sorted_photo_and_challenge_progress_snapshot_records_from_live_world(
    capture: &PhotoSnapshotCaptureQueries,
) -> Result<
    (
        Vec<PhotoSnapshotRecord>,
        Vec<PhotoChallengeProgressSnapshotRecord>,
    ),
    WorldSnapshotPersistenceFailure,
> {
    let mut photo_snapshot_records = Vec::new();
    let mut captured_photo_ids = BTreeMap::new();
    for (album_persistent_identifier, members) in &capture.albums {
        for (
            photo_entity,
            persistent_identifier,
            photo,
            optional_album_order,
            optional_challenge_photo_files,
        ) in members.iter().filter_map(|entity| {
            let (id, photo, order, files) = capture.photos.get(entity).ok()?;
            Some((entity, id, photo, order, files))
        }) {
            let photo_image = capture
                .images
                .get(&photo.image)
                .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
            let image_size = photo_image.texture_descriptor.size;
            let expected_rgba8_byte_count = image_size
                .width
                .checked_mul(image_size.height)
                .and_then(|pixel_count| pixel_count.checked_mul(4))
                .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
            let rgba8_pixel_bytes = photo_image
                .data
                .as_ref()
                .filter(|pixel_bytes| {
                    image_size.depth_or_array_layers == 1
                        && pixel_bytes.len() == expected_rgba8_byte_count as usize
                })
                .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;

            photo_snapshot_records.push(PhotoSnapshotRecord {
                persistent_identifier: *persistent_identifier,
                album_persistent_identifier: *album_persistent_identifier,
                album_order: optional_album_order.map_or(photo.captured_tick, |order| order.0),
                captured_simulation_tick: photo.captured_tick,
                camera_world_position: photo.camera_position,
                camera_world_rotation: photo.camera_rotation,
                score_millipoints: photo.score_milli,
                image_width_pixels: image_size.width,
                image_height_pixels: image_size.height,
                rgba8_pixel_bytes: rgba8_pixel_bytes.clone(),
                challenge_photo_file_identifiers: optional_challenge_photo_files.map_or_else(
                    Vec::new,
                    |challenge_photo_files| {
                        challenge_photo_files
                            .challenge_definition_identifiers
                            .clone()
                    },
                ),
            });
            captured_photo_ids.insert(photo_entity, *persistent_identifier);
        }
    }
    photo_snapshot_records.sort_unstable_by_key(|record| record.persistent_identifier.0);

    let mut challenge_progress_snapshot_records =
        Vec::with_capacity(capture.challenges.iter().len());
    for (photo_challenge, photo_challenge_progress) in &capture.challenges {
        let completed_by_photo_persistent_identifier = photo_challenge_progress
            .completed_by
            .and_then(|photo_entity| captured_photo_ids.get(&photo_entity).copied());
        challenge_progress_snapshot_records.push(PhotoChallengeProgressSnapshotRecord {
            challenge_definition_identifier: photo_challenge.id,
            completed: photo_challenge_progress.completed,
            completed_by_photo_persistent_identifier,
            rating_stars: photo_challenge_progress.rating_stars,
        });
    }
    challenge_progress_snapshot_records
        .sort_unstable_by_key(|record| record.challenge_definition_identifier);
    if challenge_progress_snapshot_records
        .windows(2)
        .any(|adjacent| {
            adjacent[0].challenge_definition_identifier
                == adjacent[1].challenge_definition_identifier
        })
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }

    Ok((photo_snapshot_records, challenge_progress_snapshot_records))
}
