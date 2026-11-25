//! Gameplay impulse requests and failures.

use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct ApplyPhysicsImpulseRequest {
    pub(crate) entity: Entity,
    pub(crate) impulse_ns: Vec3,
    pub(crate) point_world: Vec3,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhysicsInteractionFailure {
    pub(crate) entity: Entity,
    pub(crate) reason: PhysicsInteractionFailureReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicsInteractionFailureReason {
    MissingBody,
    InvalidTarget,
}

#[derive(Component)]
pub(super) struct FallingReleasedObject {
    pub(super) downward_speed: f32,
    pub(super) resume_body: bool,
    pub(super) airborne_request: Option<usize>,
    pub(super) land_impact_clip: Option<String>,
    pub(super) water_impact_clip: Option<String>,
    pub(super) previous_footprint: Option<openzt2_zoo_state::placement::placed_object_types::PlacedObjectFootprintOccupancy>,
    pub(super) instruction: Option<openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorInstructionId>,
}
