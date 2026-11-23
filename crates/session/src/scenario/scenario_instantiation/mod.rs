use bevy::prelude::*;
use openzt2_game_data::{
    world_scenario::{ScenarioDefinitionRecord, WorldScenarioDocument},
    AssetId,
};

use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::selected_world_document_handles::SelectedWorldDocumentHandles;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjective;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveAuthoredVisibility;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveDeadline;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveProgress;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;
use openzt2_zoo_state::scenario::scenario_objective_types::{
    ScenarioObjectiveOf, ScenarioObjectivePrerequisiteFor, ScenarioObjectiveRequires,
};
use openzt2_zoo_state::scenario::scenario_script_timing_types::ScenarioScriptTiming;
use openzt2_zoo_state::scenario::scenario_session_types::ActiveScenarioSession;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioDocument;
use openzt2_zoo_state::scenario::scenario_session_types::ScenarioSessionState;

use super::challenge_offer_types::ScenarioChallengeSelectionRandomNumberGenerator;

pub(super) fn instantiate_selected_scenario_and_authored_objectives(
    mut commands: Commands,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    scenarios: Res<WorldScenarios>,
    mut roots: Query<
        (
            Entity,
            &WorldRoot,
            &ZooClock,
            Option<&SelectedWorldDocumentHandles>,
            &mut PersistentIdAllocator,
        ),
        (With<WorldLoadCompleted>, Without<ActiveScenarioSession>),
    >,
) {
    for (root, world, clock, saved_handles, mut allocator) in &mut roots {
        let Some(view) = scenarios.get(&assets).and_then(|view| {
            saved_handles.map_or(Some(view), |handles| view.with_document(&handles.start))
        }) else {
            continue;
        };
        let Some(handle) = view.handle_for_scenario(world.scenario).cloned() else {
            continue;
        };
        let Some(scenario) = view.scenario(world.scenario) else {
            continue;
        };

        commands.entity(root).insert((
            ScenarioDocument(handle),
            ActiveScenarioSession {
                definition: scenario.id,
                started_tick: clock.tick,
                state: ScenarioSessionState::Running,
            },
            ScenarioScriptTiming::default(),
        ));
        spawn_authored_scenario_objectives(
            &mut commands,
            &mut allocator,
            root,
            root,
            scenario.id,
            scenario,
            clock.tick,
        );
    }
}

pub(super) fn initialize_scenario_challenge_random_number_generator(
    mut commands: Commands,
    roots: Query<
        (Entity, &ZooSeed),
        (
            With<WorldLoadCompleted>,
            Without<ScenarioChallengeSelectionRandomNumberGenerator>,
        ),
    >,
) {
    for (root, seed) in &roots {
        commands
            .entity(root)
            .insert(ScenarioChallengeSelectionRandomNumberGenerator::create_from_zoo_seed(*seed));
    }
}

fn spawn_authored_scenario_objectives(
    commands: &mut Commands,
    allocator: &mut PersistentIdAllocator,
    root: Entity,
    owner: Entity,
    scenario_id: AssetId,
    scenario: &ScenarioDefinitionRecord,
    started_tick: u64,
) {
    let mut spawned = Vec::with_capacity(scenario.objectives.len());
    for (index, record) in scenario.objectives.iter().enumerate() {
        let status = if record.prerequisite_objectives.is_empty() {
            ScenarioObjectiveStatus::Active
        } else {
            ScenarioObjectiveStatus::Inactive
        };
        let Ok(id) = allocator.allocate() else {
            return;
        };
        let objective = commands
            .spawn((
                WorldMember { root },
                id,
                ScenarioObjectiveOf(owner),
                ScenarioObjective {
                    scenario: scenario_id,
                    record_index: index as u32,
                },
                status,
                ScenarioObjectiveProgress {
                    current: 0,
                    target: 1,
                },
                ScenarioObjectiveAuthoredVisibility(!record.hidden),
            ))
            .id();
        if scenario.time_limit_ticks != 0 {
            commands
                .entity(objective)
                .insert(ScenarioObjectiveDeadline {
                    end_tick: started_tick.saturating_add(scenario.time_limit_ticks),
                });
        }
        spawned.push((index, objective));
    }
    for (index, entity) in &spawned {
        let prerequisites = scenario.objectives[*index]
            .prerequisite_objectives
            .iter()
            .filter_map(|id| {
                spawned.iter().find_map(|(candidate, entity)| {
                    (scenario.objectives[*candidate].id == *id).then_some(*entity)
                })
            });
        for prerequisite in prerequisites {
            commands.spawn((
                ScenarioObjectivePrerequisiteFor(*entity),
                ScenarioObjectiveRequires(prerequisite),
            ));
        }
    }
    super::scenario_group_completion::spawn_scenario_objective_groups(
        commands,
        owner,
        &scenario.objective_groups,
        Default::default(),
    );
}

pub(crate) fn spawn_authored_challenge_objectives(
    commands: &mut Commands,
    allocator: &mut PersistentIdAllocator,
    root: Entity,
    owner: Entity,
    scenario_id: AssetId,
    catalog: &WorldScenarioDocument,
    started_tick: u64,
) -> bool {
    let Some(scenario) = catalog.find_scenario_record(scenario_id) else {
        return false;
    };
    spawn_authored_scenario_objectives(
        commands,
        allocator,
        root,
        owner,
        scenario_id,
        scenario,
        started_tick,
    );
    true
}
