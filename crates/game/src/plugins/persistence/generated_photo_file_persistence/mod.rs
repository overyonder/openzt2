//! Asynchronous durable storage for captured challenge photos.

use std::io;

use bevy::{
    prelude::*,
    tasks::{block_on, poll_once, IoTaskPool, Task},
};
use image::{codecs::jpeg::JpegEncoder, RgbaImage};

use crate::plugins::{
    photos::photo_capture_types::Photo, world_spawn::persistent_id_types::PersistentId,
};

use super::{
    durable_filesystem_operations::{
        atomically_write_and_sync_file, durably_delete_file_if_present,
    },
    generated_photo_persistence_types::{
        ChallengePhotoJpegCopyPersisted, ChallengePhotoJpegCopyPersistenceFailed,
        DeleteGeneratedPhotoImageAndChallengeCopies, DurableChallengePhotoFileIdentifiers,
        GeneratedPhotoImageAndChallengeCopiesDeleted,
        GeneratedPhotoImageAndChallengeCopiesDeletionFailed, PersistChallengePhotoJpegCopy,
    },
    persistence_filesystem_paths::PersistenceFilesystemPaths,
    profile_types::ProfileOptions,
};

#[derive(Component)]
pub(super) struct ChallengePhotoJpegCopyWriteTask {
    photo_entity: Entity,
    challenge_definition_identifier: openzt2_game_data::AssetId,
    /// Keeps the image alive until its file write completes.
    _generated_image_handle: Handle<Image>,
    jpeg_file_write_task: Task<io::Result<u64>>,
}

pub(super) fn begin_challenge_photo_writes(
    mut persistence_requests: MessageReader<PersistChallengePhotoJpegCopy>,
    directories: Option<Res<PersistenceFilesystemPaths>>,
    profile: Res<ProfileOptions>,
    photos: Query<(&PersistentId, &Photo)>,
    images: Res<Assets<Image>>,
    mut commands: Commands,
    mut persistence_failures: MessageWriter<ChallengePhotoJpegCopyPersistenceFailed>,
) {
    for persistence_request in persistence_requests.read() {
        let Some(directories) = directories.as_deref() else {
            persistence_failures.write(ChallengePhotoJpegCopyPersistenceFailed {
                photo_entity: persistence_request.photo_entity,
                challenge_definition_identifier: persistence_request
                    .challenge_definition_identifier,
            });
            continue;
        };
        let Ok((persistent_identifier, photo)) = photos.get(persistence_request.photo_entity)
        else {
            persistence_failures.write(ChallengePhotoJpegCopyPersistenceFailed {
                photo_entity: persistence_request.photo_entity,
                challenge_definition_identifier: persistence_request
                    .challenge_definition_identifier,
            });
            continue;
        };
        let Some(image) = images.get(&photo.image) else {
            persistence_failures.write(ChallengePhotoJpegCopyPersistenceFailed {
                photo_entity: persistence_request.photo_entity,
                challenge_definition_identifier: persistence_request
                    .challenge_definition_identifier,
            });
            continue;
        };
        let Some(pixels) = image.data.clone() else {
            persistence_failures.write(ChallengePhotoJpegCopyPersistenceFailed {
                photo_entity: persistence_request.photo_entity,
                challenge_definition_identifier: persistence_request
                    .challenge_definition_identifier,
            });
            continue;
        };
        let size = image.texture_descriptor.size;
        let Some(expected_bytes) = size
            .width
            .checked_mul(size.height)
            .and_then(|pixels| pixels.checked_mul(4))
            .map(|bytes| bytes as usize)
        else {
            persistence_failures.write(ChallengePhotoJpegCopyPersistenceFailed {
                photo_entity: persistence_request.photo_entity,
                challenge_definition_identifier: persistence_request
                    .challenge_definition_identifier,
            });
            continue;
        };
        if size.depth_or_array_layers != 1 || pixels.len() != expected_bytes {
            persistence_failures.write(ChallengePhotoJpegCopyPersistenceFailed {
                photo_entity: persistence_request.photo_entity,
                challenge_definition_identifier: persistence_request
                    .challenge_definition_identifier,
            });
            continue;
        }

        let path = directories.challenge_photo_jpeg_file_path(
            &profile.profile_identifier.0,
            &persistence_request.challenge_definition_identifier.0,
            persistent_identifier.0,
        );
        commands.spawn(ChallengePhotoJpegCopyWriteTask {
            photo_entity: persistence_request.photo_entity,
            challenge_definition_identifier: persistence_request.challenge_definition_identifier,
            _generated_image_handle: photo.image.clone(),
            jpeg_file_write_task: IoTaskPool::get().spawn(async move {
                let bytes = encode_challenge_photo_as_jpeg(size.width, size.height, pixels)?;
                let len = bytes.len() as u64;
                atomically_write_and_sync_file(&path, &bytes)?;
                Ok(len)
            }),
        });
    }
}

pub(super) fn complete_challenge_photo_writes(
    mut pending_write_tasks: Query<(Entity, &mut ChallengePhotoJpegCopyWriteTask)>,
    mut persisted_challenge_photo_identifiers: Query<&mut DurableChallengePhotoFileIdentifiers>,
    mut commands: Commands,
    mut completed_writes: MessageWriter<ChallengePhotoJpegCopyPersisted>,
    mut persistence_failures: MessageWriter<ChallengePhotoJpegCopyPersistenceFailed>,
) {
    for (write_task_entity, mut pending_write) in &mut pending_write_tasks {
        let Some(write_result) = block_on(poll_once(&mut pending_write.jpeg_file_write_task))
        else {
            continue;
        };
        match write_result {
            Ok(encoded_jpeg_byte_count) => {
                if let Ok(mut persisted_identifiers) =
                    persisted_challenge_photo_identifiers.get_mut(pending_write.photo_entity)
                {
                    let identifiers = &mut persisted_identifiers.challenge_definition_identifiers;
                    if !identifiers.contains(&pending_write.challenge_definition_identifier) {
                        identifiers.push(pending_write.challenge_definition_identifier);
                        identifiers.sort_unstable_by_key(|identifier| identifier.0);
                    }
                } else {
                    commands.entity(pending_write.photo_entity).insert(
                        DurableChallengePhotoFileIdentifiers {
                            challenge_definition_identifiers: vec![
                                pending_write.challenge_definition_identifier,
                            ],
                        },
                    );
                }
                completed_writes.write(ChallengePhotoJpegCopyPersisted {
                    photo_entity: pending_write.photo_entity,
                    challenge_definition_identifier: pending_write.challenge_definition_identifier,
                    encoded_jpeg_byte_count,
                });
            }
            Err(_) => {
                persistence_failures.write(ChallengePhotoJpegCopyPersistenceFailed {
                    photo_entity: pending_write.photo_entity,
                    challenge_definition_identifier: pending_write.challenge_definition_identifier,
                });
            }
        }
        commands.entity(write_task_entity).despawn();
    }
}

#[derive(Component)]
pub(super) struct GeneratedPhotoImageAndChallengeCopiesDeleteTask {
    photo_entity: Entity,
    generated_image_handle: Handle<Image>,
    challenge_copy_file_delete_task: Task<io::Result<()>>,
}

#[derive(Component)]
pub(super) struct GeneratedPhotoImageAndChallengeCopiesDeletionPending;

pub(super) fn begin_generated_image_deletes(
    mut deletion_requests: MessageReader<DeleteGeneratedPhotoImageAndChallengeCopies>,
    directories: Option<Res<PersistenceFilesystemPaths>>,
    profile: Res<ProfileOptions>,
    photos: Query<
        (&PersistentId, Option<&DurableChallengePhotoFileIdentifiers>),
        Without<GeneratedPhotoImageAndChallengeCopiesDeletionPending>,
    >,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut completed_deletions: MessageWriter<GeneratedPhotoImageAndChallengeCopiesDeleted>,
    mut deletion_failures: MessageWriter<GeneratedPhotoImageAndChallengeCopiesDeletionFailed>,
) {
    for deletion_request in deletion_requests.read() {
        let Ok((persistent_identifier, challenge_photo_files)) =
            photos.get(deletion_request.photo_entity)
        else {
            deletion_failures.write(GeneratedPhotoImageAndChallengeCopiesDeletionFailed {
                photo_entity: deletion_request.photo_entity,
            });
            continue;
        };
        if challenge_photo_files
            .is_none_or(|files| files.challenge_definition_identifiers.is_empty())
        {
            images.remove(deletion_request.generated_image_handle.id());
            commands.entity(deletion_request.photo_entity).despawn();
            completed_deletions.write(GeneratedPhotoImageAndChallengeCopiesDeleted {
                photo_entity: deletion_request.photo_entity,
            });
            continue;
        }
        let Some(directories) = directories.as_deref() else {
            deletion_failures.write(GeneratedPhotoImageAndChallengeCopiesDeletionFailed {
                photo_entity: deletion_request.photo_entity,
            });
            continue;
        };
        let challenge_photo_file_paths = challenge_photo_files
            .into_iter()
            .flat_map(|files| files.challenge_definition_identifiers.iter())
            .map(|challenge_definition_identifier| {
                directories.challenge_photo_jpeg_file_path(
                    &profile.profile_identifier.0,
                    &challenge_definition_identifier.0,
                    persistent_identifier.0,
                )
            })
            .collect::<Vec<_>>();
        commands
            .entity(deletion_request.photo_entity)
            .insert(GeneratedPhotoImageAndChallengeCopiesDeletionPending);
        commands.spawn(GeneratedPhotoImageAndChallengeCopiesDeleteTask {
            photo_entity: deletion_request.photo_entity,
            generated_image_handle: deletion_request.generated_image_handle.clone(),
            challenge_copy_file_delete_task: IoTaskPool::get().spawn(async move {
                challenge_photo_file_paths
                    .iter()
                    .try_for_each(|path| durably_delete_file_if_present(path))
            }),
        });
    }
}

pub(super) fn complete_generated_image_deletes(
    mut pending_delete_tasks: Query<(Entity, &mut GeneratedPhotoImageAndChallengeCopiesDeleteTask)>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut completed_deletions: MessageWriter<GeneratedPhotoImageAndChallengeCopiesDeleted>,
    mut deletion_failures: MessageWriter<GeneratedPhotoImageAndChallengeCopiesDeletionFailed>,
) {
    for (delete_task_entity, mut pending_delete) in &mut pending_delete_tasks {
        let Some(delete_result) = block_on(poll_once(
            &mut pending_delete.challenge_copy_file_delete_task,
        )) else {
            continue;
        };
        if delete_result.is_ok() {
            images.remove(pending_delete.generated_image_handle.id());
            commands.entity(pending_delete.photo_entity).despawn();
            completed_deletions.write(GeneratedPhotoImageAndChallengeCopiesDeleted {
                photo_entity: pending_delete.photo_entity,
            });
        } else {
            commands
                .entity(pending_delete.photo_entity)
                .remove::<GeneratedPhotoImageAndChallengeCopiesDeletionPending>();
            deletion_failures.write(GeneratedPhotoImageAndChallengeCopiesDeletionFailed {
                photo_entity: pending_delete.photo_entity,
            });
        }
        commands.entity(delete_task_entity).despawn();
    }
}

fn encode_challenge_photo_as_jpeg(
    image_width_pixels: u32,
    image_height_pixels: u32,
    rgba8_pixel_bytes: Vec<u8>,
) -> io::Result<Vec<u8>> {
    let payload_bytes = image_width_pixels
        .checked_mul(image_height_pixels)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| io::Error::other("challenge photo dimensions overflow"))?;
    if rgba8_pixel_bytes.len() != payload_bytes as usize {
        return Err(io::Error::other(
            "challenge photo is not tightly packed RGBA8",
        ));
    }
    let image = RgbaImage::from_raw(image_width_pixels, image_height_pixels, rgba8_pixel_bytes)
        .ok_or_else(|| io::Error::other("invalid challenge photo pixel layout"))?;
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 90)
        .encode_image(&image)
        .map_err(io::Error::other)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::encode_challenge_photo_as_jpeg;

    #[test]
    fn challenge_photo_encoder_rejects_bad_layout_and_writes_jpeg() {
        assert!(encode_challenge_photo_as_jpeg(2, 1, vec![0; 7]).is_err());
        let bytes =
            encode_challenge_photo_as_jpeg(2, 1, vec![255; 8]).expect("valid tightly packed RGBA8");
        assert_eq!(&bytes[..2], &[0xff, 0xd8]);
        assert_eq!(&bytes[bytes.len() - 2..], &[0xff, 0xd9]);
    }
}
