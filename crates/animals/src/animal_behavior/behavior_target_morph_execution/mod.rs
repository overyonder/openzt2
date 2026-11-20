//! `ZTBehMorph` and `BFBehKill killUsingDestroyEntity` on a task's target: a
//! fence an animal bashes becomes its broken variant, which opens the habitat
//! it bounded, and a building gives way to its ruin.

use bevy::prelude::*;
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    application_schedule::FixedGameSet, simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorDestroyTarget, RunBehaviorMorphTarget, RunningBehaviorInstruction,
};
use openzt2_zoo_state::{
    behavior_task_execution_types::{BehaviorTaskExecutionState, PendingBehaviorTaskFailure},
    placement::{
        placed_object_types::PlacedObjectDefinitionReference,
        placement_transaction_types::DestroyPlacedObject,
    },
    topology::{
        topology_edit_types::TopologyChanged,
        topology_graph_types::{FenceEdge, FenceEnd, FenceStart, TopologyIndex, TopologyNode},
        topology_grid_geometry::calculate_topology_edge_cell_bounds,
    },
};

pub(super) fn register_target_morph_execution(application: &mut App) {
    application.add_systems(
        FixedUpdate,
        morph_current_behavior_targets
            .after(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
            .in_set(FixedGameSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
}

// Only a fence's `s_Broken` morph is applied; the shipped tasks
// morph nothing else, and any other target keeps its type.
fn morph_current_behavior_targets(
    mut commands: Commands,
    clock: Res<ZooClock>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &GlobalTransform,
            Option<&RunBehaviorMorphTarget>,
            Has<RunBehaviorDestroyTarget>,
        ),
        (
            Without<PendingBehaviorTaskFailure>,
            Or<(With<RunBehaviorMorphTarget>, With<RunBehaviorDestroyTarget>)>,
        ),
    >,
    grid: Res<openzt2_zoo_state::topology::topology_graph_types::TopologyGrid>,
    mut fences: Query<(&mut FenceEdge, &FenceStart, &FenceEnd)>,
    nodes: Query<&TopologyNode>,
    mut topology: ResMut<TopologyIndex>,
    mut changed: MessageWriter<TopologyChanged>,
    placed: Query<(), With<PlacedObjectDefinitionReference>>,
    mut destroyed: MessageWriter<DestroyPlacedObject>,
) {
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    for (actor, task, instruction, actor_transform, morph, destroy) in &actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        if destroy {
            if let Some(target) = task.target().filter(|target| placed.contains(*target)) {
                debug!(?target, "building destroyed by an animal");
                destroyed.write(DestroyPlacedObject { entity: target });
            }
        }
        let breaks = destroy || morph.is_some_and(|morph| morph.0 == AssetId::from_key("s_broken"));
        if breaks {
            if let Some((mut edge, start, end)) =
                task.target().and_then(|target| fences.get_mut(target).ok())
            {
                let broken = definitions
                    .find_fence(edge.definition)
                    .and_then(|fence| fence.broken);
                if let (Some(broken), Ok(first), Ok(second)) =
                    (broken, nodes.get(start.0), nodes.get(end.0))
                {
                    debug!(from = ?edge.definition, to = ?broken,
                        actor = %actor_transform.translation(),
                        first = %grid.cell_translation(first.cell),
                        second = %grid.cell_translation(second.cell),
                        "fence broken by an animal");
                    edge.definition = broken;
                    // A broken gate segment is no longer a gate.
                    if let Some(target) = task.target() {
                        commands.entity(target).remove::<(
                            openzt2_zoo_state::topology::gate_operation_types::Gate,
                            openzt2_zoo_state::topology::gate_operation_types::GateMechanism,
                            openzt2_zoo_state::topology::gate_operation_types::GateAutoClose,
                        )>();
                    }
                    // The segment's model follows the topology index.
                    topology.set_changed();
                    changed.write(TopologyChanged {
                        transaction: None,
                        bounds: calculate_topology_edge_cell_bounds(first.cell, second.cell),
                    });
                }
            }
        }
        instruction.complete(actor, clock.tick, &mut commands);
    }
}
