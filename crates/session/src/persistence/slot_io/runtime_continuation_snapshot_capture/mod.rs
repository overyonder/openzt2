use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_zoo_state::{
    behavior_task_execution_types::BehaviorTaskExecutionSequence,
    economy::service_types::ServiceRequestSequence,
    guests::guest_simulation_types::{GuestArrivalState, GuestSurvey},
    locomotion::locomotion_types::NavigationRequestSequence,
};

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    runtime_continuation_snapshot_encoding::validate_runtime_continuation_snapshot,
    runtime_continuation_snapshot_types::RuntimeContinuationSnapshot,
};

#[derive(SystemParam)]
pub(in crate::persistence) struct RuntimeContinuationSnapshotCapture<'w> {
    allocator: Res<'w, PersistentIdAllocator>,
    navigation: Res<'w, NavigationRequestSequence>,
    behavior: Res<'w, BehaviorTaskExecutionSequence>,
    service: Option<Res<'w, ServiceRequestSequence>>,
    arrivals: Option<Res<'w, GuestArrivalState>>,
    survey: Option<Res<'w, GuestSurvey>>,
}

impl RuntimeContinuationSnapshotCapture<'_> {
    pub(super) fn capture(
        &self,
        world_root: Entity,
    ) -> Result<RuntimeContinuationSnapshot, Failure> {
        if !self.allocator.owns_world(world_root) {
            return Err(Failure::BrokenPersistentEntityReference);
        }
        let record = RuntimeContinuationSnapshot {
            next_persistent_identifier: self.allocator.next(),
            navigation_sequence: self.navigation.last_issued(),
            behavior_execution_sequence: self.behavior.last_issued(),
            service_request_sequence: self
                .service
                .as_ref()
                .map(|sequence| sequence.next_sequence()),
            guest_arrivals: self.arrivals.as_deref().copied(),
            guest_survey: self.survey.as_deref().copied(),
        };
        validate_runtime_continuation_snapshot(&record)?;
        Ok(record)
    }
}
