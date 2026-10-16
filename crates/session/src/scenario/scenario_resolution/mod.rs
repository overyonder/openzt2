use bevy::prelude::*;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjective;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;
use openzt2_zoo_state::scenario::scenario_session_types::ActiveScenarioSession;
use openzt2_zoo_state::scenario::scenario_session_types::PendingScenarioTerminalResult;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioSessionState;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioTerminalResult;

use super::{
    challenge_offer_types::{ScenarioChallengeOffer, ScenarioChallengeOfferState},
    scenario_lua_types::ScenarioLuaVirtualMachineContexts,
};

pub(super) fn queue_completed_scenarios_for_terminal_resolution(
    mut commands: Commands,
    active: Query<&ActiveScenarioSession>,
    objectives: Query<(&ScenarioObjective, &ScenarioObjectiveStatus)>,
    offers: Query<&ScenarioChallengeOffer>,
    pending: Query<&PendingScenarioTerminalResult>,
    roots: Query<Entity, With<WorldRoot>>,
) {
    let Ok(root) = roots.single() else { return };
    if let Some(active) = active.single().ok().filter(|active| {
        active.state == ScenarioSessionState::Running
            && !pending
                .iter()
                .any(|item| item.scenario == active.definition)
    }) {
        queue_terminal_result_when_all_objectives_are_terminal(
            &mut commands,
            root,
            active.definition,
            &objectives,
        );
    }
    for offer in &offers {
        if !pending.iter().any(|item| item.scenario == offer.definition) {
            // The authored offer rule returns -1 on decline. Prerequisite
            // successors cannot activate after that failure; do not wait for
            // those inactive rules before releasing the declined challenge.
            if offer.state == ScenarioChallengeOfferState::Declined
                && objectives.iter().any(|(objective, status)| {
                    objective.scenario == offer.definition
                        && *status == ScenarioObjectiveStatus::Failed
                })
            {
                commands.spawn((
                    WorldMember { root },
                    PendingScenarioTerminalResult {
                        scenario: offer.definition,
                        state: ScenarioSessionState::Lost,
                    },
                ));
                continue;
            }
            queue_terminal_result_when_all_objectives_are_terminal(
                &mut commands,
                root,
                offer.definition,
                &objectives,
            );
        }
    }
}

fn queue_terminal_result_when_all_objectives_are_terminal(
    commands: &mut Commands,
    root: Entity,
    scenario: openzt2_game_data::AssetId,
    objectives: &Query<(&ScenarioObjective, &ScenarioObjectiveStatus)>,
) {
    let mut matching = objectives
        .iter()
        .filter(|(objective, _)| objective.scenario == scenario);
    let Some((_, first)) = matching.next() else {
        return;
    };
    let mut failed = *first == ScenarioObjectiveStatus::Failed;
    let mut complete = matches!(
        first,
        ScenarioObjectiveStatus::Satisfied | ScenarioObjectiveStatus::Failed
    );
    for (_, status) in matching {
        complete &= matches!(
            status,
            ScenarioObjectiveStatus::Satisfied | ScenarioObjectiveStatus::Failed
        );
        failed |= *status == ScenarioObjectiveStatus::Failed;
    }
    if complete {
        commands.spawn((
            WorldMember { root },
            PendingScenarioTerminalResult {
                scenario,
                state: if failed {
                    ScenarioSessionState::Lost
                } else {
                    ScenarioSessionState::Won
                },
            },
        ));
    }
}

pub(super) fn apply_terminal_scenario_results_and_remove_completed_challenges(
    mut commands: Commands,
    mut sessions: Query<
        (
            Option<&mut ActiveScenarioSession>,
            &openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity,
        ),
        With<WorldRoot>,
    >,
    terminals: Query<(Entity, &PendingScenarioTerminalResult)>,
    offers: Query<(Entity, &ScenarioChallengeOffer)>,
    objectives: Query<(Entity, &ScenarioObjective)>,
    mut results: MessageWriter<ScenarioTerminalResult>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    mut profile_variables: ResMut<
        openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables,
    >,
    scenarios: Res<openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios>,
    scenario_assets: Res<Assets<openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset>>,
) {
    for (entity, terminal) in &terminals {
        let Ok((active, selection)) = sessions.single_mut() else {
            continue;
        };
        if let Some(mut active) = active {
            if active.definition == terminal.scenario {
                active.state = terminal.state;
                if terminal.state == ScenarioSessionState::Won
                    && selection.mode
                        == openzt2_world_state::game_session_types::WorldSessionMode::Campaign
                {
                    if let Err(reason) =
                        profile_variables.set(selection.requested, Some("completed"))
                    {
                        error!(reason, "campaign completion could not be recorded");
                    }
                    if let Some((campaign, mission)) = scenarios
                        .get(&scenario_assets)
                        .and_then(|view| view.campaign_for_scenario(selection.requested))
                    {
                        if let Some(next) = campaign.scenarios.get(mission + 1) {
                            if profile_variables.scenario_state(next)
                                == openzt2_game_data::world_scenario::CampaignScenarioState::Locked
                            {
                                if let Err(reason) =
                                    profile_variables.set(next.id, Some("unlocked"))
                                {
                                    error!(reason, "next campaign mission could not be unlocked");
                                }
                            }
                        }
                    }
                }
            }
        }
        for (offer_entity, offer) in &offers {
            if offer.definition == terminal.scenario {
                if let Err(error) = runtime.clear_finished_challenge_rule_state(offer.definition) {
                    error!(%error, "completed challenge could not release its rule state");
                }
                commands.entity(offer_entity).despawn();
                for (objective_entity, objective) in &objectives {
                    if objective.scenario == terminal.scenario {
                        commands.entity(objective_entity).despawn();
                    }
                }
            }
        }
        results.write(ScenarioTerminalResult {
            scenario: terminal.scenario,
            state: terminal.state,
        });
        commands.entity(entity).despawn();
    }
}
