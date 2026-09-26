use bevy::{
    asset::RenderAssetUsages,
    prelude::{Assets, Commands, Entity, Image, Query},
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::plugins::{
    photos::{
        photo_album_types::{AlbumMember, PhotoAlbumOrder},
        photo_capture_types::{Photo, PhotoSubjects},
        photo_challenge_types::{PhotoChallenge, PhotoChallengeProgress},
    },
    world_spawn::{
        persistent_id_types::PersistentId, persistent_id_types::PersistentIdAllocator,
        world_membership_types::WorldMember,
    },
};

use super::{
    super::{
        generated_photo_persistence_types::DurableChallengePhotoFileIdentifiers,
        persistence_failure_types::WorldSnapshotPersistenceFailure,
    },
    photo_snapshot_types::{PhotoChallengeProgressSnapshotRecord, PhotoSnapshotRecord},
};

pub(super) fn apply_photo_and_challenge_progress_snapshot_records_to_live_world(
    commands: &mut Commands,
    photo_snapshot_records: Vec<PhotoSnapshotRecord>,
    photo_challenge_progress_snapshot_records: Vec<PhotoChallengeProgressSnapshotRecord>,
    world_root_entity: Entity,
    entities_with_persistent_identifiers: &Query<(Entity, &PersistentId)>,
    persistent_identifier_allocator: &mut PersistentIdAllocator,
    loaded_photo_images: &mut Assets<Image>,
    photo_challenges_with_mutable_progress: &mut Query<(
        &PhotoChallenge,
        &mut PhotoChallengeProgress,
    )>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let mut restored_photo_entities = Vec::with_capacity(photo_snapshot_records.len());

    for photo_snapshot_record in photo_snapshot_records {
        let album_entity = entities_with_persistent_identifiers
            .iter()
            .find_map(|(entity, persistent_identifier)| {
                (*persistent_identifier == photo_snapshot_record.album_persistent_identifier)
                    .then_some(entity)
            })
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;

        persistent_identifier_allocator
            .reserve_imported(
                world_root_entity,
                photo_snapshot_record.persistent_identifier,
            )
            .map_err(|_| WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier)?;

        let mut photo_image = Image::new_uninit(
            Extent3d {
                width: photo_snapshot_record.image_width_pixels,
                height: photo_snapshot_record.image_height_pixels,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        photo_image.data = Some(photo_snapshot_record.rgba8_pixel_bytes);
        let photo_image_handle = loaded_photo_images.add(photo_image);
        let photo_entity = commands
            .spawn((
                photo_snapshot_record.persistent_identifier,
                WorldMember {
                    root: world_root_entity,
                },
                Photo {
                    image: photo_image_handle,
                    captured_tick: photo_snapshot_record.captured_simulation_tick,
                    camera_position: photo_snapshot_record.camera_world_position,
                    camera_rotation: photo_snapshot_record.camera_world_rotation,
                    score_milli: photo_snapshot_record.score_millipoints,
                },
                PhotoSubjects::default(),
                AlbumMember(album_entity),
                PhotoAlbumOrder(photo_snapshot_record.album_order),
                DurableChallengePhotoFileIdentifiers {
                    challenge_definition_identifiers: photo_snapshot_record
                        .challenge_photo_file_identifiers,
                },
            ))
            .id();
        restored_photo_entities.push((photo_snapshot_record.persistent_identifier, photo_entity));
    }

    for photo_challenge_progress_snapshot_record in photo_challenge_progress_snapshot_records {
        let (_, mut photo_challenge_progress) = photo_challenges_with_mutable_progress
            .iter_mut()
            .find(|(photo_challenge, _)| {
                photo_challenge.row == photo_challenge_progress_snapshot_record.challenge_row_index
            })
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;

        photo_challenge_progress.completed_by = match photo_challenge_progress_snapshot_record
            .completed_by_photo_persistent_identifier
        {
            Some(completing_photo_persistent_identifier) => Some(
                restored_photo_entities
                    .iter()
                    .find_map(|(persistent_identifier, entity)| {
                        (*persistent_identifier == completing_photo_persistent_identifier)
                            .then_some(*entity)
                    })
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?,
            ),
            None => None,
        };
        photo_challenge_progress.completed = photo_challenge_progress.completed_by.is_some();
        photo_challenge_progress.rating_stars =
            photo_challenge_progress_snapshot_record.rating_stars;
    }

    Ok(())
}
