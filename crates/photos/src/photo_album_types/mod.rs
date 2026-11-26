//! Data and fixed rules for photo albums, camera-roll membership, and album UI state.

use bevy::prelude::*;

pub(super) const PHOTO_ALBUM_PAGE_SIZE: u32 = 8;

pub(super) const PHOTO_CAMERA_ROLL_CAPACITY: u32 = 24;

pub(super) const INITIAL_PHOTO_ALBUM_SPREADS: u32 = 5;
pub(super) const MAX_PHOTO_ALBUM_SPREADS: u32 = 100;

#[derive(Resource, Default)]
pub struct PhotoAlbumProfileOptions {
    pub default_spreads: Option<u32>,
}

impl PhotoAlbumProfileOptions {
    pub(super) fn get_default_spreads(&self) -> Option<u32> {
        self.default_spreads
            .map(|spreads| spreads.clamp(1, MAX_PHOTO_ALBUM_SPREADS))
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeletePhotoRequest {
    pub photo: Entity,
}

#[derive(Component)]
#[relationship(relationship_target = WorldActivePhotoAlbum)]
pub(super) struct ActivePhotoAlbum(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ActivePhotoAlbum)]
pub(super) struct WorldActivePhotoAlbum(Entity);

#[derive(Component)]
#[relationship(relationship_target = WorldPhotoCameraRoll)]
pub(super) struct PhotoCameraRoll(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = PhotoCameraRoll, linked_spawn)]
pub(super) struct WorldPhotoCameraRoll(Entity);

#[derive(Component, Default, PartialEq, Eq)]
pub(super) struct PhotoPointerPress(pub Option<(Entity, Entity)>);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PhotoAlbumPage(pub u32);

#[derive(Component, Default, PartialEq, Eq)]
pub(super) struct SelectedAlbumPhoto(pub Option<(Entity, Entity)>);

impl SelectedAlbumPhoto {
    pub(super) fn in_album(&self, album: Entity) -> Option<Entity> {
        self.0
            .filter(|(source, _)| *source == album)
            .map(|(_, photo)| photo)
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EnlargedAlbumPhoto(pub Entity);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PhotoMove {
    pub photo: Entity,
    pub from: Entity,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PhotoMoveTarget(pub u8);

/// The active move originated from a held exposure rather than the grab button.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PhotoPointerDrag;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PhotoAlbumCapacityPages(pub u32);

/// Presentation reference from an authored exposure row to its canonical
/// photo entity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PhotoCameraRollRow(pub Entity);

/// Presentation reference from an authored album-list row to its canonical
/// album entity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PhotoAlbumChoiceRow(pub Entity);
