use std::collections::BTreeMap;

use openzt2_game_data::{world_scenario::ScenarioObjectiveGroupRecord, AssetId};
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;

pub(super) fn derive_group_status(
    group: &ScenarioObjectiveGroupRecord,
    statuses: &BTreeMap<AssetId, ScenarioObjectiveStatus>,
) -> ScenarioObjectiveStatus {
    use ScenarioObjectiveStatus::{Active, Failed, Satisfied};
    let mut satisfied = 0;
    let mut failed = 0;
    for member in &group.members {
        match statuses.get(member) {
            Some(Satisfied) => satisfied += 1,
            Some(Failed) => failed += 1,
            _ => {}
        }
    }
    if group.all_success {
        if failed > 0 {
            Failed
        } else if satisfied == group.members.len() {
            Satisfied
        } else {
            Active
        }
    } else if satisfied > 0 {
        Satisfied
    } else if failed == group.members.len() {
        Failed
    } else {
        Active
    }
}
