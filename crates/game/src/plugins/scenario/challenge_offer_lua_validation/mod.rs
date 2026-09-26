use bevy::prelude::*;
use openzt2_game_data::world_scenario::{ScenarioRecordFlags, ScenarioScriptPhase};

use crate::assets::lua_script::lua_script_archive_path_index::LuaScriptArchivePathIndex;
use crate::assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::game_session_types::WorldSessionMode;
use crate::plugins::economy::guest_admission_types::AdmissionPrice;
use crate::plugins::economy::scenario_economy_command_types::ScenarioEconomyCommand;
use crate::plugins::economy::zoo_cash_types::ZooCash;
use crate::plugins::progression::award_and_progression_fact_types::AdjustScenarioAwardPointTotalRequest;
use crate::plugins::progression::fame_types::Fame;
use crate::plugins::simulation_time::simulation_clock_types::ZooCalendar;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use crate::plugins::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::{
    challenge_offer_types::{
        CreateScenarioChallengeOfferFromLuaSourcePathRequest, ScenarioChallengeLuaValidationDay,
        ScenarioChallengeOffer, ScenarioChallengeSelectionRandomNumberGenerator,
    },
    scenario_lua_command_publication::publish_commands_produced_by_scenario_lua_script,
    scenario_lua_query_fact_collection::count_live_world_entities_by_definition,
    scenario_lua_types::{ScenarioLuaQueryFacts, ScenarioLuaVirtualMachineContexts},
    scenario_lua_virtual_machine::execute_loaded_scenario_lua_script_binding,
    scenario_session_types::SelectedScenarioDocument,
};

pub(super) fn evaluate_daily_lua_validation_scripts_until_one_challenge_is_offered(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    scenarios: Res<WorldScenarios>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    cash: Res<ZooCash>,
    admission: Res<AdmissionPrice>,
    fame: Res<Fame>,
    clock: Res<ZooClock>,
    calendar: Res<ZooCalendar>,
    definitions: Query<&DefinitionId>,
    mut roots: Query<
        (
            Entity,
            &SelectedWorldIdentity,
            &mut ScenarioChallengeSelectionRandomNumberGenerator,
            Option<&mut ScenarioChallengeLuaValidationDay>,
        ),
        With<WorldLoadCompleted>,
    >,
    offers: Query<(), With<ScenarioChallengeOffer>>,
    mut outputs: (
        MessageWriter<ScenarioEconomyCommand>,
        MessageWriter<AdjustScenarioAwardPointTotalRequest>,
        MessageWriter<CreateScenarioChallengeOfferFromLuaSourcePathRequest>,
        MessageWriter<super::challenge_offer_types::ScenarioChallengePanelRequest>,
    ),
) {
    if !offers.is_empty() {
        return;
    }
    let Ok((root, selection, mut rng, validated)) = roots.single_mut() else {
        return;
    };
    if selection.mode != WorldSessionMode::Challenge
        || validated
            .as_deref()
            .is_some_and(|validated| validated.0 == clock.absolute_day)
    {
        return;
    }
    if scenarios.has_pending_document_loads(&asset_server, &scenario_assets)
        || script_index.has_pending_script_loads(&asset_server, &scripts)
    {
        return;
    }
    let Some(scenarios) = scenarios.get(&scenario_assets) else {
        return;
    };
    let mut candidates = scenarios
        .scenarios()
        .filter(|scenario| scenario.flags.contains_all(ScenarioRecordFlags::CHALLENGE))
        .filter_map(|scenario| {
            let handle = scenarios.handle_for_scenario(scenario.id)?;
            let asset = scenario_assets.get(handle)?;
            let binding = asset
                .document
                .scenario_script_bindings
                .iter()
                .find(|binding| {
                    binding.scenario == scenario.id
                        && binding.phase == ScenarioScriptPhase::Validate
                })?;
            Some((handle.clone(), binding.clone()))
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return;
    }
    if let Some(mut validated) = validated {
        validated.0 = clock.absolute_day;
    } else {
        commands
            .entity(root)
            .insert(ScenarioChallengeLuaValidationDay(clock.absolute_day));
    }
    let start = rng.generate_next_challenge_selection_value() as usize % candidates.len();
    candidates.rotate_left(start);
    let counts = count_live_world_entities_by_definition(&definitions);
    let queries = ScenarioLuaQueryFacts {
        zoo_cash: &cash,
        guest_admission_price: &admission,
        zoo_fame: &fame,
        zoo_clock: Some(&clock),
        zoo_calendar: Some(&calendar),
        live_world_entity_counts_by_definition_identifier: &counts,
    };
    for (handle, binding) in candidates {
        let result = execute_loaded_scenario_lua_script_binding(
            &mut runtime,
            &SelectedScenarioDocument(handle),
            &scenario_assets,
            &scripts,
            &script_index,
            &binding,
            queries,
        );
        match result {
            Ok(result) => {
                let offered = !result.challenge_offer_source_paths.is_empty();
                publish_commands_produced_by_scenario_lua_script(
                    root,
                    result,
                    &mut outputs.0,
                    &mut outputs.1,
                    &mut outputs.2,
                    &mut outputs.3,
                );
                if offered {
                    break;
                }
            }
            Err(error) => {
                error!(entry = %binding.entry, %error, "challenge validation Lua call failed");
            }
        }
    }
}
