use std::time::Duration;

use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_zoo_state::locomotion::locomotion_types::{SpatialGrid, SpatialGridMembership};

use openzt2_zoo_state::topology::gate_actor_type_queries::GateActorTypeQueries;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::gate_operation_types::GateAutoClose;
use openzt2_zoo_state::topology::gate_operation_types::GateMechanism;
use openzt2_zoo_state::topology::gate_operation_types::SetGateState;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;

pub(super) fn arm_gate_automatic_close_timer(
    commands: &mut Commands,
    gate: Entity,
    timer: Option<Mut<GateAutoClose>>,
    ticks: u32,
    time: &Time<Fixed>,
) {
    if ticks == 0 {
        if timer.is_some() {
            commands.entity(gate).remove::<GateAutoClose>();
        }
        return;
    }
    let duration = time.timestep().saturating_mul(ticks);
    if let Some(mut timer) = timer {
        timer.0.set_duration(duration);
        timer.0.reset();
    } else {
        commands
            .entity(gate)
            .insert(GateAutoClose(Timer::new(duration, TimerMode::Once)));
    }
}

pub(super) fn tick_armed_gate_automatic_close_timers(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut gates: Query<(Entity, &Gate, &mut GateAutoClose)>,
    mut requests: MessageWriter<SetGateState>,
) {
    for (gate_entity, gate, mut timer) in &mut gates {
        if gate.open && timer.0.tick(time.delta()).just_finished() {
            commands.entity(gate_entity).remove::<GateAutoClose>();
            requests.write(SetGateState {
                gate: gate_entity,
                open: false,
            });
        }
    }
}

/// Polls eligible authored actor types, opening at the inner radius and closing
/// only after those actors have left the outer radius.
pub(super) fn request_open_gates_for_agents_within_authored_trigger_distance(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    grids: Query<&SpatialGrid>,
    spatial_membership: SpatialGridMembership,
    actor_types: GateActorTypeQueries,
    agent_transforms: Query<&GlobalTransform>,
    mut gates: Query<(
        Entity,
        &Gate,
        &WorldMember,
        &mut GateMechanism,
        &FenceEdge,
        &GlobalTransform,
        Option<&mut GateAutoClose>,
    )>,
    mut requests: MessageWriter<SetGateState>,
) {
    let Some(definitions) = actor_types.definitions() else {
        return;
    };
    for (gate_entity, gate, member, mut mechanism, edge, transform, automatic_close) in &mut gates {
        let Ok(spatial_grid) = grids.get(member.root) else {
            continue;
        };
        let Some(maximum_cell) = spatial_grid
            .width
            .checked_sub(1)
            .and_then(|x| i32::try_from(x).ok())
            .zip(
                spatial_grid
                    .height
                    .checked_sub(1)
                    .and_then(|y| i32::try_from(y).ok()),
            )
            .map(|(x, y)| IVec2::new(x, y))
        else {
            continue;
        };
        if spatial_grid.cell_size_m <= 0.0 {
            continue;
        }

        let Some(definition) = definitions.find_gate_fence(edge.definition) else {
            continue;
        };
        let policy = &definition.gate_policy;
        if gate.locked || policy.trigger_distance_cm == 0 {
            continue;
        }
        if policy.poll_interval_ns != 0 {
            let interval = Duration::from_nanos(policy.poll_interval_ns);
            if mechanism.0.duration() != interval {
                mechanism.0.set_duration(interval);
            }
            if !mechanism.0.tick(time.delta()).just_finished() {
                continue;
            }
        }
        let position = transform.translation();
        let center = Vec2::new(position.x, position.z);
        let radius = f32::from(if gate.open && policy.close_distance_cm != 0 {
            policy.close_distance_cm
        } else {
            policy.trigger_distance_cm
        }) * 0.01;
        let radius_squared = radius * radius;
        let minimum = ((center - Vec2::splat(radius) - spatial_grid.origin)
            / spatial_grid.cell_size_m)
            .floor()
            .as_ivec2()
            .max(IVec2::ZERO);
        let maximum = ((center + Vec2::splat(radius) - spatial_grid.origin)
            / spatial_grid.cell_size_m)
            .floor()
            .as_ivec2()
            .min(maximum_cell);
        if minimum.x > maximum.x || minimum.y > maximum.y {
            continue;
        }

        let occupied = (minimum.y..=maximum.y).any(|y| {
            (minimum.x..=maximum.x).any(|x| {
                let cell = u32::try_from(y).expect("clamped grid row") * spatial_grid.width
                    + u32::try_from(x).expect("clamped grid column");
                spatial_membership.members(member.root, cell).any(|agent| {
                    if policy.open_for_entity_type.is_some_and(|required_type| {
                        !actor_types.actor_types(agent).contains(&required_type)
                    }) {
                        return false;
                    }
                    agent_transforms.get(agent).is_ok_and(|agent_transform| {
                        let agent_position = agent_transform.translation();
                        Vec2::new(agent_position.x, agent_position.z).distance_squared(center)
                            <= radius_squared
                    })
                })
            })
        });
        if !occupied {
            if gate.open && policy.close_distance_cm != 0 {
                requests.write(SetGateState {
                    gate: gate_entity,
                    open: false,
                });
            }
            continue;
        }
        if gate.open {
            arm_gate_automatic_close_timer(
                &mut commands,
                gate_entity,
                automatic_close,
                policy.auto_close_ticks,
                &time,
            );
        } else {
            requests.write(SetGateState {
                gate: gate_entity,
                open: true,
            });
        }
    }
}
