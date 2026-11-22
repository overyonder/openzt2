use bevy::prelude::*;

use bevy::ecs::relationship::RelationshipTarget;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectives;
use openzt2_zoo_state::scenario::scenario_session_types::ActiveScenarioSession;
use openzt2_zoo_state::scenario::scenario_session_types::PendingScenarioTerminalResult;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioSessionState;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioTerminalResult;

use super::{
    challenge_offer_types::{
        ScenarioChallengeOffer, ScenarioChallengeOfferState, ScenarioChallengeRuleContext,
    },
    scenario_lua_types::ScenarioLuaVirtualMachineContexts,
};

pub(super) fn queue_completed_scenarios_for_terminal_resolution(
    mut commands: Commands,
    active: Query<
        (Entity, &ActiveScenarioSession, &ScenarioObjectives),
        Without<PendingScenarioTerminalResult>,
    >,
    offers: Query<
        (Entity, &ScenarioChallengeOffer, &ScenarioObjectives),
        Without<PendingScenarioTerminalResult>,
    >,
    objectives: Query<&ScenarioObjectiveStatus>,
) {
    let owners = active
        .iter()
        .filter_map(|(entity, session, members)| {
            (session.state == ScenarioSessionState::Running).then_some((
                entity,
                session.definition,
                members,
                false,
            ))
        })
        .chain(offers.iter().map(|(entity, offer, members)| {
            (
                entity,
                offer.definition,
                members,
                offer.state == ScenarioChallengeOfferState::Declined,
            )
        }));
    for (owner, scenario, members, declined) in owners {
        let mut states = objectives.iter_many(members.iter()).flatten().peekable();
        if states.peek().is_none() {
            continue;
        }
        let mut failed = false;
        let mut complete = true;
        for status in states {
            failed |= *status == ScenarioObjectiveStatus::Failed;
            complete &= matches!(
                status,
                ScenarioObjectiveStatus::Satisfied | ScenarioObjectiveStatus::Failed
            );
        }
        if complete || (declined && failed) {
            commands
                .entity(owner)
                .insert(PendingScenarioTerminalResult {
                    scenario,
                    state: if failed {
                        ScenarioSessionState::Lost
                    } else {
                        ScenarioSessionState::Won
                    },
                });
        }
    }
}

pub(super) fn apply_terminal_scenario_results_and_remove_completed_challenges(
    mut commands: Commands,
    mut sessions: Query<
        (
            &mut ActiveScenarioSession,
            &openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity,
        ),
        With<WorldRoot>,
    >,
    terminals: Query<(Entity, &PendingScenarioTerminalResult)>,
    offers: Query<&ScenarioChallengeRuleContext, With<ScenarioChallengeOffer>>,
    mut results: MessageWriter<ScenarioTerminalResult>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    mut profile_variables: ResMut<
        openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables,
    >,
    scenarios: Res<openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios>,
    scenario_assets: Res<Assets<openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset>>,
) {
    for (entity, terminal) in &terminals {
        if let Ok((mut active, selection)) = sessions.get_mut(entity) {
            active.state = terminal.state;
            if terminal.state == ScenarioSessionState::Won
                && selection.mode
                    == openzt2_world_state::game_session_types::WorldSessionMode::Campaign
            {
                if let Err(reason) = profile_variables.set(selection.requested, Some("completed")) {
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
                            if let Err(reason) = profile_variables.set(next.id, Some("unlocked")) {
                                error!(reason, "next campaign mission could not be unlocked");
                            }
                        }
                    }
                }
            }
            commands
                .entity(entity)
                .remove::<PendingScenarioTerminalResult>();
        } else if let Ok(context) = offers.get(entity) {
            if let Err(error) = runtime.clear_finished_challenge_rule_state(context.0) {
                error!(%error, "completed challenge could not release its rule state");
            }
            commands.entity(entity).despawn();
        }
        results.write(ScenarioTerminalResult {
            scenario: terminal.scenario,
            state: terminal.state,
        });
    }
}
