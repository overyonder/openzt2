use bevy::prelude::*;

use openzt2_game_assets::world_scenario::{
    world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
    world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};

use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjective;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveAuthoredVisibility;
use openzt2_zoo_state::scenario::scenario_objective_types::ShowScenarioRuleRequest;

pub(super) fn show_requested_scenario_rules(
    mut requests: MessageReader<ShowScenarioRuleRequest>,
    scenarios: Res<WorldScenarios>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    mut objectives: Query<(&ScenarioObjective, &mut ScenarioObjectiveAuthoredVisibility)>,
) {
    let Some(scenarios) = scenarios.get(&assets) else {
        return;
    };
    for request in requests.read() {
        let Some(scenario) = scenarios.scenario(request.scenario) else {
            continue;
        };
        for (objective, mut visibility) in &mut objectives {
            if objective.scenario == request.scenario
                && scenario
                    .objectives
                    .get(objective.record_index as usize)
                    .is_some_and(|record| record.name == Some(request.rule))
            {
                visibility.0 = true;
            }
        }
    }
}
