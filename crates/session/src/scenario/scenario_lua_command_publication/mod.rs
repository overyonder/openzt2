use bevy::prelude::*;

use openzt2_zoo_state::economy::scenario_economy_command_types::ScenarioEconomyCommand;
use openzt2_zoo_state::progression::award_and_progression_fact_types::AdjustScenarioAwardPointTotalRequest;

use super::{
    challenge_offer_types::CreateScenarioChallengeOfferFromLuaSourcePathRequest,
    scenario_lua_types::ScenarioLuaScriptResult,
};

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct ScenarioLuaOutputs<'w> {
    economy: MessageWriter<'w, ScenarioEconomyCommand>,
    award_points: MessageWriter<'w, AdjustScenarioAwardPointTotalRequest>,
    challenge_offers: MessageWriter<'w, CreateScenarioChallengeOfferFromLuaSourcePathRequest>,
    panels: MessageWriter<'w, super::challenge_offer_types::ScenarioChallengePanelRequest>,
    shown_rules: MessageWriter<'w,
        openzt2_zoo_state::scenario::scenario_objective_types::ShowScenarioRuleRequest,
    >,
    available_content: MessageWriter<'w,
        openzt2_zoo_state::progression::adoption_and_content_availability_types::MakeScenarioContentAvailableRequest,
    >,
    photo_challenge_sets: MessageWriter<'w,
        super::scenario_photo_challenge_selection::SelectCurrentPhotoChallengeSetRequest,
    >,
    persist_profile_variables: MessageWriter<'w,
        openzt2_zoo_state::progression::profile_scenario_variables::PersistProfileScenarioVariables,
    >,
    ui: MessageWriter<'w, super::scenario_lua_ui::ScenarioUiCommand>,
    entities: MessageWriter<'w, super::scenario_lua_entity_commands::ScenarioEntityCommand>,
    fame: MessageWriter<'w, openzt2_zoo_state::progression::fame_types::SetScenarioMaximumFame>,
}

impl ScenarioLuaOutputs<'_> {
    pub(super) fn write_timed_boundary(
        &mut self,
        command: super::scenario_lua_ui::ScenarioUiCommand,
    ) {
        self.ui.write(command);
    }

    pub(super) fn publish(&mut self, terminal: Entity, result: ScenarioLuaScriptResult) {
        if let Some(percent) = result.maximum_fame_percent {
            self.fame.write(
                openzt2_zoo_state::progression::fame_types::SetScenarioMaximumFame { percent },
            );
        }
        let _ = self.entities.write_batch(result.entities);
        let _ = self.ui.write_batch(result.ui);
        if result.persist_profile_variables {
            self.persist_profile_variables.write(openzt2_zoo_state::progression::profile_scenario_variables::PersistProfileScenarioVariables);
        }
        if let Some(set) = result.current_photo_challenge_set {
            self.photo_challenge_sets.write(
                super::scenario_photo_challenge_selection::SelectCurrentPhotoChallengeSetRequest {
                    set,
                },
            );
        }
        for definition in result.available_content {
            self.available_content.write(
            openzt2_zoo_state::progression::adoption_and_content_availability_types::MakeScenarioContentAvailableRequest {
                source: terminal,
                definition,
            },
        );
        }
        let _ = self
            .shown_rules
            .write_batch(result.shown_rules.into_iter().map(|mut request| {
                request.source = Some(terminal);
                request
            }));
        if let Some(mut panel) = result.challenge_panel {
            panel.source = Some(terminal);
            self.panels.write(panel);
        }
        result.economy_operations.into_iter().for_each(|operation| {
            self.economy.write(ScenarioEconomyCommand {
                terminal,
                operation,
            });
        });
        result
            .award_point_adjustments
            .into_iter()
            .for_each(|delta| {
                self.award_points
                    .write(AdjustScenarioAwardPointTotalRequest { delta });
            });
        result
            .challenge_offer_source_paths
            .into_iter()
            .for_each(|source_path| {
                self.challenge_offers
                    .write(CreateScenarioChallengeOfferFromLuaSourcePathRequest { source_path });
            });
    }
}
