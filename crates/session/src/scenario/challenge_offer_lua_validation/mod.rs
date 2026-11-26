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

#[derive(Component)]
pub(crate) struct PendingChallengeValidation {
    pub(crate) scenario: openzt2_game_data::AssetId,
    pub(crate) document: Handle<WorldScenarioDocumentAsset>,
    pub(crate) script: Handle<LuaScriptAsset>,
}

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
    (mut published, mut failed): (
        MessageReader<AssetEvent<LuaScriptAsset>>,
        MessageReader<bevy::asset::AssetLoadFailedEvent<LuaScriptAsset>>,
    ),
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
            Option<&PendingChallengeValidation>,
            Option<&openzt2_world_state::world_spawn::selected_world_document_handles::SelectedWorldDocumentHandles>,
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
    let Ok((root, selection, mut rng, next_evaluation, instant_decided, pending, handles)) =
        roots.single_mut()
    else {
        return;
    };
    let Some(clock) = fact_sources.zoo_clock(root) else {
        return;
    };
    if selection.mode != WorldSessionMode::Challenge {
        return;
    }
    let changed = published.read().filter_map(|event| match event {
        AssetEvent::LoadedWithDependencies { id } => Some(*id),
        _ => None,
    });
    let mut asset_changes = changed.chain(failed.read().map(|event| event.id));
    let (handle, scenario) = if let Some(pending) = pending {
        let published = asset_changes.any(|id| id == pending.script.id());
        let ready = asset_server.is_loaded_with_dependencies(pending.script.id());
        let failed = matches!(
            asset_server.get_load_state(pending.script.id()),
            Some(bevy::asset::LoadState::Failed(_))
        ) || matches!(
            asset_server.recursive_dependency_load_state(pending.script.id()),
            bevy::asset::RecursiveDependencyLoadState::Failed(_)
        );
        if !ready && !failed && !published {
            return;
        }
        commands.entity(root).remove::<PendingChallengeValidation>();
        if !ready {
            return;
        }
        (pending.document.clone(), pending.scenario)
    } else {
        if next_evaluation
            .as_ref()
            .is_some_and(|deadline| clock.tick < deadline.0)
        {
            return;
        }
        if !offers.is_empty() {
            return;
        }
        let Some(scenarios) = scenarios.get(&scenario_assets) else {
            return;
        };
        let mode = handles
            .and_then(|handles| handles.mode.as_ref())
            .and_then(|mode| scenario_assets.get(mode));
        let manager = mode
            .and_then(|mode| {
                mode.document.scenarios.iter().find(|scenario| {
                    !scenario.flags.contains_all(ScenarioRecordFlags::CHALLENGE)
                        && scenario.evaluation_interval_seconds > 0.0
                })
            })
            .or_else(|| scenarios.scenario(openzt2_game_data::AssetId::from_key("challengegoals")));
        let Some(interval) = manager.and_then(|manager| {
            std::time::Duration::try_from_secs_f32(manager.evaluation_interval_seconds).ok()
        }) else {
            return;
        };
        let interval_ticks = u64::try_from(
            interval
                .as_nanos()
                .div_ceil(fixed_time.timestep().as_nanos().max(1)),
        )
        .unwrap_or(u64::MAX);
        let next_tick = clock.tick.saturating_add(interval_ticks);
        let Some(mut next_evaluation) = next_evaluation else {
            commands
                .entity(root)
                .insert(ScenarioChallengeManagerNextEvaluationTick(next_tick));
            return;
        };
        next_evaluation.0 = next_tick;
        let offer_instant_challenge = if instant_decided {
            false
        } else {
            commands
                .entity(root)
                .insert(ScenarioInstantChallengeDecided);
            fact_sources
                .read_instant_challenge_eligibility(root)
                .is_some_and(|eligibility| eligibility.allows_instant_challenge())
        };
        let candidates = scenarios
            .scenarios()
            .filter(|scenario| scenario.flags.contains_all(ScenarioRecordFlags::CHALLENGE))
            .filter_map(|scenario| {
                let handle = scenarios.handle_for_scenario(scenario.id)?;
                let asset = scenario_assets.get(handle)?;
                let (_, script_path, script) = asset.validation_script(scenario.id)?;
                let instant = script_path
                    .to_ascii_lowercase()
                    .contains(INSTANT_CHALLENGE_SCRIPT_DIRECTORY);
                (instant == offer_instant_challenge).then_some((handle, scenario.id, script))
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return;
        }
        let (document, scenario, script) =
            candidates[rng.generate_next_challenge_selection_value() as usize % candidates.len()];
        if !asset_server.is_loaded_with_dependencies(script.id()) {
            if !matches!(
                asset_server.get_load_state(script.id()),
                Some(bevy::asset::LoadState::Failed(_))
            ) {
                commands.entity(root).insert(PendingChallengeValidation {
                    scenario,
                    document: document.clone(),
                    script: script.clone(),
                });
            }
            return;
        }
        (document.clone(), scenario)
    };
    if !offers.is_empty() {
        return;
    }
    let Some((binding, script_path, script_handle)) = scenario_assets
        .get(&handle)
        .and_then(|asset| asset.validation_script(scenario))
    else {
        return;
    };
    let spawn_sources = std::cell::RefCell::new(spawn_sources);
    let spawn_animal = |kind, position, rotation, reserved: &[_]| {
        spawn_sources
            .borrow_mut()
            .resolve(kind, position, rotation, reserved)
    };
    let contained = |entity| fact_sources.contained_entities(entity);
    let direct = std::cell::RefCell::new(direct);
    let waste = |entity, value| direct.borrow_mut().waste(entity, value);
    let photo_set = |world, set| direct.borrow_mut().current_photo_set(world, set);
    let economy = |world, operation| direct.borrow_mut().economy(world, operation);
    let profile = |key, value: Option<Option<&str>>| direct.borrow_mut().profile(key, value);
    let world_variable = |root, key, value: Option<Option<&str>>| {
        direct.borrow_mut().world_variable(root, key, value)
    };
    let mut queries = fact_sources.borrow_scenario_lua_query_facts(&contained);
    queries.waste = Some(&waste);
    queries.photo_set = Some(&photo_set);
    queries.economy = Some(&economy);
    queries.profile = Some(&profile);
    queries.world_variable = Some(&world_variable);
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
