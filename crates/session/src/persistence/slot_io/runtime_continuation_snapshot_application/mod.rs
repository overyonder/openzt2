use std::collections::HashSet;

use bevy::prelude::*;
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
    runtime_continuation_snapshot_types::{
        RuntimeContinuationIdentityHighWaterMarks, RuntimeContinuationSnapshot,
    },
};

pub(super) fn validate_runtime_continuation_identity_high_water_marks(
    record: &RuntimeContinuationSnapshot,
    high_water_marks: RuntimeContinuationIdentityHighWaterMarks<'_>,
) -> Result<(), Failure> {
    validate_runtime_continuation_snapshot(record)?;
    if record.next_persistent_identifier <= high_water_marks.maximum_persistent_identifier
        || record.navigation_sequence < high_water_marks.maximum_navigation_request
        || record.behavior_execution_sequence < high_water_marks.maximum_behavior_execution
    {
        return Err(Failure::BrokenPersistentEntityReference);
    }
    let mut service_requests = HashSet::new();
    for sequence in high_water_marks.pending_service_request_sequences {
        if record
            .service_request_sequence
            .is_none_or(|next| *sequence >= next)
            || !service_requests.insert(*sequence)
        {
            return Err(Failure::BrokenPersistentEntityReference);
        }
    }
    Ok(())
}

/// Queue this last on the same Commands as restored entities and buffered results.
pub(super) fn apply_runtime_continuation_snapshot(
    commands: &mut Commands,
    world_root: Entity,
    record: &RuntimeContinuationSnapshot,
    high_water_marks: RuntimeContinuationIdentityHighWaterMarks<'_>,
) -> Result<(), Failure> {
    validate_runtime_continuation_identity_high_water_marks(record, high_water_marks)?;
    let allocator =
        PersistentIdAllocator::from_snapshot(world_root, record.next_persistent_identifier)
            .map_err(|_| Failure::CorruptSnapshotSection)?;
    if commands.get_entity(world_root).is_err() {
        return Err(Failure::BrokenPersistentEntityReference);
    }
    commands.insert_resource(allocator);
    commands.insert_resource(NavigationRequestSequence::from_snapshot(
        record.navigation_sequence,
    ));
    commands.insert_resource(BehaviorTaskExecutionSequence::from_snapshot(
        record.behavior_execution_sequence,
    ));
    apply_optional_resource(
        commands,
        record
            .service_request_sequence
            .map(ServiceRequestSequence::from_snapshot),
    );
    apply_optional_resource::<GuestArrivalState>(commands, record.guest_arrivals);
    apply_optional_resource::<GuestSurvey>(commands, record.guest_survey);
    Ok(())
}

fn apply_optional_resource<T: Resource>(commands: &mut Commands, resource: Option<T>) {
    if let Some(resource) = resource {
        commands.insert_resource(resource);
    } else {
        commands.remove_resource::<T>();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        runtime_continuation_snapshot_capture::RuntimeContinuationSnapshotCapture,
        runtime_continuation_snapshot_encoding::{
            append_runtime_continuation_snapshot, decode_runtime_continuation_snapshot,
        },
    };
    use super::*;
    use bevy::ecs::{system::SystemState, world::CommandQueue};
    use openzt2_world_state::{
        simulation_time::deterministic_random_stream::DeterministicRng,
        world_spawn::persistent_id_types::{PersistentId, PersistentIdError},
    };

    fn high_water_marks<'a>(
        pending_service_request_sequences: &'a [u64],
    ) -> RuntimeContinuationIdentityHighWaterMarks<'a> {
        RuntimeContinuationIdentityHighWaterMarks {
            maximum_persistent_identifier: 42,
            maximum_navigation_request: 14,
            maximum_behavior_execution: 28,
            pending_service_request_sequences,
        }
    }

    fn record() -> RuntimeContinuationSnapshot {
        RuntimeContinuationSnapshot {
            next_persistent_identifier: 800,
            navigation_sequence: 900,
            behavior_execution_sequence: 1000,
            service_request_sequence: Some(700),
            guest_arrivals: Some(GuestArrivalState {
                next_tick: 444,
                rng: DeterministicRng::from_raw([1234, 789]),
            }),
            guest_survey: Some(GuestSurvey {
                next_tick: 555,
                month: 18,
                critical_hits: 7,
                education_points: 40,
                entertainment_points: 90,
                critical_need_rate_permille: 750,
                education_view_rate_permille: 200,
                entertainment_view_rate_permille: 450,
                view_score_permille: 691,
            }),
        }
    }

    #[test]
    fn exact_resource_continuation_preserves_gaps_random_draw_and_absence() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let other_root = world.spawn_empty().id();
        let mut queue = CommandQueue::default();
        let saved = record();
        apply_runtime_continuation_snapshot(
            &mut Commands::new(&mut queue, &world),
            root,
            &saved,
            high_water_marks(&[12, 31]),
        )
        .unwrap();
        queue.apply(&mut world);
        let recaptured = {
            let mut state = SystemState::<RuntimeContinuationSnapshotCapture>::new(&mut world);
            let capture = state.get(&world).unwrap();
            assert!(capture.capture(other_root).is_err());
            capture.capture(root).unwrap()
        };
        let mut saved_bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut saved_bytes, &saved).unwrap();
        let mut recaptured_bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut recaptured_bytes, &recaptured).unwrap();
        assert_eq!(recaptured_bytes, saved_bytes);
        assert_eq!(
            world
                .resource_mut::<PersistentIdAllocator>()
                .allocate(root)
                .unwrap(),
            PersistentId(800)
        );
        assert_eq!(
            world.resource_mut::<NavigationRequestSequence>().next(),
            901
        );
        assert_eq!(
            world.resource_mut::<BehaviorTaskExecutionSequence>().next(),
            1001
        );
        assert_eq!(
            world.resource_mut::<ServiceRequestSequence>().allocate(),
            Some(700)
        );
        assert_eq!(
            world.resource_mut::<GuestArrivalState>().rng.next_u32(),
            saved.guest_arrivals.unwrap().rng.next_u32()
        );
        let mut absent = saved;
        absent.service_request_sequence = None;
        absent.guest_arrivals = None;
        absent.guest_survey = None;
        apply_runtime_continuation_snapshot(
            &mut Commands::new(&mut queue, &world),
            root,
            &absent,
            high_water_marks(&[]),
        )
        .unwrap();
        queue.apply(&mut world);
        assert!(!world.contains_resource::<ServiceRequestSequence>());
        assert!(!world.contains_resource::<GuestArrivalState>());
        assert!(!world.contains_resource::<GuestSurvey>());
        assert_eq!(
            world.resource::<NavigationRequestSequence>().last_issued(),
            900
        );
    }

    #[test]
    fn exhausted_counters_restore_without_reissuing_old_identities() {
        let mut saved = record();
        saved.next_persistent_identifier = u64::MAX;
        saved.navigation_sequence = u64::MAX;
        saved.behavior_execution_sequence = u64::MAX;
        saved.service_request_sequence = Some(u64::MAX);
        let mut bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut bytes, &saved).unwrap();
        let decoded = decode_runtime_continuation_snapshot(&bytes).unwrap();
        let mut world = World::new();
        let root = world.spawn_empty().id();
        assert_eq!(
            PersistentIdAllocator::from_snapshot(root, 0),
            Err(PersistentIdError::Zero)
        );
        let mut queue = CommandQueue::default();
        apply_runtime_continuation_snapshot(
            &mut Commands::new(&mut queue, &world),
            root,
            &decoded,
            RuntimeContinuationIdentityHighWaterMarks {
                maximum_persistent_identifier: u64::MAX - 1,
                maximum_navigation_request: u64::MAX,
                maximum_behavior_execution: u64::MAX,
                pending_service_request_sequences: &[u64::MAX - 1],
            },
        )
        .unwrap();
        queue.apply(&mut world);
        assert_eq!(
            world.resource_mut::<PersistentIdAllocator>().allocate(root),
            Err(PersistentIdError::Exhausted)
        );
        assert_eq!(
            world.resource_mut::<ServiceRequestSequence>().allocate(),
            None
        );
        assert_eq!(
            world.resource::<NavigationRequestSequence>().last_issued(),
            u64::MAX
        );
        assert_eq!(
            world
                .resource::<BehaviorTaskExecutionSequence>()
                .last_issued(),
            u64::MAX
        );
        let mut navigation = NavigationRequestSequence::from_snapshot(u64::MAX - 1);
        let mut behavior = BehaviorTaskExecutionSequence::from_snapshot(u64::MAX - 1);
        assert_eq!(navigation.next(), u64::MAX);
        assert_eq!(behavior.next(), u64::MAX);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| navigation.next())).is_err()
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| behavior.next())).is_err()
        );
    }

    #[test]
    fn inconsistent_highwaters_reject_before_queuing_resource_replacements() {
        let saved = record();
        let mut invalid = high_water_marks(&[12]);
        for field in 0..5 {
            invalid.maximum_persistent_identifier = if field == 0 { 800 } else { 42 };
            invalid.maximum_navigation_request = if field == 1 { 901 } else { 14 };
            invalid.maximum_behavior_execution = if field == 2 { 1001 } else { 28 };
            invalid.pending_service_request_sequences = if field == 3 {
                &[700]
            } else if field == 4 {
                &[12, 12]
            } else {
                &[12]
            };
            let mut world = World::new();
            let root = world.spawn_empty().id();
            world.insert_resource(NavigationRequestSequence::from_snapshot(55));
            let mut queue = CommandQueue::default();
            assert!(
                apply_runtime_continuation_snapshot(
                    &mut Commands::new(&mut queue, &world),
                    root,
                    &saved,
                    invalid
                )
                .is_err()
            );
            queue.apply(&mut world);
            assert_eq!(
                world.resource::<NavigationRequestSequence>().last_issued(),
                55
            );
            assert!(!world.contains_resource::<PersistentIdAllocator>());
        }
        let mut absent = saved;
        absent.service_request_sequence = None;
        assert!(
            validate_runtime_continuation_identity_high_water_marks(
                &absent,
                high_water_marks(&[0])
            )
            .is_err()
        );
    }
}
