//! Direct controller actions routed to existing gameplay message owners.

use super::input_types::{ActionRequest, ActionSource, GameAction};
use crate::plugins::{
    camera::camera_runtime_state_types::ZooCamera,
    construction::construction_interaction_types::CancelConstruction,
    photos::photo_capture_types::{CapturePhotoRequest, PhotoMode},
};
use bevy::prelude::*;

pub(super) fn dispatch_controller_gameplay_actions(
    mut actions: MessageReader<ActionRequest>,
    cameras: Query<Entity, (With<ZooCamera>, With<PhotoMode>)>,
    mut photos: MessageWriter<CapturePhotoRequest>,
    mut cancel: MessageWriter<CancelConstruction>,
) {
    for request in actions
        .read()
        .filter(|request| matches!(request.source, ActionSource::Controller(_)))
    {
        match request.action {
            GameAction::UseObject => {
                if let Ok(camera) = cameras.single() {
                    photos.write(CapturePhotoRequest { camera });
                }
            }
            GameAction::SecondaryPointer => {
                cancel.write(CancelConstruction);
            }
            _ => {}
        }
    }
}
