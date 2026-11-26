use bevy::prelude::*;

use bevy::ecs::relationship::RelationshipTarget;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_zoo_state::scenario::scenario_objective_types::{
    ScenarioObjectiveGroup, ScenarioObjectiveGroupOf, ScenarioObjectiveGroups, ScenarioObjectiveOf,
    ScenarioObjectiveStatus, ScenarioObjectives,
};
use openzt2_zoo_state::scenario::scenario_session_types::ActiveScenarioSession;
use openzt2_zoo_state::scenario::scenario_session_types::PendingScenarioTerminalResult;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioSessionState;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioTerminalResult;

use super::{
    challenge_offer_types::{
        ScenarioChallengeOffer, ScenarioChallengeOfferState, ScenarioChallengeRuleContext,
    },
    scenario_group_completion::{reduce_group, ScenarioCompletionGroup},
    scenario_lua_types::ScenarioLuaContext,
};

pub(super) fn queue_completed_scenarios_for_terminal_resolution(
    mut commands: Commands,
    active: Query<
        (
            &ActiveScenarioSession,
            Option<&ScenarioObjectives>,
            Option<&ScenarioObjectiveGroups>,
        ),
        Without<PendingScenarioTerminalResult>,
    >,
    offers: Query<
        (
            &ScenarioChallengeOffer,
            Option<&ScenarioObjectives>,
            Option<&ScenarioObjectiveGroups>,
        ),
        Without<PendingScenarioTerminalResult>,
    >,
    changed_offers: Query<Entity, Changed<ScenarioChallengeOffer>>,
    objectives: Query<(&ScenarioObjectiveStatus, Has<ScenarioCompletionGroup>)>,
    groups: Query<&ScenarioObjectiveGroup, With<ScenarioCompletionGroup>>,
    changed_objectives: Query<&ScenarioObjectiveOf, Changed<ScenarioObjectiveStatus>>,
    changed_groups: Query<&ScenarioObjectiveGroupOf, Changed<ScenarioObjectiveGroup>>,
) {
    let changed = changed_objectives
        .iter()
        .map(|owner| owner.0)
        .chain(changed_groups.iter().map(|owner| owner.0))
        .chain(changed_offers.iter())
        .collect::<bevy::ecs::entity::EntityHashSet>();
    for owner in changed {
        let (scenario, members, authored_groups, declined) =
            if let Ok((session, members, groups)) = active.get(owner) {
                if session.state != ScenarioSessionState::Running {
                    continue;
                }
                (session.definition, members, groups, false)
            } else if let Ok((offer, members, groups)) = offers.get(owner) {
                (
                    offer.definition,
                    members,
                    groups,
                    offer.state == ScenarioChallengeOfferState::Declined,
                )
            } else {
                continue;
            };
        let mut root_statuses = authored_groups
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .filter_map(|group| groups.get(group).ok().map(|group| group.status))
            .chain(
                members
                    .into_iter()
                    .flat_map(RelationshipTarget::iter)
                    .filter_map(|entity| {
                        let (status, required) = objectives.get(entity).ok()?;
                        required.then_some(*status)
                    }),
            )
            .peekable();
        if root_statuses.peek().is_some() {
            let state = match reduce_group(true, root_statuses) {
                ScenarioObjectiveStatus::Satisfied => ScenarioSessionState::Won,
                ScenarioObjectiveStatus::Failed => ScenarioSessionState::Lost,
                ScenarioObjectiveStatus::Active | ScenarioObjectiveStatus::Inactive => continue,
            };
            commands
                .entity(owner)
                .insert(PendingScenarioTerminalResult { scenario, state });
            continue;
        }
        if authored_groups.is_some() {
            continue;
        }
        let Some(members) = members else {
            continue;
        };
        let mut states = objectives.iter_many(members.iter()).flatten().peekable();
        if states.peek().is_none() {
            continue;
        }
        let mut failed = false;
        let mut complete = true;
        for (status, _) in states {
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
    mut runtime: Query<&mut ScenarioLuaContext>,
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
            if let Err(error) = runtime.get_mut(context.0).map_or(Ok(()), |mut context| {
                context.clear_finished_challenge_rule_state()
            }) {
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
