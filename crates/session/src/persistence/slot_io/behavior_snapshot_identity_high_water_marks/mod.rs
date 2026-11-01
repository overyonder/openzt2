use super::behavior_snapshot_types::BehaviorActorSnapshot;

pub(super) fn behavior_snapshot_identity_high_water_marks(
    records: &[BehaviorActorSnapshot],
) -> (u64, u64) {
    let mut navigation = 0;
    let mut execution = 0;
    for actor in records {
        execution = execution.max(actor.task.execution_id);
        if let Some(score) = &actor.task.score {
            execution = execution.max(score.execution_id);
        }
        if let Some(transaction) = &actor.transaction {
            execution = execution.max(transaction.execution_id);
        }
        if let Some(synchronized) = &actor.synchronized {
            execution = execution
                .max(synchronized.execution_id)
                .max(synchronized.target_execution_id);
        }
        let Some(state) = &actor.navigation else {
            continue;
        };
        if let Some(destination) = &state.destination {
            navigation = navigation.max(destination.request_id);
        }
        if let Some(docking) = &state.docking {
            navigation = navigation.max(docking.request_id);
        }
        for completion in state
            .move_completion
            .iter()
            .chain(state.docking_completion.iter())
        {
            navigation = navigation.max(completion.request_id);
        }
        for result in &state.pending_results {
            navigation = navigation.max(result.request_id());
        }
        if let Some(pursuit) = &state.pursuit {
            execution = execution.max(pursuit.execution_id);
            for request in pursuit.leg.navigation_request_ids().into_iter().flatten() {
                navigation = navigation.max(request);
            }
        }
    }
    (navigation, execution)
}
