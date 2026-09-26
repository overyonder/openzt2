use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StaffJobRequest {
    pub(crate) kind: StaffJobKind,
    pub(crate) target: Entity,
    pub(crate) urgency: u16,
    /// Lowercase authored request token. Distinct tokens on one target are
    /// distinct jobs; a missing token cannot select a behavior task.
    pub(crate) token: Option<AssetId>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct HireStaffRequest {
    pub(crate) role: AssetId,
    pub(crate) position: Vec3,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FireStaffRequest {
    pub(crate) staff: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StaffHired {
    pub(crate) staff: Entity,
    pub(crate) role: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StaffFired {
    pub(crate) staff: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StaffJobCompleted {
    pub(crate) staff: Entity,
    pub(crate) job: Entity,
    pub(crate) kind: StaffJobKind,
    pub(crate) target: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StaffJobCancelled {
    pub(crate) kind: StaffJobKind,
    pub(crate) target: Entity,
    /// The cancelled job's authored request token so its natural owner can
    /// requeue the same request instead of an unscoped kind.
    pub(crate) token: Option<AssetId>,
}

/// Requests termination when a job's natural owner rejects its live target or
/// its claimed authored effect.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CancelStaffJobRequest {
    pub job: Entity,
}

/// Reports application of the authored staff cleanliness policy to a tank.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TankWaterCleaned {
    pub(crate) staff: Entity,
    pub(crate) tank: Entity,
    pub(crate) policy: AssetId,
    pub(crate) quality_permille: u16,
    pub(crate) localization_key: AssetId,
}
