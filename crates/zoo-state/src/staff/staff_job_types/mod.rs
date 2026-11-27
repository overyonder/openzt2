use bevy::ecs::{lifecycle::HookContext, world::DeferredWorld};
use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct StaffJob {
    /// Domain-specific settlement for jobs that need it.
    pub kind: Option<StaffJobKind>,
    /// Submitted identity for terminal receipts. Live target access uses StaffJobTarget.
    pub target: Option<Entity>,
    pub point: Option<Vec3>,
    pub priority: f32,
    pub issued_at_seconds: f32,
    pub remaining_wait_seconds: f32,
    /// Cleared when the first claim dispatches its task token.
    pub first_attempt: bool,
    /// Lowercase authored request token. Distinct tokens on one target are
    /// distinct jobs; a missing token cannot select a behavior task.
    pub token: Option<AssetId>,
}

impl StaffJob {
    pub fn advance_wait(&mut self, elapsed_seconds: f32) {
        if self.remaining_wait_seconds > 0.0 {
            self.remaining_wait_seconds -= elapsed_seconds;
        }
    }

    pub fn destination(&self, live_target_position: Option<Vec3>) -> Option<Vec3> {
        self.point.or(live_target_position)
    }
}

/// The live target of a job. The job can finish after a target is consumed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = StaffJobsAtTarget)]
pub struct StaffJobTarget(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = StaffJobTarget)]
pub struct StaffJobsAtTarget(Vec<Entity>);

#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaffJobRouting {
    pub subject_type: Option<AssetId>,
    /// An authored object instance identifier, scoped to the job's world.
    pub recipient: Option<AssetId>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffCognitiveEligibility {
    pub interruptible: bool,
    pub on_tap: bool,
    pub blocks_request_search: bool,
}

#[derive(Component)]
pub struct StaffLocalViewingArea;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StaffRequestResourceCoverage {
    pub food: bool,
    pub water: bool,
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
