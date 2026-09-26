//! Creation of default photo storage and deletion of stored photo entities.

use bevy::prelude::*;

use crate::{
    assets::localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    plugins::{
        persistence::{
            generated_photo_persistence_types::DeleteGeneratedPhotoImageAndChallengeCopies,
            profile_types::ProfileOptions,
        },
        world_spawn::{
            persistent_id_types::PersistentId, persistent_id_types::PersistentIdAllocator,
            world_load_completion_marker::WorldLoadCompleted, world_membership_types::WorldMember,
            world_membership_types::WorldRoot,
        },
    },
};

use super::{
    photo_album_types::{
        ActivePhotoAlbum, AlbumMember, DeletePhotoRequest, PhotoAlbum, PhotoAlbumCapacityPages,
        PhotoAlbumPage, PhotoCameraRoll, INITIAL_PHOTO_ALBUM_SPREADS,
    },
    photo_capture_types::{Photo, PhotoCaptureFailed, PhotoFailure},
};

#[derive(Component)]
pub(super) struct DefaultPhotoAlbumStorageInitializedForWorld;

pub(super) fn initialize_default_photo_album_and_camera_roll_for_loaded_worlds(
    profile: Res<ProfileOptions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    mut allocator: ResMut<PersistentIdAllocator>,
    roots: Query<
        Entity,
        (
            With<WorldRoot>,
            With<WorldLoadCompleted>,
            Without<DefaultPhotoAlbumStorageInitializedForWorld>,
        ),
    >,
    albums: Query<
        (
            Entity,
            &WorldMember,
            Option<&PersistentId>,
            Option<&PhotoCameraRoll>,
        ),
        With<PhotoAlbum>,
    >,
    names: Query<(), With<Name>>,
    mut commands: Commands,
    mut failed: MessageWriter<PhotoCaptureFailed>,
) {
    let Some(default_name) = active_localization
        .borrow_loaded_localization_view(&localizations)
        .and_then(|localization| {
            localization.find_plain_localized_text(openzt2_game_data::AssetId::from_key(
                "ztphotomode:default_album_name",
            ))
        })
    else {
        return;
    };
    for root in &roots {
        let member = WorldMember { root };
        if let Some((album, ..)) = albums
            .iter()
            .find(|row| row.1.root == root && row.2.is_some() && row.3.is_none())
        {
            commands.entity(album).insert((
                ActivePhotoAlbum,
                PhotoAlbumPage::default(),
                PhotoAlbumCapacityPages(INITIAL_PHOTO_ALBUM_SPREADS),
            ));
            if !names.contains(album) {
                commands
                    .entity(album)
                    .insert(Name::new(default_name.to_owned()));
            }
        } else {
            let id = match allocator.allocate(root) {
                Ok(id) => id,
                Err(reason) => {
                    failed.write(PhotoCaptureFailed {
                        reason: PhotoFailure::PersistentId(reason),
                    });
                    continue;
                }
            };
            commands.spawn((
                PhotoAlbum {
                    profile: profile.profile_identifier,
                },
                Name::new(default_name.to_owned()),
                member,
                id,
                ActivePhotoAlbum,
                PhotoAlbumPage::default(),
                PhotoAlbumCapacityPages(INITIAL_PHOTO_ALBUM_SPREADS),
            ));
        }
        if !albums
            .iter()
            .any(|row| row.1.root == root && row.3.is_some())
        {
            commands.spawn((
                PhotoAlbum {
                    profile: profile.profile_identifier,
                },
                member,
                PhotoCameraRoll,
            ));
        }
        commands
            .entity(root)
            .insert(DefaultPhotoAlbumStorageInitializedForWorld);
    }
}

pub(super) fn delete_requested_photos_and_release_unpersisted_images(
    mut commands: Commands,
    mut requests: MessageReader<DeletePhotoRequest>,
    photos: Query<(Entity, &Photo, &AlbumMember, Option<&PersistentId>)>,
    mut images: ResMut<Assets<Image>>,
    mut generated_photo_deletion_requests: MessageWriter<
        DeleteGeneratedPhotoImageAndChallengeCopies,
    >,
) {
    for request in requests.read() {
        let Ok((entity, photo, _, persistent)) = photos.get(request.photo) else {
            continue;
        };
        generated_photo_deletion_requests.write(DeleteGeneratedPhotoImageAndChallengeCopies {
            photo_entity: entity,
            generated_image_handle: photo.image.clone(),
        });
        if persistent.is_none() {
            images.remove(photo.image.id());
            commands.entity(entity).despawn();
        }
    }
}
