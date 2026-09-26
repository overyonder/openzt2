use bevy::prelude::*;

use crate::plugins::{
    economy::scenario_economy_command_types::ScenarioEconomyCommand,
    progression::award_and_progression_fact_types::AdjustScenarioAwardPointTotalRequest,
};

use super::{
    challenge_offer_types::CreateScenarioChallengeOfferFromLuaSourcePathRequest,
    scenario_lua_types::ScenarioLuaScriptResult,
};

pub(super) fn publish_commands_produced_by_scenario_lua_script(
    terminal: Entity,
    result: ScenarioLuaScriptResult,
    economy: &mut MessageWriter<ScenarioEconomyCommand>,
    award_points: &mut MessageWriter<AdjustScenarioAwardPointTotalRequest>,
    challenge_offers: &mut MessageWriter<CreateScenarioChallengeOfferFromLuaSourcePathRequest>,
    panels: &mut MessageWriter<super::challenge_offer_types::ScenarioChallengePanelRequest>,
) {
    if let Some(panel) = result.challenge_panel {
        panels.write(panel);
    }
    result.economy_operations.into_iter().for_each(|operation| {
        economy.write(ScenarioEconomyCommand {
            terminal,
            operation,
        });
    });
    result
        .award_point_adjustments
        .into_iter()
        .for_each(|delta| {
            award_points.write(AdjustScenarioAwardPointTotalRequest { delta });
        });
    result
        .challenge_offer_source_paths
        .into_iter()
        .for_each(|source_path| {
            challenge_offers
                .write(CreateScenarioChallengeOfferFromLuaSourcePathRequest { source_path });
        });
}
