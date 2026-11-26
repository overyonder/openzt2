use bevy::prelude::*;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    behavior_task_execution_types::{
        running_actions::{
            behavior_action_actors, mark_behavior_task_for_failure_and_stop_navigation,
            BehaviorInstructionId, RunBehaviorExitTankGate, RunningBehaviorInstruction,
        },
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    locomotion::locomotion_types::{
        Arrived, NavAgent, NavigateTo, NavigationFailed, NavigationRequestGeneration,
    },
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
    topology::{
        gate_operation_types::SetGateState,
        topology_graph_types::{FenceSegment, TopologyGrid, TopologyNode},
    },
};

#[derive(Component)]
pub struct RegisteredTankGate {
    pub registration_order: u64,
    pub tank: Entity,
    pub physical_sides: [Option<Entity>; 2],
    pub platforms: [Option<Entity>; 2],
    pub door: Option<Entity>,
}

#[derive(Component)]
pub struct TankLinkedPhysicalObject(pub Entity);

#[derive(Component)]
pub(super) struct AuthoredTankGateExit {
    pub(super) gate: Entity,
    pub(super) tank: Entity,
    pub(super) entry: Vec3,
    pub(super) exit: Vec3,
    pub(super) request: u64,
    instruction: BehaviorInstructionId,
}

fn first_linked_gate(
    linked: Entity,
    gates: impl IntoIterator<Item = (Entity, [Option<Entity>; 2])>,
) -> Option<Entity> {
    gates
        .into_iter()
        .find_map(|(gate, sides)| sides.contains(&Some(linked)).then_some(gate))
}

fn portal_points(
    a: Vec3,
    b: Vec3,
    extent: f32,
    actor: Vec3,
    mut can_place: impl FnMut(Vec3) -> bool,
) -> [Vec3; 2] {
    let mut midpoint = (a + b) * 0.5;
    midpoint.y = b.y * 0.5;
    let heading = (a - b).xz().normalize_or_zero();
    let backoff = Vec3::new(-heading.y, 0.0, heading.x);
    let sides = [backoff, -backoff].map(|normal| {
        let point = midpoint - normal * extent;
        let clear = (1..=4)
            .map(|step| point - normal * step as f32)
            .find(|&probe| can_place(probe));
        (clear.unwrap_or(point), clear.is_some())
    });
    let [(first, first_clear), (second, second_clear)] = sides;
    let first_entrance = if first_clear != second_clear {
        first_clear
    } else {
        actor.distance_squared(first) < actor.distance_squared(second)
    };
    if first_entrance {
        [first, second]
    } else {
        [second, first]
    }
}

pub(super) fn start_authored_tank_gate_exit(
    activation: In<Option<Entity>>,
    roots: Query<(&ZooClock, &TopologyGrid)>,
    gates: Query<(Entity, &RegisteredTankGate, &WorldMember)>,
    fences: Query<FenceSegment>,
    nodes: Query<&TopologyNode>,
    poses: Query<&GlobalTransform>,
    surfaces: DrySurfaceEligibility,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    mut navigate: MessageWriter<NavigateTo>,
    mut door_commands: MessageWriter<SetGateState>,
    mut commands: Commands,
    mut actors: Query<
        (
            Entity,
            &WorldMember,
            &GlobalTransform,
            &NavAgent,
            Option<&TankLinkedPhysicalObject>,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
        ),
        (
            With<RunBehaviorExitTankGate>,
            Without<AuthoredTankGateExit>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
) {
    for (actor, world, pose, agent, linked, task, instruction) in
        behavior_action_actors(&mut actors, activation.0)
    {
        let Ok((clock, grid)) = roots.get(world.root) else {
            continue;
        };
        if !instruction.is_current(task)
            || (activation.0.is_none() && !instruction.can_update_after_activation(clock.tick))
            || task.next_action_tick > clock.tick
        {
            continue;
        }
        let selected = linked.and_then(|linked| {
            let mut ordered: Vec<_> = gates
                .iter()
                .filter(|(_, _, member)| member.root == world.root)
                .collect();
            ordered.sort_by_key(|(_, gate, _)| gate.registration_order);
            first_linked_gate(
                linked.0,
                ordered
                    .into_iter()
                    .map(|(id, gate, _)| (id, gate.physical_sides)),
            )
        });
        let Some(gate_entity) = selected else {
            instruction.complete(actor, clock.tick, &mut commands);
            continue;
        };
        let Ok((_, gate, _)) = gates.get(gate_entity) else {
            continue;
        };
        let points = fences.get(gate_entity).ok().and_then(|fence| {
            let a = grid.cell_translation(nodes.get(fence.a()).ok()?.cell);
            let b = grid.cell_translation(nodes.get(fence.b()).ok()?.cell);
            Some(portal_points(
                a,
                b,
                agent.radius_m,
                pose.translation(),
                |probe| surfaces.on_land_at_position(world.root, probe).is_some(),
            ))
        });
        let Some([mut entry, mut exit]) = points else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let (first, second) = if gate.physical_sides[0]
            .is_some_and(|side| linked.is_some_and(|linked| linked.0 == side))
        {
            (0, 1)
        } else {
            (1, 0)
        };
        let height = |index: usize| {
            gate.platforms[index]
                .and_then(|platform| poses.get(platform).ok())
                .or_else(|| poses.get(gate_entity).ok())
                .map(|pose| pose.translation().y)
        };
        if let Some(height) = height(first) {
            entry.y = height;
        }
        if let Some(height) = height(second) {
            exit.y = height;
        }
        if let Some(door) = gate.door {
            door_commands.write(SetGateState {
                gate: door,
                open: true,
            });
        }
        let Ok(mut generation) = navigation_generations.get_mut(actor) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let request = generation.advance();
        commands.entity(actor).insert(AuthoredTankGateExit {
            gate: gate_entity,
            tank: gate.tank,
            entry,
            exit,
            request,
            instruction: instruction.id(),
        });
        navigate.write(NavigateTo {
            entity: actor,
            request_id: request,
            destination: exit,
            arrival_radius_m: 0.1,
        });
    }
}

pub(super) fn settle_authored_tank_gate_exit(
    mut arrived: MessageReader<Arrived>,
    mut failed: MessageReader<NavigationFailed>,
    actors: Query<(
        Entity,
        &AuthoredTankGateExit,
        &WorldMember,
        Option<(&BehaviorTaskExecutionState, &RunningBehaviorInstruction)>,
    )>,
    clocks: Query<&ZooClock>,
    mut commands: Commands,
) {
    let outcomes: bevy::ecs::entity::EntityHashMap<(u64, bool)> = arrived
        .read()
        .map(|event| (event.entity, (event.request_id, false)))
        .chain(
            failed
                .read()
                .map(|event| (event.entity, (event.request_id, true))),
        )
        .collect();
    for (actor, exit, world, task) in &actors {
        let Some((task, instruction)) =
            task.filter(|(task, step)| exit.instruction == step.id() && step.is_current(task))
        else {
            commands.entity(actor).remove::<AuthoredTankGateExit>();
            continue;
        };
        if let Some((_, failed)) = outcomes
            .get(&actor)
            .copied()
            .filter(|(id, _)| *id == exit.request)
        {
            let Ok(clock) = clocks.get(world.root) else {
                continue;
            };
            if task.next_action_tick > clock.tick {
                continue;
            }
            commands.entity(actor).remove::<AuthoredTankGateExit>();
            if failed {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            } else {
                instruction.complete(actor, clock.tick, &mut commands);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_selection_checks_both_links_in_registration_order() {
        let other = Entity::from_bits(10);
        let linked = Entity::from_bits(11);
        let [a, b, c] = [
            Entity::from_bits(1),
            Entity::from_bits(2),
            Entity::from_bits(3),
        ];
        let gates = [
            (a, [Some(other), None]),
            (b, [None, Some(linked)]),
            (c, [Some(linked), None]),
        ];
        assert_eq!(first_linked_gate(linked, gates), Some(b));
        assert_eq!(first_linked_gate(linked, gates.into_iter().take(1)), None);
    }
}
