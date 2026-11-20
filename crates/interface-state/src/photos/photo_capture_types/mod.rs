//! Data used by photo capture, screenshot capture, and captured evidence.

use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhotoMode;

#[derive(Component, Debug, Clone)]
pub struct Photo {
    pub image: Handle<Image>,
    pub captured_tick: u64,
    pub camera_position: Vec3,
    pub camera_rotation: Quat,
    pub score_milli: i32,
}

#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct PhotoSubjects(pub Vec<Entity>);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapturePhotoRequest {
    pub camera: Entity,
}

/// A camera screenshot for export, without album membership or photo scoring.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureScreenshotRequest {
    pub camera: Entity,
}

/// The image produced by a screenshot request.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotCaptured {
    pub camera: Entity,
    pub image: Handle<Image>,
}

/// Links Bevy's in-flight album screenshot to its photo camera.
#[derive(Component)]
#[relationship(relationship_target = AlbumPhotoReadbacks)]
pub struct AlbumPhotoReadbackOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = AlbumPhotoReadbackOf, linked_spawn)]
pub struct AlbumPhotoReadbacks(Vec<Entity>);
