use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffJobRequest {
    pub kind: StaffJobKind,
    pub target: Entity,
    pub urgency: u32,
    /// Lowercase authored request token. Distinct tokens on one target are
    /// distinct jobs; a missing token cannot select a behavior task.
    pub token: Option<AssetId>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct HireStaffRequest {
    pub role: AssetId,
    pub position: Vec3,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FireStaffRequest {
    pub staff: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffHired {
    pub staff: Entity,
    pub role: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffFired {
    pub staff: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffJobCompleted {
    pub staff: Entity,
    pub job: Entity,
    pub kind: StaffJobKind,
    pub target: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffJobCancelled {
    pub kind: StaffJobKind,
    pub target: Entity,
    /// The cancelled job's authored request token so its natural owner can
    /// requeue the same request instead of an unscoped kind.
    pub token: Option<AssetId>,
}

/// Requests termination when a job's natural owner rejects its live target or
/// its claimed authored effect.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelStaffJobRequest {
    pub job: Entity,
}
