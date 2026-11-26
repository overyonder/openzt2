use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_zoo_state::progression::award_and_progression_fact_types::AdjustScenarioAwardPointTotalRequest;

use super::{
    challenge_offer_types::CreateScenarioChallengeOfferFromLuaSourcePathRequest,
    scenario_lua_types::ScenarioLuaScriptResult,
};

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct ScenarioLuaOutputs<'w, 's> {
    members: Query<'w, 's, &'static WorldMember>,
    award_points: MessageWriter<'w, AdjustScenarioAwardPointTotalRequest>,
    challenge_offers: MessageWriter<'w, CreateScenarioChallengeOfferFromLuaSourcePathRequest>,
    panels: MessageWriter<'w, super::challenge_offer_types::ScenarioChallengePanelRequest>,
    shown_rules: MessageWriter<'w,
        openzt2_zoo_state::scenario::scenario_objective_types::ShowScenarioRuleRequest,
    >,
    available_content: MessageWriter<'w,
        openzt2_zoo_state::progression::adoption_and_content_availability_types::MakeScenarioContentAvailableRequest,
    >,
    persist_profile_variables: MessageWriter<'w,
        openzt2_zoo_state::progression::profile_scenario_variables::PersistProfileScenarioVariables,
    >,
    camera: MessageWriter<'w, openzt2_interface_state::camera::camera_control_message_types::ScriptedOverheadCameraMotion>,
    fossil: MessageWriter<'w, openzt2_extinct_animals::fossil_site_types::SetFossilPuzzleToFind>,
    ui: MessageWriter<'w, super::scenario_lua_ui::ScenarioUiCommand>,
    entities: MessageWriter<'w, super::scenario_lua_entity_commands::ScenarioEntityCommand>,
    fame: MessageWriter<'w, openzt2_zoo_state::progression::fame_types::SetScenarioMaximumFame>,
}

impl ScenarioLuaOutputs<'_, '_> {
    pub(super) fn write_timed_boundary(
        &mut self,
        command: super::scenario_lua_ui::ScenarioUiCommand,
    ) {
        self.ui.write(command);
    }

    pub(super) fn publish(&mut self, terminal: Entity, result: ScenarioLuaScriptResult) {
        let root = self
            .members
            .get(terminal)
            .map_or(terminal, |member| member.root);
        if let Some(percent) = result.maximum_fame_percent {
            self.fame.write(
                openzt2_zoo_state::progression::fame_types::SetScenarioMaximumFame { percent },
            );
        }
        let _ = self.entities.write_batch(result.entities);
        for mut command in result.ui {
            use super::scenario_lua_ui::ScenarioUiCommand;
            match command {
                ScenarioUiCommand::OverheadMotion(operation) => {
                    self.camera.write(openzt2_interface_state::camera::camera_control_message_types::ScriptedOverheadCameraMotion { root, operation });
                }
                ScenarioUiCommand::FossilPuzzle(puzzle) => {
                    self.fossil.write(
                        openzt2_extinct_animals::fossil_site_types::SetFossilPuzzleToFind {
                            root,
                            puzzle,
                        },
                    );
                }
                _ => {
                    if let ScenarioUiCommand::ZooMessage { root: owner, .. } = &mut command {
                        *owner = Some(root);
                    }
                    self.ui.write(command);
                }
            }
        }
        if result.persist_profile_variables {
            self.persist_profile_variables.write(openzt2_zoo_state::progression::profile_scenario_variables::PersistProfileScenarioVariables);
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
        result
            .award_point_adjustments
            .into_iter()
            .for_each(|delta| {
                self.award_points
                    .write(AdjustScenarioAwardPointTotalRequest { root, delta });
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
