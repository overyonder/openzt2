//! Creation of default photo storage and deletion of stored photo entities.

use openzt2_interface_state::photos::photo_album_types::DEFAULT_PHOTO_ALBUM_NAME_KEY;
use openzt2_interface_state::photos::photo_album_types::DefaultPhotoAlbum;
use openzt2_interface_state::photos::photo_album_types::DefaultPhotoAlbumStorageInitializedForWorld;
use openzt2_interface_state::photos::photo_album_types::PhotoAlbum;
use openzt2_interface_state::photos::photo_capture_types::Photo;

use bevy::prelude::*;

use openzt2_game_assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};
use openzt2_interface_state::persistence::generated_photo_persistence_types::DeleteGeneratedPhotoImageAndChallengeCopies;
use openzt2_interface_state::persistence::profile_types::ProfileOptions;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::{
    photo_album_types::{
        ActivePhotoAlbum, DeletePhotoRequest, INITIAL_PHOTO_ALBUM_SPREADS, PhotoAlbumCapacityPages,
        PhotoAlbumPage, PhotoCameraRoll,
    },
    photo_capture_types::{PhotoCaptureFailed, PhotoFailure},
    photo_configuration::{PhotoConfiguration, PhotoConfigurationSource},
};

pub(super) fn initialize_default_photo_album_and_camera_roll_for_loaded_worlds(
    profile: Res<ProfileOptions>,
    configuration: Res<PhotoConfigurationSource>,
    configurations: Res<Assets<PhotoConfiguration>>,
    server: Res<AssetServer>,
    mut roots: Query<
        (Entity, &mut PersistentIdAllocator),
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
    mut commands: Commands,
    mut failed: MessageWriter<PhotoCaptureFailed>,
) {
    if roots.is_empty() {
        return;
    }
    let album_spreads = if let Some(configuration) = configurations.get(&configuration.0) {
        configuration.album_spreads
    } else if matches!(
        server.load_state(configuration.0.id()),
        bevy::asset::LoadState::Failed(_)
    ) {
        INITIAL_PHOTO_ALBUM_SPREADS
    } else {
        return;
    };
    // Allocate at the post-import boundary; pending text must not move this ID.
    for (root, mut allocator) in &mut roots {
        let member = WorldMember { root };
        if let Some((album, ..)) = albums
            .iter()
            .find(|row| row.1.root == root && row.2.is_some() && row.3.is_none())
        {
            commands.entity(album).insert((
                DefaultPhotoAlbum,
                ActivePhotoAlbum,
                PhotoAlbumPage::default(),
                PhotoAlbumCapacityPages(album_spreads),
            ));
        } else {
            let id = match allocator.allocate() {
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
                member,
                id,
                ActivePhotoAlbum,
                DefaultPhotoAlbum,
                PhotoAlbumPage::default(),
                PhotoAlbumCapacityPages(album_spreads),
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

pub(super) fn initialize_localized_default_photo_album_names(
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    unnamed_albums: Query<Entity, (With<DefaultPhotoAlbum>, Without<Name>)>,
    mut commands: Commands,
) {
    if unnamed_albums.is_empty() {
        return;
    }
    let Some(default_name) = active_localization
        .borrow_loaded_localization_view(&localizations)
        .and_then(|localization| {
            localization.find_plain_localized_text(openzt2_game_data::AssetId::from_key(
                DEFAULT_PHOTO_ALBUM_NAME_KEY,
            ))
        })
    else {
        return;
    };
    for album in &unnamed_albums {
        commands
            .entity(album)
            .insert(Name::new(default_name.to_owned()));
    }
}

pub(super) fn delete_requested_photos_and_release_unpersisted_images(
    mut commands: Commands,
    mut requests: MessageReader<DeletePhotoRequest>,
    photos: Query<(Entity, &Photo, Option<&PersistentId>)>,
    mut images: ResMut<Assets<Image>>,
    mut generated_photo_deletion_requests: MessageWriter<
        DeleteGeneratedPhotoImageAndChallengeCopies,
    >,
) {
    for request in requests.read() {
        let Ok((entity, photo, persistent)) = photos.get(request.photo) else {
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
