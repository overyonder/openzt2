use bevy::prelude::*;

use crate::plugins::world_spawn::{
    world_membership_types::WorldMember, world_membership_types::WorldRoot,
};

use super::{
    campaign_progress_types::ScenarioCampaignCompletionProgress,
    challenge_offer_types::{ScenarioChallengeOffer, ScenarioChallengeOfferState},
    scenario_lua_types::ScenarioLuaVirtualMachineContexts,
    scenario_objective_types::{ScenarioObjective, ScenarioObjectiveStatus},
    scenario_session_types::{
        ActiveScenarioSession, PendingScenarioTerminalResult, ScenarioSessionState,
        ScenarioTerminalResult,
    },
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
            Option<&mut ScenarioCampaignCompletionProgress>,
        ),
        With<WorldRoot>,
    >,
    terminals: Query<(Entity, &PendingScenarioTerminalResult)>,
    offers: Query<(Entity, &ScenarioChallengeOffer)>,
    objectives: Query<(Entity, &ScenarioObjective)>,
    mut results: MessageWriter<ScenarioTerminalResult>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
) {
    for (entity, terminal) in &terminals {
        let Ok((active, mut campaign)) = sessions.single_mut() else {
            continue;
        };
        if let Some(mut active) = active {
            if active.definition == terminal.scenario {
                active.state = terminal.state;
                if terminal.state == ScenarioSessionState::Won {
                    if let Some(campaign) = campaign.as_deref_mut() {
                        let mission = campaign.mission;
                        let _ = campaign.record_completed_mission(mission);
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
