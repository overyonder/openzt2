use std::collections::BTreeMap;

use bevy::prelude::*;

use openzt2_game_assets::world_scenario::authored_world_scenario_state::AuthoredWorldScenarioState;
use openzt2_game_data::{world_scenario::ScenarioObjectiveGroupRecord, AssetId};
use openzt2_zoo_state::scenario::scenario_objective_types::{
    ScenarioObjectiveGroup, ScenarioObjectiveGroupOf, ScenarioObjectiveStatus,
};

#[derive(Component)]
pub(super) struct ScenarioCompletionGroup;

pub(super) fn derive_group_status(
    group: &ScenarioObjectiveGroupRecord,
    statuses: &BTreeMap<AssetId, ScenarioObjectiveStatus>,
    authored: &AuthoredWorldScenarioState,
) -> ScenarioObjectiveStatus {
    use ScenarioObjectiveStatus::{Active, Failed, Satisfied};
    let (deciding, neutral_result) = if group.all_success {
        (Failed, Satisfied)
    } else {
        (Satisfied, Failed)
    };
    let mut result = neutral_result;
    for member in &group.members {
        if authored
            .scenario_nodes
            .get(member)
            .and_then(|policy| policy.optional)
            .unwrap_or(false)
        {
            continue;
        }
        match statuses.get(member).copied() {
            Some(status) if status == deciding => return deciding,
            Some(status) if status == neutral_result => {}
            _ => result = Active,
        }
    }
    result
}

pub(crate) fn spawn_scenario_objective_groups(
    commands: &mut Commands,
    owner: Entity,
    groups: &[ScenarioObjectiveGroupRecord],
    mut statuses: BTreeMap<AssetId, ScenarioObjectiveStatus>,
    authored: &AuthoredWorldScenarioState,
) {
    for (record_index, group) in groups.iter().enumerate() {
        let status = derive_group_status(group, &statuses, authored);
        statuses.insert(group.id, status);
        let optional = authored
            .scenario_nodes
            .get(&group.id)
            .and_then(|policy| policy.optional)
            .unwrap_or(false);
        let mut group_entity = commands.spawn((
            ScenarioObjectiveGroupOf(owner),
            ScenarioObjectiveGroup {
                record_index,
                status,
            },
        ));
        if record_index + 1 == groups.len() && !optional {
            group_entity.insert(ScenarioCompletionGroup);
        }
    }
}
