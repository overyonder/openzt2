//! Direct controller actions routed to existing gameplay message owners.

use super::input_types::ActionRequest;
use super::input_types::ActionSource;
use super::input_types::GameAction;
use bevy::prelude::*;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::photos::photo_capture_types::CapturePhotoRequest;
use openzt2_interface_state::photos::photo_capture_types::PhotoMode;
use openzt2_zoo_state::construction::construction_interaction_types::CancelConstruction;

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
