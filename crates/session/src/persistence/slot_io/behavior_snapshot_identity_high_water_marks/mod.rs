use super::behavior_snapshot_types::{BehaviorActorSnapshot, BehaviorOwnedNavigationSnapshot};

pub(super) fn behavior_snapshot_identity_high_water_marks(
    records: &[BehaviorActorSnapshot],
) -> (u64, u64) {
    let mut navigation = 0;
    let mut execution = 0;
    for actor in records
        .iter()
        .filter_map(BehaviorActorSnapshot::task_record)
    {
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
        navigation = navigation.max(maximum_owned_navigation_request(state));
        if let Some(pursuit) = &state.pursuit {
            execution = execution.max(pursuit.execution_id);
        }
    }
    (navigation, execution)
}

pub(super) fn maximum_owned_navigation_request(state: &BehaviorOwnedNavigationSnapshot) -> u64 {
    state
        .destination
        .iter()
        .map(|destination| destination.request_id)
        .chain(state.docking.iter().map(|dock| dock.request_id))
        .chain(
            state
                .move_completion
                .iter()
                .chain(&state.docking_completion)
                .map(|completion| completion.request_id),
        )
        .chain(
            state
                .pending_results
                .iter()
                .map(|result| result.request_id()),
        )
        .chain(
            state
                .pursuit
                .iter()
                .flat_map(|pursuit| pursuit.leg.navigation_request_ids().into_iter().flatten()),
        )
        .chain(state.action_navigation_requests())
        .max()
        .unwrap_or_default()
}
