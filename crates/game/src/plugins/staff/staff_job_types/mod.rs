use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StaffJob {
    pub(crate) kind: StaffJobKind,
    pub(crate) target: Entity,
    pub(crate) urgency: u16,
    /// Lowercase authored request token. Distinct tokens on one target are
    /// distinct jobs; a missing token cannot select a behavior task.
    pub(crate) token: Option<AssetId>,
}

/// The winning shipped behavior-task declaration selected when this job is claimed.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct StaffJobBehaviorTask {
    pub(crate) document: Handle<BehaviorDocumentAsset>,
    /// Canonical source declaration identity; indexes can change on asset reload.
    pub(crate) task: AssetId,
    pub(crate) declaration_index: usize,
    /// Identity of the task started for this claim. Retained through terminal
    /// event settlement so a previous assignment cannot finish a replacement.
    pub(crate) execution_id: Option<u64>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JobClaim {
    pub(crate) staff: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CurrentJob {
    pub(crate) job: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JobProgress;

/// Native request-owned unsuccessful candidates, cleared by the manager's
/// authored badEntityCleanupInterval rather than by a fresh threshold crossing.
#[derive(Component, Debug, Default)]
pub(super) struct UnsuccessfulStaffJobCandidates {
    staff: Vec<Entity>,
}

impl UnsuccessfulStaffJobCandidates {
    pub(super) fn contains(&self, staff: Entity) -> bool {
        self.staff.contains(&staff)
    }

    pub(super) fn record(&mut self, staff: Entity) {
        if !self.contains(staff) {
            self.staff.push(staff);
        }
    }

    pub(super) fn clear(&mut self) {
        self.staff.clear();
    }
}
