use bevy::prelude::*;
use openzt2_game_data::{
    world_scenario::{ScenarioDefinitionRecord, WorldScenarioDocument},
    AssetId,
};

use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::game_session_types::WorldSessionMode;
use crate::plugins::simulation_time::deterministic_random_stream::ZooSeed;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use crate::plugins::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    campaign_progress_types::ScenarioCampaignCompletionProgress,
    challenge_offer_types::ScenarioChallengeSelectionRandomNumberGenerator,
    scenario_objective_types::{
        ScenarioObjective, ScenarioObjectiveAuthoredVisibility, ScenarioObjectiveDeadline,
        ScenarioObjectivePrerequisiteEntities, ScenarioObjectiveProgress, ScenarioObjectiveStatus,
    },
    scenario_session_types::{
        ActiveScenarioSession, ScenarioSessionState, SelectedScenarioDocument,
    },
};

pub(super) fn instantiate_selected_scenario_and_authored_objectives(
    mut commands: Commands,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    scenarios: Res<WorldScenarios>,
    clock: Res<ZooClock>,
    active: Query<(), With<ActiveScenarioSession>>,
    mut allocator: ResMut<PersistentIdAllocator>,
    roots: Query<
        (Entity, &WorldRoot, &SelectedWorldIdentity),
        (With<WorldLoadCompleted>, Without<ActiveScenarioSession>),
    >,
    existing: Query<(), With<ScenarioObjective>>,
) {
    if !active.is_empty() || !existing.is_empty() {
        return;
    }
    for (root, world, selection) in &roots {
        let Some(view) = scenarios.get(&assets) else {
            continue;
        };
        let Some(handle) = view.handle_for_scenario(world.scenario).cloned() else {
            continue;
        };
        let Some(scenario) = view.scenario(world.scenario) else {
            continue;
        };

        commands.insert_resource(SelectedScenarioDocument(handle));
        commands.entity(root).insert(ActiveScenarioSession {
            definition: scenario.id,
            started_tick: clock.tick,
            state: ScenarioSessionState::Running,
        });
        if selection.mode == WorldSessionMode::Campaign {
            if let Some((campaign, mission)) = view.campaign_for_scenario(world.scenario) {
                commands.entity(root).insert(
                    ScenarioCampaignCompletionProgress::create_for_campaign_mission_count(
                        campaign.id,
                        mission as u32,
                        campaign.scenarios.len() as u32,
                    ),
                );
            }
        }
        spawn_authored_scenario_objectives(
            &mut commands,
            &mut allocator,
            root,
            scenario.id,
            scenario,
            clock.tick,
        );
    }
}

pub(super) fn initialize_scenario_challenge_random_number_generator(
    mut commands: Commands,
    seed: Res<ZooSeed>,
    roots: Query<
        Entity,
        (
            With<WorldLoadCompleted>,
            Without<ScenarioChallengeSelectionRandomNumberGenerator>,
        ),
    >,
) {
    for root in &roots {
        commands
            .entity(root)
            .insert(ScenarioChallengeSelectionRandomNumberGenerator::create_from_zoo_seed(*seed));
    }
}

fn spawn_authored_scenario_objectives(
    commands: &mut Commands,
    allocator: &mut PersistentIdAllocator,
    root: Entity,
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
        let Ok(id) = allocator.allocate(root) else {
            return;
        };
        let objective = commands
            .spawn((
                WorldMember { root },
                id,
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
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        commands
            .entity(*entity)
            .insert(ScenarioObjectivePrerequisiteEntities(prerequisites));
    }
}

pub(crate) fn spawn_authored_challenge_objectives(
    commands: &mut Commands,
    allocator: &mut PersistentIdAllocator,
    root: Entity,
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
        scenario_id,
        scenario,
        started_tick,
    );
    true
}
