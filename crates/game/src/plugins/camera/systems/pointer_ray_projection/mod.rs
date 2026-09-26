use bevy::prelude::*;

use crate::plugins::input::input_types::PrimaryPointerInputState;

use super::super::{
    camera_runtime_state_types::ZooCamera, world_pointer_ray_types::WorldPointerRay,
};

pub(in crate::plugins::camera) fn project_pointer_ray(
    pointer: Res<PrimaryPointerInputState>,
    cameras: Query<(&Camera, &GlobalTransform), With<ZooCamera>>,
    mut ray: ResMut<WorldPointerRay>,
) {
    let next = cameras
        .single()
        .ok()
        .filter(|_| pointer.available)
        .and_then(|(camera, transform)| {
            let scale = camera.target_scaling_factor()?;
            let viewport_position = pointer.screen / scale;
            camera
                .logical_viewport_rect()
                .filter(|viewport| viewport.contains(viewport_position))?;
            camera.viewport_to_world(transform, viewport_position).ok()
        });
    if ray.0 != next {
        ray.0 = next;
    }
}
