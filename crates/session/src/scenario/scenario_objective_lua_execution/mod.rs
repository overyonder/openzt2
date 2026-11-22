use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::world_scenario::ScenarioScriptPhase;

use openzt2_game_assets::lua_script::lua_script_archive_path_index::LuaScriptArchivePathIndex;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveProgress;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatusChanged;
use openzt2_zoo_state::scenario::scenario_objective_types::{
    ScenarioObjective, ScenarioObjectiveGroup, ScenarioObjectiveGroups, ScenarioObjectiveOf,
    ScenarioObjectivePrerequisites, ScenarioObjectiveRequires, ScenarioObjectives,
};
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioDocument;
use openzt2_zoo_state::scenario::{
    scenario_script_timing_types::ScenarioScriptTiming,
    scenario_session_types::{ActiveScenarioSession, ScenarioSessionState},
};

use super::{
    challenge_offer_types::ScenarioChallengeOffer,
    scenario_lua_command_publication::ScenarioLuaOutputs,
    scenario_lua_query_fact_collection::ScenarioLuaQueryFactSources,
    scenario_lua_types::ScenarioLuaVirtualMachineContexts,
    scenario_lua_virtual_machine::execute_loaded_scenario_lua_script_binding,
};

#[derive(Message, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct EvaluatedScenarioTerminal(pub Entity);

pub(super) fn evaluate_active_scenario_objective_lua_condition_scripts(
    sources: Query<(
        Entity,
        &ScenarioDocument,
        Option<&super::challenge_offer_types::ScenarioChallengeRuleContext>,
    )>,
    time: Res<Time<Fixed>>,
    simulation: Res<
        openzt2_world_state::simulation_time::simulation_control_types::SimulationControl,
    >,
    mut sessions: Query<(
        Entity,
        &ScenarioDocument,
        Option<&ActiveScenarioSession>,
        Option<&ScenarioChallengeOffer>,
        &mut ScenarioScriptTiming,
    )>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    (fact_sources, direct): (
        ScenarioLuaQueryFactSources,
        super::scenario_lua_entity_commands::ScenarioLuaDirectAccess,
    ),
    spawn_sources: super::scenario_lua_entity_commands::ScenarioAnimalSpawnSources,
    members: Query<&ScenarioObjectives>,
    edges: Query<&ScenarioObjectiveRequires>,
    mut objectives: Query<(
        Entity,
        &ScenarioObjective,
        &mut ScenarioObjectiveStatus,
        &ScenarioObjectiveProgress,
        &ScenarioObjectiveOf,
        Option<&ScenarioObjectivePrerequisites>,
    )>,
    mut changed: MessageWriter<ScenarioObjectiveStatusChanged>,
    mut finished: MessageWriter<EvaluatedScenarioTerminal>,
    mut outputs: ScenarioLuaOutputs,
) {
    let mut evaluation_due = std::collections::BTreeMap::new();
    let mut always_run_due = Vec::new();
    for (entity, source, session, offer, mut timing) in &mut sessions {
        let Some(scenario) = session
            .map(|session| session.definition)
            .or_else(|| offer.map(|offer| offer.definition))
        else {
            continue;
        };
        let Some(definition) = scenario_assets
            .get(&source.0)
            .and_then(|asset| asset.document.find_scenario_record(scenario))
        else {
            continue;
        };
        let evaluate = timing.evaluation_is_due(&time, definition.evaluation_interval_seconds);
        let always_run = !simulation.paused && timing.always_run_is_due(&time);
        evaluation_due.insert(
            entity,
            evaluate
                && session.is_none_or(|session| session.state == ScenarioSessionState::Running),
        );
        if always_run {
            always_run_due.push((entity, scenario));
        }
    }
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
    for (owner, source, context) in &sources {
        let Ok(members) = members.get(owner) else {
            continue;
        };
        let mut ordered = objectives
            .as_readonly()
            .iter_many(members.iter())
            .flatten()
            .map(|(entity, objective, ..)| (objective.record_index, entity))
            .collect::<Vec<_>>();
        ordered.sort_unstable_by_key(|(index, _)| *index);
        for (_, entity) in ordered {
            let activate = objectives.as_readonly().get(entity).is_ok_and(
                |(_, _, status, _, _, prerequisites)| {
                    *status == ScenarioObjectiveStatus::Inactive
                        && prerequisites.is_some_and(|prerequisites| {
                            prerequisites.iter().all(|edge| {
                                edges.get(edge).is_ok_and(|required| {
                                    objectives.as_readonly().get(required.0).is_ok_and(
                                        |(_, _, status, ..)| {
                                            *status == ScenarioObjectiveStatus::Satisfied
                                        },
                                    )
                                })
                            })
                        })
                },
            );
            let Ok((entity, objective, mut status, progress, _, _)) = objectives.get_mut(entity)
            else {
                continue;
            };
            if activate {
                status.set_if_neq(ScenarioObjectiveStatus::Active);
            }
            if *status != ScenarioObjectiveStatus::Active
                || evaluation_due.get(&owner) == Some(&false)
            {
                continue;
            }
            let Some(asset) = scenario_assets.get(&source.0) else {
                continue;
            };
            let Some(record) = asset
                .document
                .find_scenario_objective(objective.scenario, objective.record_index)
            else {
                continue;
            };
            if simulation.paused && !record.update_when_paused {
                continue;
            }
            for binding in asset
                .document
                .scenario_script_bindings
                .iter()
                .filter(|binding| {
                    (binding.scenario, binding.objective, binding.phase)
                        == (objective.scenario, record.id, ScenarioScriptPhase::Evaluate)
                })
            {
                match execute_loaded_scenario_lua_script_binding(
                    &mut runtime,
                    context.map_or(owner, |context| context.0),
                    source,
                    &scenario_assets,
                    &scripts,
                    &script_index,
                    binding,
                    queries,
                ) {
                    Ok(result) if result.failure.is_some() => {
                        if let Some(error) = &result.failure {
                            error!(entry = %binding.entry, %error, "scenario Lua call failed");
                        }
                        outputs.publish(entity, result);
                    }
                    Ok(result) if result.condition_status != ScenarioObjectiveStatus::Active => {
                        let next_status = result.condition_status;
                        outputs.publish(entity, result);
                        *status = next_status;
                        changed.write(ScenarioObjectiveStatusChanged {
                            objective: entity,
                            status: *status,
                            current: progress.current,
                            target: progress.target,
                        });
                        finished.write(EvaluatedScenarioTerminal(entity));
                        let phase = if next_status == ScenarioObjectiveStatus::Satisfied {
                            ScenarioScriptPhase::Success
                        } else {
                            ScenarioScriptPhase::Failure
                        };
                        for terminal in
                            asset
                                .document
                                .scenario_script_bindings
                                .iter()
                                .filter(|binding| {
                                    (binding.scenario, binding.objective, binding.phase)
                                        == (objective.scenario, record.id, phase)
                                })
                        {
                            match execute_loaded_scenario_lua_script_binding(
                                &mut runtime,
                                context.map_or(owner, |context| context.0),
                                source,
                                &scenario_assets,
                                &scripts,
                                &script_index,
                                terminal,
                                queries,
                            ) {
                                Ok(result) => {
                                    if let Some(error) = &result.failure {
                                        error!(entry = %terminal.entry, %error, "scenario Lua terminal call failed");
                                    }
                                    outputs.publish(entity, result);
                                }
                                Err(error) => {
                                    error!(entry = %terminal.entry, %error, "scenario Lua terminal call failed")
                                }
                            }
                        }
                        break;
                    }
                    Ok(result) => outputs.publish(entity, result),
                    Err(error) => {
                        error!(entry = %binding.entry, %error, "scenario Lua call failed")
                    }
                }
                if *status != ScenarioObjectiveStatus::Active {
                    break;
                }
            }
        }
    }
    for (entity, scenario) in always_run_due {
        let Ok((_, source, context)) = sources.get(entity) else {
            continue;
        };
        let Some(asset) = scenario_assets.get(&source.0) else {
            continue;
        };
        for binding in asset
            .document
            .scenario_script_bindings
            .iter()
            .filter(|binding| {
                binding.scenario == scenario && binding.phase == ScenarioScriptPhase::AlwaysRun
            })
        {
            match execute_loaded_scenario_lua_script_binding(
                &mut runtime,
                context.map_or(entity, |context| context.0),
                source,
                &scenario_assets,
                &scripts,
                &script_index,
                binding,
                queries,
            ) {
                Ok(result) => {
                    if let Some(error) = &result.failure {
                        error!(entry = %binding.entry, %error, "scenario Lua always-run call failed");
                    }
                    outputs.publish(entity, result);
                }
                Err(error) => {
                    error!(entry = %binding.entry, %error, "scenario Lua always-run call failed")
                }
            }
        }
    }
}

pub(super) fn execute_scenario_objective_success_and_failure_lua_scripts(
    sources: Query<(
        &ScenarioDocument,
        Option<&super::challenge_offer_types::ScenarioChallengeRuleContext>,
    )>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    (fact_sources, direct): (
        ScenarioLuaQueryFactSources,
        super::scenario_lua_entity_commands::ScenarioLuaDirectAccess,
    ),
    spawn_sources: super::scenario_lua_entity_commands::ScenarioAnimalSpawnSources,
    objectives: Query<(
        &ScenarioObjective,
        &ScenarioObjectiveStatus,
        &ScenarioObjectiveOf,
    )>,
    members: Query<(&ScenarioObjectives, &ScenarioObjectiveGroups)>,
    mut groups: Query<&mut ScenarioObjectiveGroup>,
    mut changed: MessageReader<ScenarioObjectiveStatusChanged>,
    mut evaluated: MessageReader<EvaluatedScenarioTerminal>,
    mut outputs: ScenarioLuaOutputs,
) {
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
    let evaluated = evaluated
        .read()
        .map(|event| event.0)
        .collect::<bevy::platform::collections::HashSet<_>>();
    let mut invocations = Vec::new();
    let mut affected_scenarios = std::collections::BTreeMap::new();
    for event in changed.read() {
        let Ok((objective, _, owner)) = objectives.get(event.objective) else {
            continue;
        };
        let phase = match event.status {
            ScenarioObjectiveStatus::Satisfied => ScenarioScriptPhase::Success,
            ScenarioObjectiveStatus::Failed => ScenarioScriptPhase::Failure,
            ScenarioObjectiveStatus::Inactive | ScenarioObjectiveStatus::Active => continue,
        };
        let Ok((source, _)) = sources.get(owner.0) else {
            continue;
        };
        let Some(asset) = scenario_assets.get(&source.0) else {
            continue;
        };
        let Some(record) = asset
            .document
            .find_scenario_objective(objective.scenario, objective.record_index)
        else {
            continue;
        };
        if !evaluated.contains(&event.objective) {
            invocations.push((
                event.objective,
                owner.0,
                objective.scenario,
                record.id,
                phase,
            ));
        }
        affected_scenarios.insert(owner.0, (objective.scenario, event.objective));
    }
    for (scenario_owner, (scenario, source_entity)) in affected_scenarios {
        let Ok((source, _)) = sources.get(scenario_owner) else {
            continue;
        };
        let Some(asset) = scenario_assets.get(&source.0) else {
            continue;
        };
        let Some(definition) = asset.document.find_scenario_record(scenario) else {
            continue;
        };
        let Ok((members, group_entities)) = members.get(scenario_owner) else {
            continue;
        };
        let mut statuses = std::collections::BTreeMap::new();
        for (objective, status, _) in objectives.iter_many(members.iter()).flatten() {
            if let Some(record) = definition.objectives.get(objective.record_index as usize) {
                statuses.insert(record.id, *status);
            }
        }
        // Groups are created in authored postorder so nested children commit first.
        for entity in group_entities.iter() {
            let Ok(mut live) = groups.get_mut(entity) else {
                continue;
            };
            let Some(group) = definition.objective_groups.get(live.record_index) else {
                continue;
            };
            let current = super::scenario_group_completion::derive_group_status(group, &statuses);
            statuses.insert(group.id, current);
            if live.status == current {
                continue;
            }
            live.status = current;
            let phase = match current {
                ScenarioObjectiveStatus::Satisfied => ScenarioScriptPhase::Success,
                ScenarioObjectiveStatus::Failed => ScenarioScriptPhase::Failure,
                _ => continue,
            };
            invocations.push((source_entity, scenario_owner, scenario, group.id, phase));
        }
    }
    for (source_entity, owner, scenario, objective_id, phase) in invocations {
        let Ok((source, context)) = sources.get(owner) else {
            continue;
        };
        let Some(asset) = scenario_assets.get(&source.0) else {
            continue;
        };
        for binding in asset
            .document
            .scenario_script_bindings
            .iter()
            .filter(|binding| {
                (binding.scenario, binding.objective, binding.phase)
                    == (scenario, objective_id, phase)
            })
        {
            match execute_loaded_scenario_lua_script_binding(
                &mut runtime,
                context.map_or(owner, |context| context.0),
                source,
                &scenario_assets,
                &scripts,
                &script_index,
                binding,
                queries,
            ) {
                Ok(result) => {
                    if let Some(error) = &result.failure {
                        error!(entry = %binding.entry, %error, "scenario Lua terminal call failed");
                    }
                    outputs.publish(source_entity, result);
                }
                Err(error) => {
                    error!(entry = %binding.entry, %error, "scenario Lua terminal call failed")
                }
            }
        }
    }
}
