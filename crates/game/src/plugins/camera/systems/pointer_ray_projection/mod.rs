use bevy::{camera::RenderTarget, diagnostic::FrameCount, prelude::*};

use crate::plugins::input::input_types::PrimaryPointerInputState;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::verification::verification_staff_gate_topology_diagnostics::StaffGateTopologyObservation;

use super::super::{
    camera_runtime_state_types::ZooCamera, world_pointer_ray_types::WorldPointerRay,
};

pub(in crate::plugins::camera) fn project_pointer_ray(
    pointer: Res<PrimaryPointerInputState>,
    cameras: Query<
        (
            Entity,
            &Camera,
            &GlobalTransform,
            Option<&WorldMember>,
            Option<&RenderTarget>,
        ),
        With<ZooCamera>,
    >,
    mut ray: ResMut<WorldPointerRay>,
    frame: Option<Res<FrameCount>>,
    mut observation: Option<ResMut<StaffGateTopologyObservation>>,
) {
    let camera = cameras.single().ok();
    let next = camera
        .filter(|_| pointer.available)
        .and_then(|(_, camera, transform, ..)| {
            let scale = camera.target_scaling_factor()?;
            let viewport_position = pointer.screen / scale;
            camera
                .logical_viewport_rect()
                .filter(|viewport| viewport.contains(viewport_position))?;
            camera.viewport_to_world(transform, viewport_position).ok()
        });
    if let Some(observation) = observation.as_mut() {
        observation.record_anchor_hover_projection(
            frame.as_ref().map(|frame| frame.0),
            &pointer,
            next,
            camera,
        );
    }
    if ray.0 != next {
        ray.0 = next;
    }
}
