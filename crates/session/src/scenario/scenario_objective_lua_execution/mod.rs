use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};

use bevy::ecs::{entity::EntityHashSet, relationship::RelationshipTarget, system::SystemParam};
use bevy::prelude::*;
use openzt2_game_assets::lua_script::{
    lua_script_asset_loading::LuaScriptAsset, lua_script_sources::LuaScriptSources,
};
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_game_data::world_scenario::{ScenarioDefinitionRecord, ScenarioScriptPhase};
use openzt2_game_data::AssetId;
use openzt2_world_state::simulation_time::simulation_control_types::SimulationPaused;
use openzt2_zoo_state::scenario::{
    scenario_objective_types::{
        ScenarioObjective, ScenarioObjectiveDependents, ScenarioObjectiveGroup,
        ScenarioObjectiveGroups, ScenarioObjectiveOf, ScenarioObjectivePrerequisiteFor,
        ScenarioObjectivePrerequisites, ScenarioObjectiveProgress, ScenarioObjectiveRequires,
        ScenarioObjectiveStatus, ScenarioObjectiveStatusChanged, ScenarioObjectives,
    },
    scenario_script_timing_types::ScenarioScriptTiming,
    scenario_session_types::{ActiveScenarioSession, ScenarioDocument, ScenarioSessionState},
};

use super::{
    challenge_offer_types::{ScenarioChallengeOffer, ScenarioChallengeRuleContext},
    scenario_group_completion::{
        is_optional, reduce_group, ScenarioGroupChildOf, ScenarioGroupChildren, ScenarioRootGroup,
    },
    scenario_lua_command_publication::ScenarioLuaOutputs,
    scenario_lua_entity_commands::{ScenarioAnimalSpawnSources, ScenarioLuaDirectAccess},
    scenario_lua_query_fact_collection::ScenarioLuaQueryFactSources,
    scenario_lua_types::{ScenarioLuaContext, ScenarioLuaQueryFacts},
    scenario_lua_virtual_machine::execute_loaded_scenario_lua_script_binding,
};

#[derive(Message, Clone, Copy)]
pub(super) struct ScenarioEvaluationDue {
    owner: Entity,
    scenario: AssetId,
    evaluate: bool,
    always_run: bool,
}

#[derive(Message, Clone, Copy)]
pub(super) struct EvaluatedScenarioTerminal(pub Entity);

#[derive(Component)]
pub(super) struct ScenarioRuleBindings(BTreeMap<(AssetId, AssetId, usize), Vec<usize>>);

fn phase_key(phase: ScenarioScriptPhase) -> usize {
    match phase {
        ScenarioScriptPhase::Validate => 0,
        ScenarioScriptPhase::Evaluate => 1,
        ScenarioScriptPhase::Success => 2,
        ScenarioScriptPhase::Failure => 3,
        ScenarioScriptPhase::AlwaysRun => 4,
    }
}

pub(super) fn rebuild_scenario_rule_binding_indexes(
    mut commands: Commands,
    mut events: MessageReader<AssetEvent<WorldScenarioDocumentAsset>>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    mut owners: Query<
        (
            Entity,
            Ref<ScenarioDocument>,
            Option<&mut ScenarioRuleBindings>,
        ),
        With<ScenarioScriptTiming>,
    >,
) {
    let changed_assets = events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect::<bevy::platform::collections::HashSet<_>>();
    for (entity, source, existing) in &mut owners {
        if existing.is_some() && !source.is_changed() && !changed_assets.contains(&source.0.id()) {
            continue;
        }
        let Some(asset) = assets.get(&source.0) else {
            continue;
        };
        let mut rules = BTreeMap::<_, Vec<usize>>::new();
        for (offset, binding) in asset.document.scenario_script_bindings.iter().enumerate() {
            rules
                .entry((
                    binding.scenario,
                    binding.objective,
                    phase_key(binding.phase),
                ))
                .or_default()
                .push(offset);
        }
        if let Some(mut existing) = existing {
            existing
                .reborrow()
                .map_unchanged(|index| &mut index.0)
                .set_if_neq(rules);
        } else {
            commands.entity(entity).insert(ScenarioRuleBindings(rules));
        }
    }
}

pub(super) fn advance_scenario_script_timers(
    time: Res<Time<Fixed>>,
    simulation: Single<&SimulationPaused>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    mut sessions: Query<(
        Entity,
        &ScenarioDocument,
        Option<&ActiveScenarioSession>,
        Option<&ScenarioChallengeOffer>,
        &mut ScenarioScriptTiming,
    )>,
    mut due: MessageWriter<ScenarioEvaluationDue>,
) {
    for (owner, source, session, offer, mut timing) in &mut sessions {
        let Some(scenario) = session
            .map(|session| session.definition)
            .or_else(|| offer.map(|offer| offer.definition))
        else {
            continue;
        };
        let Some(record) = assets
            .get(&source.0)
            .and_then(|asset| asset.document.find_scenario_record(scenario))
        else {
            continue;
        };
        let evaluate = timing.evaluation_is_due(&time, record.evaluation_interval_seconds)
            && session.is_none_or(|session| session.state == ScenarioSessionState::Running);
        let always_run = !simulation.0 && timing.always_run_is_due(&time);
        if evaluate || always_run {
            due.write(ScenarioEvaluationDue {
                owner,
                scenario,
                evaluate,
                always_run,
            });
        }
    }
}

#[derive(SystemParam)]
pub(super) struct ScenarioRuleQueries<'w, 's> {
    objectives: Query<
        'w,
        's,
        (
            &'static ScenarioObjective,
            &'static mut ScenarioObjectiveStatus,
            &'static ScenarioObjectiveProgress,
            Option<&'static ScenarioObjectivePrerequisites>,
        ),
    >,
    groups: Query<'w, 's, &'static mut ScenarioObjectiveGroup>,
    roots: Query<'w, 's, (), With<ScenarioRootGroup>>,
    children: Query<'w, 's, &'static ScenarioGroupChildren>,
    parents: Query<'w, 's, &'static ScenarioGroupChildOf>,
    dependents: Query<'w, 's, &'static ScenarioObjectiveDependents>,
    edges: Query<
        'w,
        's,
        (
            &'static ScenarioObjectiveRequires,
            &'static ScenarioObjectivePrerequisiteFor,
        ),
    >,
    owners: Query<'w, 's, &'static ScenarioObjectiveOf>,
    owner_rules: Query<
        'w,
        's,
        (
            Option<&'static ScenarioObjectives>,
            Option<&'static ScenarioObjectiveGroups>,
        ),
    >,
}

impl ScenarioRuleQueries<'_, '_> {
    fn objective_status(&self, entity: Entity) -> Option<ScenarioObjectiveStatus> {
        self.objectives
            .as_readonly()
            .get(entity)
            .ok()
            .map(|(_, status, ..)| *status)
    }

    fn activate_if_ready(&mut self, entity: Entity) {
        let ready =
            self.objectives
                .as_readonly()
                .get(entity)
                .is_ok_and(|(_, status, _, prerequisites)| {
                    *status == ScenarioObjectiveStatus::Inactive
                        && prerequisites.is_some_and(|prerequisites| {
                            prerequisites.iter().all(|edge| {
                                self.edges.get(edge).is_ok_and(|(required, _)| {
                                    self.objective_status(required.0)
                                        == Some(ScenarioObjectiveStatus::Satisfied)
                                })
                            })
                        })
                });
        if ready {
            if let Ok((_, mut status, ..)) = self.objectives.get_mut(entity) {
                status.set_if_neq(ScenarioObjectiveStatus::Active);
            }
        }
    }

    fn activate_dependents(&mut self, source: Entity) {
        let Ok(dependents) = self.dependents.get(source) else {
            return;
        };
        // Collect only the dependent leaves of this transition.
        let leaves = dependents
            .iter()
            .filter_map(|edge| self.edges.get(edge).ok().map(|(_, dependent)| dependent.0))
            .collect::<Vec<_>>();
        for dependent in leaves {
            self.activate_if_ready(dependent);
        }
    }

    fn node(
        &self,
        entity: Entity,
        definition: &ScenarioDefinitionRecord,
    ) -> Option<(AssetId, ScenarioObjectiveStatus)> {
        if let Ok((objective, status, ..)) = self.objectives.as_readonly().get(entity) {
            return definition
                .objectives
                .get(objective.record_index as usize)
                .map(|node| (node.id, *status));
        }
        let groups = self.groups.as_readonly();
        let group = groups.get(entity).ok()?;
        definition
            .objective_groups
            .get(group.record_index)
            .map(|node| (node.id, group.status))
    }

    fn runs_when_paused(&self, entity: Entity, scenario: &ScenarioInvocation) -> bool {
        if let Ok((objective, ..)) = self.objectives.as_readonly().get(entity) {
            return scenario
                .definition
                .objectives
                .get(objective.record_index as usize)
                .is_some_and(|node| node.update_when_paused);
        }
        self.node(entity, scenario.definition)
            .is_some_and(|(id, _)| {
                scenario
                    .asset
                    .authored
                    .scenario_nodes
                    .get(&id)
                    .is_some_and(|policy| policy.update_when_paused)
            })
            || self.children.get(entity).is_ok_and(|children| {
                children
                    .iter()
                    .any(|child| self.runs_when_paused(child, scenario))
            })
    }

    fn reduce(
        &mut self,
        entity: Entity,
        scenario: &ScenarioInvocation,
    ) -> Option<(AssetId, ScenarioObjectiveStatus)> {
        let (id, prior) = self.node(entity, scenario.definition)?;
        if scenario.sticky(id, prior, false) {
            return None;
        }
        let groups = self.groups.as_readonly();
        let group = groups.get(entity).ok()?;
        let record = scenario
            .definition
            .objective_groups
            .get(group.record_index)?;
        let statuses = self
            .children
            .get(entity)
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|child| self.node(child, scenario.definition))
            .filter(|(id, _)| !is_optional(&scenario.asset.authored, *id))
            .map(|(_, status)| status);
        let status = reduce_group(record.all_success, statuses);
        if status == prior {
            return None;
        }
        self.groups.get_mut(entity).ok()?.status = status;
        Some((id, status))
    }

    fn evaluate(
        &mut self,
        entity: Entity,
        scenario: &ScenarioInvocation,
        paused: bool,
        scripts: &mut ScenarioRuleScripts,
        facts: ScenarioLuaQueryFacts,
        changed: &mut MessageWriter<ScenarioObjectiveStatusChanged>,
        finished: &mut MessageWriter<EvaluatedScenarioTerminal>,
    ) {
        if paused && !self.runs_when_paused(entity, scenario) {
            return;
        }
        let Some((id, current)) = self.node(entity, scenario.definition) else {
            return;
        };
        if self.groups.as_readonly().contains(entity) {
            if scenario.sticky(id, current, false) {
                return;
            }
            let chain = scenario
                .asset
                .authored
                .scenario_nodes
                .get(&id)
                .is_some_and(|policy| policy.chain);
            let children = self
                .children
                .get(entity)
                .map(|children| children.iter().collect::<Vec<_>>())
                .unwrap_or_default();
            for child in children {
                if paused && !self.runs_when_paused(child, scenario) {
                    continue;
                }
                self.evaluate(child, scenario, paused, scripts, facts, changed, finished);
                if chain
                    && self
                        .node(child, scenario.definition)
                        .is_some_and(|(id, status)| {
                            status != ScenarioObjectiveStatus::Satisfied
                                && !is_optional(&scenario.asset.authored, id)
                        })
                {
                    break;
                }
            }
            if let Some((id, status)) = self.reduce(entity, scenario) {
                scripts.terminal(scenario, scenario.owner, id, status, facts);
            }
            return;
        }

        self.activate_if_ready(entity);
        let Ok((_, mut status, progress, _)) = self.objectives.get_mut(entity) else {
            return;
        };
        if *status == ScenarioObjectiveStatus::Inactive || scenario.sticky(id, *status, true) {
            return;
        }
        let Some(next) = scripts.invoke(scenario, entity, id, ScenarioScriptPhase::Evaluate, facts)
        else {
            return;
        };
        if !status.set_if_neq(next) {
            return;
        }
        changed.write(ScenarioObjectiveStatusChanged {
            objective: entity,
            status: next,
            current: progress.current,
            target: progress.target,
        });
        finished.write(EvaluatedScenarioTerminal(entity));
        scripts.terminal(scenario, entity, id, next, facts);
        if next == ScenarioObjectiveStatus::Satisfied {
            self.activate_dependents(entity);
        }
    }
}

struct ScenarioInvocation<'a> {
    owner: Entity,
    context: Entity,
    source: &'a ScenarioDocument,
    asset: &'a WorldScenarioDocumentAsset,
    bindings: &'a ScenarioRuleBindings,
    definition: &'a ScenarioDefinitionRecord,
}

impl ScenarioInvocation<'_> {
    fn sticky(&self, id: AssetId, status: ScenarioObjectiveStatus, default: bool) -> bool {
        matches!(
            status,
            ScenarioObjectiveStatus::Satisfied | ScenarioObjectiveStatus::Failed
        ) && self
            .asset
            .authored
            .scenario_nodes
            .get(&id)
            .and_then(|policy| policy.sticky)
            .unwrap_or(default)
    }
}

#[derive(SystemParam)]
pub(super) struct ScenarioRuleScripts<'w, 's> {
    assets: Res<'w, Assets<WorldScenarioDocumentAsset>>,
    scripts: Res<'w, Assets<LuaScriptAsset>>,
    index: Res<'w, LuaScriptSources>,
    runtime: Query<'w, 's, &'static mut ScenarioLuaContext>,
    outputs: ScenarioLuaOutputs<'w, 's>,
}

impl ScenarioRuleScripts<'_, '_> {
    fn invoke(
        &mut self,
        scenario: &ScenarioInvocation,
        entity: Entity,
        objective: AssetId,
        phase: ScenarioScriptPhase,
        facts: ScenarioLuaQueryFacts,
    ) -> Option<ScenarioObjectiveStatus> {
        let mut evaluated = None;
        let Ok(mut context) = self.runtime.get_mut(scenario.context) else {
            return None;
        };
        let offsets =
            scenario
                .bindings
                .0
                .get(&(scenario.definition.id, objective, phase_key(phase)))?;
        for &offset in offsets {
            let Some(binding) = scenario.asset.document.scenario_script_bindings.get(offset) else {
                continue;
            };
            if (binding.scenario, binding.objective, binding.phase)
                != (scenario.definition.id, objective, phase)
            {
                continue;
            }
            match execute_loaded_scenario_lua_script_binding(
                &mut context,
                scenario.source,
                &self.assets,
                &self.scripts,
                &self.index,
                binding,
                facts,
            ) {
                Ok(result) => {
                    if let Some(error) = &result.failure {
                        error!(entry = %binding.entry, %error, ?phase, "scenario Lua call failed");
                    } else if phase == ScenarioScriptPhase::Evaluate {
                        evaluated = Some(result.condition_status);
                    }
                    self.outputs.publish(entity, result);
                    if evaluated.is_some_and(|status| status != ScenarioObjectiveStatus::Active) {
                        break;
                    }
                }
                Err(error) => {
                    error!(entry = %binding.entry, %error, ?phase, "scenario Lua call failed")
                }
            }
        }
        evaluated
    }

    fn terminal(
        &mut self,
        scenario: &ScenarioInvocation,
        entity: Entity,
        id: AssetId,
        status: ScenarioObjectiveStatus,
        facts: ScenarioLuaQueryFacts,
    ) {
        let phase = match status {
            ScenarioObjectiveStatus::Satisfied => ScenarioScriptPhase::Success,
            ScenarioObjectiveStatus::Failed => ScenarioScriptPhase::Failure,
            _ => return,
        };
        self.invoke(scenario, entity, id, phase, facts);
    }
}

// The read-only part of script lookup is separate from the VM and output writer
// so one borrowed document can serve every call in a scenario evaluation.
#[derive(SystemParam)]
pub(super) struct ScenarioRuleDocuments<'w, 's> {
    sources: Query<
        'w,
        's,
        (
            &'static ScenarioDocument,
            Option<&'static ScenarioChallengeRuleContext>,
            &'static ScenarioRuleBindings,
        ),
    >,
    assets: Res<'w, Assets<WorldScenarioDocumentAsset>>,
}

impl ScenarioRuleDocuments<'_, '_> {
    fn get(&self, owner: Entity, scenario: AssetId) -> Option<ScenarioInvocation<'_>> {
        let (source, context, bindings) = self.sources.get(owner).ok()?;
        let asset = self.assets.get(&source.0)?;
        Some(ScenarioInvocation {
            owner,
            context: context.map_or(owner, |context| context.0),
            source,
            asset,
            bindings,
            definition: asset.document.find_scenario_record(scenario)?,
        })
    }
}

pub(super) fn evaluate_due_scenario_rules(
    mut due: MessageReader<ScenarioEvaluationDue>,
    simulation: Single<&SimulationPaused>,
    documents: ScenarioRuleDocuments,
    mut rules: ScenarioRuleQueries,
    mut scripts: ScenarioRuleScripts,
    (fact_sources, direct): (ScenarioLuaQueryFactSources, ScenarioLuaDirectAccess),
    spawning: ScenarioAnimalSpawnSources,
    mut changed: MessageWriter<ScenarioObjectiveStatusChanged>,
    mut finished: MessageWriter<EvaluatedScenarioTerminal>,
) {
    if due.is_empty() {
        return;
    }
    let direct = RefCell::new(direct);
    let spawning = RefCell::new(spawning);
    let contained = |entity| fact_sources.contained_entities(entity);
    let waste = |entity, value| direct.borrow_mut().waste(entity, value);
    let profile = |key, value: Option<Option<&str>>| direct.borrow_mut().profile(key, value);
    let spawn_animal = |kind, position, rotation, reserved: &[_]| {
        spawning
            .borrow_mut()
            .resolve(kind, position, rotation, reserved)
    };
    let mut facts = fact_sources.borrow_scenario_lua_query_facts(&contained);
    let selected_photo_set = Cell::new(None);
    facts.selected_photo_set = Some(&selected_photo_set);
    facts.waste = Some(&waste);
    facts.profile = Some(&profile);
    facts.spawn_animal = Some(&spawn_animal);
    let mut always_run = Vec::new();
    for due in due.read() {
        if due.always_run {
            always_run.push((due.owner, due.scenario));
        }
        if !due.evaluate {
            continue;
        }
        let Some(scenario) = documents.get(due.owner, due.scenario) else {
            continue;
        };
        let Ok((leaves, groups)) = rules.owner_rules.get(due.owner) else {
            continue;
        };
        let roots = if let Some(groups) = groups {
            groups
                .iter()
                .filter(|entity| rules.roots.contains(*entity))
                .collect::<Vec<_>>()
        } else {
            leaves
                .into_iter()
                .flat_map(|leaves| leaves.iter())
                .collect()
        };
        for root in roots {
            rules.evaluate(
                root,
                &scenario,
                simulation.0,
                &mut scripts,
                facts,
                &mut changed,
                &mut finished,
            );
        }
    }
    for (owner, definition) in always_run {
        if let Some(scenario) = documents.get(owner, definition) {
            scripts.invoke(
                &scenario,
                owner,
                definition,
                ScenarioScriptPhase::AlwaysRun,
                facts,
            );
        }
    }
}

pub(super) fn resolve_external_scenario_rule_transitions(
    mut changed: MessageReader<ScenarioObjectiveStatusChanged>,
    mut evaluated: MessageReader<EvaluatedScenarioTerminal>,
    documents: ScenarioRuleDocuments,
    mut rules: ScenarioRuleQueries,
    mut scripts: ScenarioRuleScripts,
    (fact_sources, direct): (ScenarioLuaQueryFactSources, ScenarioLuaDirectAccess),
    spawning: ScenarioAnimalSpawnSources,
) {
    if changed.is_empty() {
        return;
    }
    let evaluated = evaluated
        .read()
        .map(|event| event.0)
        .collect::<EntityHashSet>();
    let direct = RefCell::new(direct);
    let spawning = RefCell::new(spawning);
    let contained = |entity| fact_sources.contained_entities(entity);
    let waste = |entity, value| direct.borrow_mut().waste(entity, value);
    let profile = |key, value: Option<Option<&str>>| direct.borrow_mut().profile(key, value);
    let spawn_animal = |kind, position, rotation, reserved: &[_]| {
        spawning
            .borrow_mut()
            .resolve(kind, position, rotation, reserved)
    };
    let mut facts = fact_sources.borrow_scenario_lua_query_facts(&contained);
    let selected_photo_set = Cell::new(None);
    facts.selected_photo_set = Some(&selected_photo_set);
    facts.waste = Some(&waste);
    facts.profile = Some(&profile);
    facts.spawn_animal = Some(&spawn_animal);
    for event in changed.read() {
        if evaluated.contains(&event.objective) {
            continue;
        }
        let Ok(owner) = rules.owners.get(event.objective) else {
            continue;
        };
        let objectives = rules.objectives.as_readonly();
        let Ok((objective, ..)) = objectives.get(event.objective) else {
            continue;
        };
        let Some(scenario) = documents.get(owner.0, objective.scenario) else {
            continue;
        };
        let Some((id, _)) = rules.node(event.objective, scenario.definition) else {
            continue;
        };
        scripts.terminal(&scenario, event.objective, id, event.status, facts);
        if event.status == ScenarioObjectiveStatus::Satisfied {
            rules.activate_dependents(event.objective);
        }
        let mut child = event.objective;
        while let Ok(parent) = rules.parents.get(child) {
            child = parent.0;
            let Some((id, status)) = rules.reduce(child, &scenario) else {
                break;
            };
            scripts.terminal(&scenario, scenario.owner, id, status, facts);
        }
    }
}
