use bevy::prelude::*;
use openzt2_game_data::world_scenario::ScenarioRecordFlags;

use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_assets::lua_script::lua_script_sources::LuaScriptSources;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;

use openzt2_zoo_state::scenario::scenario_session_types::ScenarioDocument;

use super::{
    challenge_offer_types::{
        ScenarioChallengeManagerNextEvaluationTick, ScenarioChallengeOffer,
        ScenarioChallengeSelectionRandomNumberGenerator, ScenarioInstantChallengeDecided,
    },
    scenario_lua_command_publication::ScenarioLuaOutputs,
    scenario_lua_query_fact_collection::ScenarioLuaQueryFactSources,
    scenario_lua_types::ScenarioLuaContext,
};

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
    script_sources: Res<LuaScriptSources>,
    mut runtime: Query<&mut ScenarioLuaContext>,
    (fact_sources, direct): (
        ScenarioLuaQueryFactSources,
        super::scenario_lua_entity_commands::ScenarioLuaDirectAccess,
    ),
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
    contexts: Query<(
        Entity,
        &super::challenge_offer_types::ScenarioChallengeLuaCandidate,
        &openzt2_world_state::world_spawn::world_membership_types::WorldMember,
    )>,
    mut outputs: ScenarioLuaOutputs,
) {
    let Ok((root, selection, mut rng, next_evaluation, instant_decided)) = roots.single_mut()
    else {
        return;
    };
    let Some(clock) = fact_sources.zoo_clock(root) else {
        return;
    };
    if selection.mode != WorldSessionMode::Challenge {
        return;
    }
    if scenarios.has_pending_document_loads(&asset_server, &scenario_assets) {
        return;
    }
    let Some(scenarios) = scenarios.get(&scenario_assets) else {
        return;
    };
    let Some(interval) = scenarios
        .scenario(openzt2_game_data::AssetId::from_key("challengegoals"))
        .and_then(|manager| {
            std::time::Duration::try_from_secs_f32(manager.evaluation_interval_seconds).ok()
        })
    else {
        return;
    };
    let interval_ticks =
        u64::try_from(interval.as_nanos() / fixed_time.timestep().as_nanos().max(1))
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
    if !offers.is_empty() {
        next_evaluation.0 = next_tick;
        return;
    }
    let mut candidates = scenarios
        .scenarios()
        .filter(|scenario| scenario.flags.contains_all(ScenarioRecordFlags::CHALLENGE))
        .filter_map(|scenario| {
            let handle = scenarios.handle_for_scenario(scenario.id)?;
            let asset = scenario_assets.get(handle)?;
            let (binding, path, script) = asset.validation_script(scenario.id)?;
            Some((handle, binding, path, script))
        })
        .collect::<Vec<_>>();
    if candidates.iter().any(|(_, _, _, script)| {
        scripts.get(*script).is_none()
            && !matches!(
                asset_server.get_load_state(script.id()),
                Some(bevy::asset::LoadState::Failed(_))
            )
    }) {
        return;
    }
    next_evaluation.0 = next_tick;
    candidates.retain(|(_, _, _, script)| scripts.get(*script).is_some());
    // `checkchallenges` decides the instant challenge once, on its first
    // evaluation after priming. Every later pick comes from the others.
    let offer_instant_challenge = if instant_decided {
        false
    } else {
        commands
            .entity(root)
            .insert(ScenarioInstantChallengeDecided);
        let eligibility = fact_sources.read_instant_challenge_eligibility(root);
        let allowed = eligibility.is_some_and(|eligibility| eligibility.allows_instant_challenge());
        info!(
            tick = clock.tick,
            ?eligibility,
            allowed,
            "instant challenge decided"
        );
        allowed
    };
    candidates.retain(|(_, binding, _, _)| {
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
    let (handle, binding, script_path, script_handle) = candidates
        .swap_remove(rng.generate_next_challenge_selection_value() as usize % candidate_count);
    let spawn_sources = std::cell::RefCell::new(spawn_sources);
    let spawn_animal = |kind, position, rotation, reserved: &[_]| {
        spawn_sources
            .borrow_mut()
            .resolve(kind, position, rotation, reserved)
    };
    let contained = |entity| fact_sources.contained_entities(entity);
    let direct = std::cell::RefCell::new(direct);
    let waste = |entity, value| direct.borrow_mut().waste(entity, value);
    let profile = |key, value: Option<Option<&str>>| direct.borrow_mut().profile(key, value);
    let mut queries = fact_sources.borrow_scenario_lua_query_facts(&contained);
    queries.waste = Some(&waste);
    queries.profile = Some(&profile);
    queries.spawn_animal = Some(&spawn_animal);
    let owner = contexts
        .iter()
        .find(|(_, candidate, member)| candidate.0 == binding.scenario && member.root == root)
        .map(|(entity, _, _)| entity);
    let source = ScenarioDocument(handle.clone());
    let execute = |context: &mut ScenarioLuaContext| {
        context.execute(
            Some(handle.id()),
            script_path,
            script_handle,
            &scripts,
            &script_sources,
            binding,
            queries,
            None,
        )
    };
    let result = if let Some(mut context) = owner.and_then(|entity| runtime.get_mut(entity).ok()) {
        execute(&mut context)
    } else {
        let mut context = ScenarioLuaContext::default();
        let result = execute(&mut context);
        if let Some(entity) = owner {
            commands.entity(entity).insert(context);
        } else {
            commands.spawn((
                openzt2_world_state::world_spawn::world_membership_types::WorldMember { root },
                super::challenge_offer_types::ScenarioChallengeLuaCandidate(binding.scenario),
                source,
                context,
            ));
        }
        result
    };
    match result {
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
            outputs.publish(root, result);
        }
        Err(error) => {
            error!(script = %binding.script, %error, "challenge validation Lua call failed");
        }
    }
}
