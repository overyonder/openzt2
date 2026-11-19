use bevy::prelude::*;

use openzt2_interface_state::input::input_types::MousePointer;

use super::super::camera_runtime_state_types::ZooCamera;
use super::super::world_pointer_ray_types::WorldPointerRay;

pub fn project_pointer_ray(
    pointer: MousePointer,
    cameras: Query<(&Camera, &GlobalTransform), With<ZooCamera>>,
    mut ray: ResMut<WorldPointerRay>,
) {
    let next = cameras.single().ok().and_then(|(camera, transform)| {
        let viewport_position = pointer.location()?.position;
        camera
            .logical_viewport_rect()
            .filter(|viewport| viewport.contains(viewport_position))?;
        camera.viewport_to_world(transform, viewport_position).ok()
    });
    if ray.0 != next {
        ray.0 = next;
    }
}
