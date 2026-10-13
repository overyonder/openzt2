use bevy::prelude::*;

use super::super::camera_control_message_types::CameraMoved;
use super::super::camera_runtime_state_types::OverheadRig;
use super::super::camera_runtime_state_types::ZooCamera;

pub(crate) fn report_camera_motion(
    cameras: Query<(&Transform, &OverheadRig), (With<ZooCamera>, Changed<Transform>)>,
    mut moved: MessageWriter<CameraMoved>,
) {
    for (transform, overhead) in &cameras {
        moved.write(CameraMoved {
            eye: transform.translation,
            focus: transform.translation + transform.forward() * overhead.distance,
        });
    }
}
