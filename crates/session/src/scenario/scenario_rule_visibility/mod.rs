use bevy::{ecs::relationship::RelationshipTarget, prelude::*};

use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_zoo_state::scenario::{
    scenario_objective_types::{
        ScenarioObjective, ScenarioObjectiveAuthoredVisibility, ScenarioObjectiveOf,
        ScenarioObjectives, ShowScenarioRuleRequest,
    },
    scenario_session_types::ScenarioDocument,
};

pub(super) fn show_requested_scenario_rules(
    mut requests: MessageReader<ShowScenarioRuleRequest>,
    owners: Query<(&ScenarioDocument, &ScenarioObjectives)>,
    membership: Query<&ScenarioObjectiveOf>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    mut objectives: Query<(&ScenarioObjective, &mut ScenarioObjectiveAuthoredVisibility)>,
) {
    for request in requests.read() {
        let Some(source) = request.source else {
            continue;
        };
        let owner = membership
            .get(source)
            .map_or(source, |membership| membership.0);
        let Ok((document, related)) = owners.get(owner) else {
            continue;
        };
        let Some(scenario) = assets
            .get(&document.0)
            .and_then(|asset| asset.document.find_scenario_record(request.scenario))
        else {
            continue;
        };
        let mut cursor = objectives.iter_many_mut(related.iter());
        while let Some(item) = cursor.fetch_next() {
            let Ok((objective, mut visibility)) = item else {
                continue;
            };
            if scenario
                .objectives
                .get(objective.record_index as usize)
                .is_some_and(|record| record.name == Some(request.rule))
            {
                visibility.set_if_neq(ScenarioObjectiveAuthoredVisibility(true));
            }
        }
    }
}
