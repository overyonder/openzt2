//! `ZTBehMorph` on a task's target: a fence an animal bashes becomes its
//! broken variant, which opens the habitat it bounded.

use bevy::prelude::*;
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    behavior::behavior_asset_types::BehaviorDocumentAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};
use openzt2_world_state::{
    application_schedule::FixedGameSet, simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_zoo_state::{
    behavior_task_execution_types::{
        advance_behavior_task_to_next_action, find_current_behavior_task_action,
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    topology::{
        topology_edit_types::TopologyChanged,
        topology_graph_types::{FenceEdge, TopologyIndex, TopologyNode},
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

// ponytail: only a fence's `s_Broken` morph is applied; the shipped tasks
// morph nothing else, and any other target keeps its type.
fn morph_current_behavior_targets(
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    mut actors: Query<
        (&mut BehaviorTaskExecutionState, &GlobalTransform),
        Without<PendingBehaviorTaskFailure>,
    >,
    grid: Res<openzt2_zoo_state::topology::topology_graph_types::TopologyGrid>,
    mut fences: Query<&mut FenceEdge>,
    nodes: Query<&TopologyNode>,
    mut topology: ResMut<TopologyIndex>,
    mut changed: MessageWriter<TopologyChanged>,
) {
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    for (mut task, actor_transform) in &mut actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(&BehaviorAction::MorphTarget(attribute)) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        debug!(target = ?task.target(), ?attribute, "behavior target morph");
        if attribute == AssetId::from_key("s_broken") {
            if let Some(mut edge) = task.target().and_then(|target| fences.get_mut(target).ok()) {
                let broken = definitions
                    .find_fence(edge.definition)
                    .and_then(|fence| fence.broken);
                if let (Some(broken), Ok(first), Ok(second)) =
                    (broken, nodes.get(edge.a), nodes.get(edge.b))
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
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}
