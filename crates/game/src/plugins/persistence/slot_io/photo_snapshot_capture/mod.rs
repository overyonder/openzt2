use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;
use super::{
    photo_snapshot_types::{PhotoChallengeProgressSnapshotRecord, PhotoSnapshotRecord},
    world_snapshot_capture_system_parameters::WorldSnapshotCaptureQueries,
};

pub(super) fn capture_sorted_photo_and_challenge_progress_snapshot_records_from_live_world(
    world_snapshot_capture_queries: &WorldSnapshotCaptureQueries,
) -> Result<
    (
        Vec<PhotoSnapshotRecord>,
        Vec<PhotoChallengeProgressSnapshotRecord>,
    ),
    WorldSnapshotPersistenceFailure,
> {
    let mut photo_snapshot_records = Vec::with_capacity(
        world_snapshot_capture_queries
            .photos_with_album_membership
            .iter()
            .len(),
    );
    for (
        persistent_identifier,
        photo,
        album_membership,
        optional_album_order,
        optional_challenge_photo_files,
    ) in &world_snapshot_capture_queries.photos_with_album_membership
    {
        let album_persistent_identifier = world_snapshot_capture_queries
            .photo_album_persistent_identifiers
            .get(album_membership.0)
            .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let photo_image = world_snapshot_capture_queries
            .loaded_photo_images
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
    }
    photo_snapshot_records.sort_unstable_by_key(|record| record.persistent_identifier.0);

    let mut challenge_progress_snapshot_records = Vec::with_capacity(
        world_snapshot_capture_queries
            .photo_challenges_with_progress
            .iter()
            .len(),
    );
    for (photo_challenge, photo_challenge_progress) in
        &world_snapshot_capture_queries.photo_challenges_with_progress
    {
        let completed_by_photo_persistent_identifier = photo_challenge_progress
            .completed_by
            .and_then(|photo_entity| {
                world_snapshot_capture_queries
                    .entities_with_persistent_identifiers
                    .get(photo_entity)
                    .ok()
                    .copied()
            });
        if photo_challenge_progress.completed && completed_by_photo_persistent_identifier.is_none()
        {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        }
        challenge_progress_snapshot_records.push(PhotoChallengeProgressSnapshotRecord {
            challenge_row_index: photo_challenge.row,
            completed_by_photo_persistent_identifier,
            rating_stars: photo_challenge_progress.rating_stars,
        });
    }
    challenge_progress_snapshot_records.sort_unstable_by_key(|record| record.challenge_row_index);

    Ok((photo_snapshot_records, challenge_progress_snapshot_records))
}
