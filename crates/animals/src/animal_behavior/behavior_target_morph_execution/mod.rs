//! Authored replacement and destruction of live topology segments and placed objects.

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::{
        WorldDefinitions, WorldDefinitionsView,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{behavior::action_record::BehaviorMorphRecipient, AssetId};
use openzt2_world::world_spawn::prefab_world_instance_spawning::morph_fade_projection::PrefabMorphFade;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;
use openzt2_zoo_state::{
    behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation,
    behavior_task_execution_types::{
        running_actions::{
            BehaviorInstructionId, BehaviorStepFixedSet, RunBehaviorMorphEntity,
            RunningBehaviorInstruction,
        },
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    topology::{
        gate_operation_types::{Gate, GateAutoClose, GateMechanism},
        topology_edit_types::TopologyChanged,
        topology_graph_types::{
            FenceEdge, FenceEnd, FenceStart, TopologyGrid, TopologyIndex, TopologyNode,
        },
        topology_grid_geometry::calculate_topology_edge_cell_bounds,
    },
};

#[derive(Component)]
struct PendingFenceMorph {
    instruction: BehaviorInstructionId,
    target: Entity,
    fades: bool,
}

pub(super) fn register_target_morph_execution(app: &mut App) {
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
        activated_behavior_actor, update_waiting_behavior_actors,
    };
    app.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorMorphEntity>.pipe(execute_fence_morph));

    app.add_observer(discard_pending_fence_morph).add_systems(
        FixedUpdate,
        update_waiting_behavior_actors
            .pipe(execute_fence_morph)
            .in_set(BehaviorStepFixedSet::Think),
    );
}

fn discard_pending_fence_morph(
    removed: On<Remove<RunBehaviorMorphEntity>>,
    pending: Query<&PendingFenceMorph>,
    mut commands: Commands,
) {
    if let Ok(pending) = pending.get(removed.entity) {
        if pending.fades {
            commands
                .entity(pending.target)
                .try_remove::<PrefabMorphFade>();
        }
    }
    commands
        .entity(removed.entity)
        .try_remove::<PendingFenceMorph>();
}

pub(super) fn resolve_fence_morph(
    definitions: WorldDefinitionsView<'_>,
    fence: AssetId,
    selector: AssetId,
) -> Option<AssetId> {
    let definition = definitions.find_fence(fence)?;
    let attribute = definitions
        .find_object(definition.object)
        .and_then(|object| {
            object
                .authored_runtime
                .morph_targets
                .iter()
                .find_map(|(key, value)| (*key == selector).then_some(*value))
        });
    let replacement = attribute.or_else(|| {
        if selector == AssetId::from_key("s_broken") {
            definition.broken
        } else {
            Some(selector)
        }
    })?;
    (replacement != fence && definitions.find_fence(replacement).is_some()).then_some(replacement)
}

#[derive(SystemParam)]
pub(super) struct FenceTopology<'w, 's> {
    pub(super) fences: Query<
        'w,
        's,
        (
            &'static mut FenceEdge,
            &'static FenceStart,
            &'static FenceEnd,
        ),
    >,
    nodes: Query<'w, 's, &'static TopologyNode>,
    index: Single<'w, 's, &'static TopologyIndex>,
    gates: Query<'w, 's, &'static mut Gate>,
    changed: MessageWriter<'w, TopologyChanged>,
}

impl FenceTopology<'_, '_> {
    pub(super) fn edges_in_box(&self, grid: TopologyGrid, min: Vec3, max: Vec3) -> Vec<Entity> {
        let first = grid.position_cell(min).truncate();
        let last = grid.position_cell(max).truncate();
        let mut seen = bevy::platform::collections::HashSet::<Entity>::default();
        self.index
            .fences_within(first, last)
            .filter(|entity| seen.insert(*entity))
            .filter(|entity| {
                let Ok((_, start, end)) = self.fences.get(*entity) else {
                    return false;
                };
                let (Ok(first), Ok(second)) = (self.nodes.get(start.0), self.nodes.get(end.0))
                else {
                    return false;
                };
                let a = grid.cell_translation(first.cell);
                let b = grid.cell_translation(second.cell);
                a.min(b).cmple(max).all() && a.max(b).cmpge(min).all()
            })
            .collect()
    }

    pub(super) fn replace(
        &mut self,
        target: Entity,
        replacement: AssetId,
        replacement_is_gate: bool,
        commands: &mut Commands,
    ) -> bool {
        let Ok((mut edge, start, end)) = self.fences.get_mut(target) else {
            return false;
        };
        let (Ok(first), Ok(second)) = (self.nodes.get(start.0), self.nodes.get(end.0)) else {
            return false;
        };
        edge.definition = replacement;
        if replacement_is_gate {
            let closed = Gate {
                open: false,
                locked: false,
            };
            if let Ok(mut gate) = self.gates.get_mut(target) {
                gate.set_if_neq(closed);
            } else {
                commands
                    .entity(target)
                    .insert((closed, GateMechanism::default()));
            }
            commands.entity(target).try_remove::<GateAutoClose>();
        } else {
            commands
                .entity(target)
                .try_remove::<(Gate, GateMechanism, GateAutoClose)>();
        }
        self.changed.write(TopologyChanged {
            transaction: None,
            bounds: calculate_topology_edge_cell_bounds(first.cell, second.cell),
        });
        true
    }
}

fn execute_fence_morph(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorMorphEntity,
            Option<&PendingFenceMorph>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut topology: FenceTopology,
) {
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    for (actor, mut task, instruction, morph, pending) in
        behavior_action_actors(&mut actors, activation.0)
    {
        if activation.0.is_none() && !instruction.can_update_after_activation(clock.tick) {
            continue;
        }
        if task.next_action_tick > clock.tick || !instruction.is_current(&task) {
            continue;
        }
        let target = match morph.recipient {
            BehaviorMorphRecipient::Target => task.target(),
            BehaviorMorphRecipient::NamedChild(id) if id == AssetId::from_key("object") => {
                task.object
            }
            _ => continue,
        };
        let Some((target, fence)) = target.and_then(|target| {
            topology
                .fences
                .get(target)
                .ok()
                .map(|(edge, ..)| (target, edge.definition))
        }) else {
            continue;
        };
        let selector = morph.definition;
        let Some(replacement) = resolve_fence_morph(definitions, fence, selector) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if !morph.period.is_zero() {
            if pending.is_none_or(|pending| pending.instruction != instruction.id()) {
                let delay = morph
                    .period
                    .as_nanos()
                    .saturating_mul(u128::from(definitions.timing().fixed_hz))
                    .div_ceil(1_000_000_000);
                let due = clock
                    .tick
                    .saturating_add(u64::try_from(delay).unwrap_or(u64::MAX));
                commands.entity(actor).insert(PendingFenceMorph {
                    instruction: instruction.id(),
                    target,
                    fades: morph.fade,
                });
                if morph.fade {
                    commands
                        .entity(target)
                        .insert(PrefabMorphFade::fade_out(morph.period));
                }
                task.next_action_tick = due;
                continue;
            }
        }
        let is_gate = definitions
            .find_fence(replacement)
            .and_then(|fence| definitions.find_object(fence.object))
            .is_some_and(|object| {
                matches!(
                    object.kind,
                    openzt2_game_data::world_definitions::world_objects::WorldObjectKind::Gate
                )
            });
        if topology.replace(target, replacement, is_gate, &mut commands) {
            if morph.fade && !morph.period.is_zero() {
                commands
                    .entity(target)
                    .insert(PrefabMorphFade::fade_in(morph.period));
            }
            commands.entity(actor).try_remove::<PendingFenceMorph>();
            instruction.complete(actor, clock.tick, &mut commands);
        } else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    }
}
