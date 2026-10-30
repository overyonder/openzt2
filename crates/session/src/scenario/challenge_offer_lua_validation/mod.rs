use bevy::prelude::*;
use openzt2_game_data::world_scenario::{ScenarioRecordFlags, ScenarioScriptPhase};

use openzt2_game_assets::lua_script::lua_script_archive_path_index::LuaScriptArchivePathIndex;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_zoo_state::economy::scenario_economy_command_types::ScenarioEconomyCommand;
use openzt2_zoo_state::progression::award_and_progression_fact_types::AdjustScenarioAwardPointTotalRequest;

use openzt2_zoo_state::scenario::scenario_session_types::SelectedScenarioDocument;

use super::{
    challenge_offer_types::{
        CreateScenarioChallengeOfferFromLuaSourcePathRequest,
        ScenarioChallengeManagerNextEvaluationTick, ScenarioChallengeOffer,
        ScenarioChallengeSelectionRandomNumberGenerator, ScenarioInstantChallengeDecided,
    },
    scenario_lua_command_publication::publish_commands_produced_by_scenario_lua_script,
    scenario_lua_query_fact_collection::ScenarioLuaQueryFactSources,
    scenario_lua_types::ScenarioLuaVirtualMachineContexts,
    scenario_lua_virtual_machine::execute_loaded_scenario_lua_script_binding,
};

/// Challenge mode's authored scenario manager (`scenario/goals/challengegoals.xml`)
/// has `interval="4"`. Its rule calls `decidechallengeset`, which measures its
/// waits in fractions of a month (a `BFS_GET_CHALLENGE_DELAY` default of 0.2
/// and `checkTimeLimit` margins of 0.03), so it must run many times a month.
/// The interval is read as seconds of the authored 100 Hz simulation tick.
const CHALLENGE_MANAGER_EVALUATION_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(4);

/// Script directory of the challenges `checkchallenges` offers to a new zoo.
const INSTANT_CHALLENGE_SCRIPT_DIRECTORY: &str = "scenario/scripts/challenge/instant/";

/// Runs one challenge-manager evaluation every authored interval. The first
/// evaluation only primes `checkchallenges`. The second decides the new zoo's
/// one instant challenge, and later ones pick one other challenge, as
/// `BFS_PICKCHALLENGE` does, and run its `validate`. Nothing is picked while an
/// offer or accepted challenge exists.
pub(super) fn evaluate_challenge_manager_and_validate_one_picked_challenge(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    scenarios: Res<WorldScenarios>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    fact_sources: ScenarioLuaQueryFactSources,
    spawn_sources: super::scenario_lua_entity_commands::ScenarioAnimalSpawnSources,
    fixed_time: Res<Time<Fixed>>,
    mut roots: Query<
        (
            Entity,
            &SelectedWorldIdentity,
            &mut ScenarioChallengeSelectionRandomNumberGenerator,
            Option<&mut ScenarioChallengeManagerNextEvaluationTick>,
            Has<ScenarioInstantChallengeDecided>,
        ),
        With<WorldLoadCompleted>,
    >,
    offers: Query<(), With<ScenarioChallengeOffer>>,
    mut outputs: (
        MessageWriter<ScenarioEconomyCommand>,
        MessageWriter<AdjustScenarioAwardPointTotalRequest>,
        MessageWriter<CreateScenarioChallengeOfferFromLuaSourcePathRequest>,
        MessageWriter<super::challenge_offer_types::ScenarioChallengePanelRequest>,
        MessageWriter<openzt2_zoo_state::scenario::scenario_objective_types::ShowScenarioRuleRequest>,
        MessageWriter<openzt2_zoo_state::progression::adoption_and_content_availability_types::MakeScenarioContentAvailableRequest>,
        MessageWriter<super::scenario_photo_challenge_selection::SelectCurrentPhotoChallengeSetRequest>,
        MessageWriter<openzt2_zoo_state::progression::profile_scenario_variables::SetProfileScenarioVariable>,
        MessageWriter<openzt2_zoo_state::progression::profile_scenario_variables::PersistProfileScenarioVariables>,
        MessageWriter<super::scenario_lua_ui::ScenarioUiCommand>,
        MessageWriter<super::scenario_lua_entity_commands::ScenarioEntityCommand>,
        MessageWriter<openzt2_zoo_state::progression::fame_types::SetScenarioMaximumFame>,
    ),
) {
    let Some(clock) = fact_sources.zoo_clock() else {
        return;
    };
    let Ok((root, selection, mut rng, next_evaluation, instant_decided)) = roots.single_mut()
    else {
        return;
    };
    if selection.mode != WorldSessionMode::Challenge {
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
    let interval_ticks = u64::try_from(
        CHALLENGE_MANAGER_EVALUATION_INTERVAL.as_nanos() / fixed_time.timestep().as_nanos().max(1),
    )
    .unwrap_or(u64::MAX);
    let next_tick = clock.tick.saturating_add(interval_ticks);
    let Some(mut next_evaluation) = next_evaluation else {
        // The first evaluation sets `firstTick` and gives nothing.
        commands
            .entity(root)
            .insert(ScenarioChallengeManagerNextEvaluationTick(next_tick));
        info!(
            tick = clock.tick,
            next_tick, "challenge manager primed; its first decision follows one interval"
        );
        return;
    };
    if clock.tick < next_evaluation.0 {
        return;
    }
    next_evaluation.0 = next_tick;
    if !offers.is_empty() {
        return;
    }
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
    // `checkchallenges` decides the instant challenge once, on its first
    // evaluation after priming. Every later pick comes from the others.
    let offer_instant_challenge = if instant_decided {
        false
    } else {
        commands
            .entity(root)
            .insert(ScenarioInstantChallengeDecided);
        let eligibility = fact_sources.read_instant_challenge_eligibility();
        let allowed = eligibility.is_some_and(|eligibility| eligibility.allows_instant_challenge());
        info!(
            tick = clock.tick,
            ?eligibility,
            allowed,
            "instant challenge decided"
        );
        allowed
    };
    candidates.retain(|(_, binding)| {
        binding
            .script
            .to_ascii_lowercase()
            .contains(INSTANT_CHALLENGE_SCRIPT_DIRECTORY)
            == offer_instant_challenge
    });
    if candidates.is_empty() {
        info!(
            tick = clock.tick,
            instant = offer_instant_challenge,
            "challenge manager has no challenge to pick"
        );
        return;
    }
    let candidate_count = candidates.len();
    let (handle, binding) = candidates
        .swap_remove(rng.generate_next_challenge_selection_value() as usize % candidate_count);
    let spawn_sources = std::cell::RefCell::new(spawn_sources);
    let spawn_animal = |kind, position, rotation, reserved: &[_]| {
        spawn_sources
            .borrow_mut()
            .resolve(kind, position, rotation, reserved)
    };
    let mut queries = fact_sources.borrow_scenario_lua_query_facts();
    queries.spawn_animal = Some(&spawn_animal);
    match execute_loaded_scenario_lua_script_binding(
        &mut runtime,
        &SelectedScenarioDocument(handle),
        &scenario_assets,
        &scripts,
        &script_index,
        &binding,
        queries,
    ) {
        Ok(result) => {
            if let Some(error) = &result.failure {
                error!(script = %binding.script, %error, "challenge validation Lua call failed");
            }
            info!(
                tick = clock.tick,
                instant = offer_instant_challenge,
                candidate_count,
                script = %binding.script,
                offered = ?result.challenge_offer_source_paths,
                "challenge validation ran"
            );
            publish_commands_produced_by_scenario_lua_script(
                root,
                result,
                &mut outputs.0,
                &mut outputs.1,
                &mut outputs.2,
                &mut outputs.3,
                &mut outputs.4,
                &mut outputs.5,
                &mut outputs.6,
                &mut outputs.7,
                &mut outputs.8,
                &mut outputs.9,
                &mut outputs.10,
                &mut outputs.11,
            );
        }
        Err(error) => {
            error!(script = %binding.script, %error, "challenge validation Lua call failed");
        }
    }
}
