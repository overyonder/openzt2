//! Data and fixed rules for photo albums, camera-roll membership, and album UI state.

use bevy::prelude::*;
use openzt2_game_data::AssetId as DomainAssetId;

pub const DEFAULT_PHOTO_ALBUM_NAME_KEY: &str = "ztphotomode:default_album_name";

#[derive(Component)]
pub struct DefaultPhotoAlbumStorageInitializedForWorld {
    pub album: Entity,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefaultPhotoStorageInitializationSet;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhotoAlbum {
    pub profile: DomainAssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlbumMember(pub Entity);

/// Marks a captured photo as transient camera-roll content until the player
/// moves it into a persistent album.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraRollPhoto;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhotoAlbumOrder(pub u64);

/// Explicit player-data I/O request; persistence owns serialization and paths.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportPhotoAlbum {
    pub album: Entity,
}
