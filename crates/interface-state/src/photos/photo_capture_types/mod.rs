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

/// Prevents simultaneous captures of the same presented frame.
#[derive(Resource, Debug, Default)]
pub struct PhotoReadback {
    pub busy: bool,
}
