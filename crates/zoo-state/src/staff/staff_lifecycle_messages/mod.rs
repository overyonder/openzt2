use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use super::staff_job_types::{StaffJob, StaffJobRouting};

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct StaffJobRequest {
    pub root: Entity,
    pub kind: Option<StaffJobKind>,
    pub target: Option<Entity>,
    pub point: Option<Vec3>,
    pub priority: f32,
    pub issued_at_seconds: f32,
    pub remaining_wait_seconds: f32,
    pub token: Option<AssetId>,
    pub routing: StaffJobRouting,
}

impl StaffJobRequest {
    /// The accepted job receives a persistent ID in request encounter order.
    pub fn into_job(self) -> Option<StaffJob> {
        let valid_destination = match self.point {
            Some(point) => point.is_finite(),
            None => self.target.is_some(),
        };
        if !valid_destination
            || !self.priority.is_finite()
            || self.priority <= 0.0
            || !self.issued_at_seconds.is_finite()
            || !self.remaining_wait_seconds.is_finite()
        {
            return None;
        }
        Some(StaffJob {
            kind: self.kind,
            target: self.target,
            point: self.point,
            priority: self.priority,
            issued_at_seconds: self.issued_at_seconds,
            remaining_wait_seconds: self.remaining_wait_seconds,
            first_attempt: true,
            token: self.token,
        })
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeginStaffHirePlacement(pub AssetId);

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct HireStaffRequest {
    pub role: AssetId,
    pub object: AssetId,
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
