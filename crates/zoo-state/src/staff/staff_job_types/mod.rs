use bevy::ecs::{lifecycle::HookContext, world::DeferredWorld};
use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffJob {
    pub kind: StaffJobKind,
    pub target: Entity,
    pub urgency: u32,
    /// Lowercase authored request token. Distinct tokens on one target are
    /// distinct jobs; a missing token cannot select a behavior task.
    pub token: Option<AssetId>,
}

/// Urgency of an authored staff request. A lower authored priority is served
/// first, and a request without a positive priority is never dispatched.
/// Positive floats have the same bit ordering as their numeric ordering.
pub fn authored_staff_request_urgency(priority: f32) -> Option<u32> {
    if !priority.is_finite() || priority <= 0.0 {
        return None;
    }
    Some(u32::MAX - priority.to_bits())
}

/// The point in a dirty tank where the water-cleaning manager asks a keeper
/// to work. It is the job's target: the keeper's own task moves there.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = TankCleaningSite)]
pub struct TankCleaningRequestSite {
    #[relationship]
    pub tank: Entity,
}

#[derive(Component)]
#[relationship_target(relationship = TankCleaningRequestSite, linked_spawn)]
pub struct TankCleaningSite(Entity);

/// The winning shipped behavior-task declaration selected when this job is claimed.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct StaffJobBehaviorTask {
    pub document: Handle<BehaviorDocumentAsset>,
    /// Canonical source declaration identity; indexes can change on asset reload.
    pub task: AssetId,
    pub declaration_index: usize,
    /// Identity of the task started for this claim. Retained through terminal
    /// event settlement so a previous assignment cannot finish a replacement.
    pub execution_id: Option<u64>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = CurrentJob)]
#[component(on_remove = clear_ended_staff_claim)]
pub struct JobClaim {
    #[relationship]
    pub staff: Entity,
}

#[derive(Component, Debug)]
#[relationship_target(relationship = JobClaim)]
pub struct CurrentJob {
    job: Entity,
}

impl CurrentJob {
    pub const fn job(&self) -> Entity {
        self.job
    }
}

fn clear_ended_staff_claim(mut world: DeferredWorld, context: HookContext) {
    world.commands().queue_silenced(move |world: &mut World| {
        if let Ok(mut job) = world.get_entity_mut(context.entity) {
            if !job.contains::<JobClaim>() {
                job.remove::<(StaffJobBehaviorTask, JobProgress)>();
            }
        }
    });
}

#[derive(Component)]
pub struct StaffJobCancellationPending;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobProgress;

/// Native request-owned unsuccessful candidates, cleared by the manager's
/// authored badEntityCleanupInterval rather than by a fresh threshold crossing.
#[derive(Component, Debug, Default)]
pub struct UnsuccessfulStaffJobCandidates {
    staff: Vec<Entity>,
}

impl UnsuccessfulStaffJobCandidates {
    pub fn contains(&self, staff: Entity) -> bool {
        self.staff.contains(&staff)
    }

    pub fn record(&mut self, staff: Entity) {
        if !self.contains(staff) {
            self.staff.push(staff);
        }
    }

    pub fn clear(&mut self) {
        self.staff.clear();
    }
}
