use std::cmp::Ordering;

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RankedStaffJobClaimCandidate {
    pub(super) job: Entity,
    pub(super) staff: Entity,
    pub(super) urgency: u32,
    pub(super) authored_priority: f32,
    pub(super) distance_squared: f32,
}

impl RankedStaffJobClaimCandidate {
    pub(super) fn claim_order(&self, other: &Self) -> Ordering {
        other
            .urgency
            .cmp(&self.urgency)
            .then_with(|| other.authored_priority.total_cmp(&self.authored_priority))
            .then_with(|| self.distance_squared.total_cmp(&other.distance_squared))
            .then_with(|| {
                (self.job.to_bits(), self.staff.to_bits())
                    .cmp(&(other.job.to_bits(), other.staff.to_bits()))
            })
    }
}
