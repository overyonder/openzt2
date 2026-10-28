use avian3d::prelude::RigidBody;
use bevy::prelude::*;

use crate::behavior_task_execution_types::BehaviorTaskReturnFrame;
use crate::locomotion::locomotion_types::{Destination, Docking};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaffTankGateStage {
    Approach,
    Cross,
    GetOnUp,
    ClimbUp,
    GetOffUp,
    GetOnDown,
    ClimbDown,
    GetOffDown,
}

#[derive(Component)]
pub struct StaffTankGateJourney {
    pub execution_id: u64,
    pub origin: BehaviorTaskReturnFrame,
    pub depth: usize,
    pub gate: Entity,
    pub goal: Destination,
    pub docking: Option<Docking>,
    pub approach_request: u64,
    pub entry: Vec3,
    pub exit: Vec3,
    pub wall_height: f32,
    pub stage: StaffTankGateStage,
    pub clip_request: Option<usize>,
    pub clip_completed: bool,
    pub failed: bool,
    pub previous_body: Option<RigidBody>,
}

#[derive(Component)]
pub struct StaffTankGateResume {
    pub execution_id: u64,
    pub origin: BehaviorTaskReturnFrame,
    pub depth: usize,
    pub goal: Destination,
    pub docking: Option<Docking>,
    pub body: Option<RigidBody>,
}

#[derive(Component)]
pub struct StaffExitTankGate {
    pub execution_id: u64,
    pub request_id: u64,
    pub origin: BehaviorTaskReturnFrame,
    pub depth: usize,
}
