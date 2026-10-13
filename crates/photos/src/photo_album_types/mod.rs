//! Data and fixed rules for photo albums, camera-roll membership, and album UI state.

use bevy::prelude::*;

pub(super) const PHOTO_ALBUM_PAGE_SIZE: u32 = 8;

pub(super) const PHOTO_CAMERA_ROLL_CAPACITY: u32 = 24;

pub(super) const INITIAL_PHOTO_ALBUM_SPREADS: u32 = 5;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeletePhotoRequest {
    pub photo: Entity,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ActivePhotoAlbum;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PhotoCameraRoll;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PhotoHelpShown;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PhotoAlbumPage(pub u32);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SelectedAlbumPhoto(pub Entity);

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

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PresentedPhotoCount(pub u32);

/// Presentation reference from an authored exposure row to its canonical
/// photo entity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PhotoCameraRollRow(pub Entity);

/// Presentation reference from an authored album-list row to its canonical
/// album entity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PhotoAlbumChoiceRow(pub Entity);
