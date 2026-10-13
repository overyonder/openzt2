use bevy::prelude::*;

use openzt2_zoo_state::locomotion::locomotion_types::SpatialGrid;

use openzt2_zoo_state::topology::gate_actor_type_queries::GateActorTypeQueries;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::gate_operation_types::GateAutoClose;
use openzt2_zoo_state::topology::gate_operation_types::GateMechanism;
use openzt2_zoo_state::topology::gate_operation_types::SetGateState;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;

pub(super) fn advance_gate_automatic_close_countdowns(
    mut gates: Query<(Entity, &Gate, &mut GateAutoClose)>,
    mut requests: MessageWriter<SetGateState>,
) {
    for (gate_entity, gate, mut automatic_close) in &mut gates {
        if !gate.open || automatic_close.0 == 0 {
            continue;
        }
        automatic_close.0 -= 1;
        if automatic_close.0 == 0 {
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
    time: Res<Time<Fixed>>,
    spatial_grid: Res<SpatialGrid>,
    actor_types: GateActorTypeQueries,
    agent_transforms: Query<&GlobalTransform>,
    mut gates: Query<(
        Entity,
        &Gate,
        &mut GateMechanism,
        &FenceEdge,
        &GlobalTransform,
        Option<&mut GateAutoClose>,
    )>,
    mut requests: MessageWriter<SetGateState>,
) {
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
        return;
    };
    if spatial_grid.cell_size_m <= 0.0 {
        return;
    }

    let Some(definitions) = actor_types.definitions() else {
        return;
    };
    for (gate_entity, gate, mut mechanism, edge, transform, automatic_close) in &mut gates {
        let Some(definition) = definitions.find_fence(edge.definition) else {
            continue;
        };
        let policy = &definition.gate_policy;
        if gate.locked || policy.trigger_distance_cm == 0 {
            continue;
        }
        if policy.poll_interval_ns != 0 {
            let interval = std::time::Duration::from_nanos(policy.poll_interval_ns);
            mechanism.elapsed_poll_time = mechanism.elapsed_poll_time.saturating_add(time.delta());
            if mechanism.elapsed_poll_time < interval {
                continue;
            }
            // Fixed ticks can span several polls, but one observation of live
            // positions is sufficient. Retain the sub-interval remainder.
            mechanism.elapsed_poll_time = std::time::Duration::from_nanos(
                u64::try_from(
                    mechanism.elapsed_poll_time.as_nanos() % u128::from(policy.poll_interval_ns),
                )
                .expect("poll remainder fits authored interval"),
            );
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
                spatial_grid.range(cell).iter().any(|agent| {
                    if policy.open_for_entity_type.is_some_and(|required_type| {
                        !actor_types.actor_types(*agent).contains(&required_type)
                    }) {
                        return false;
                    }
                    agent_transforms.get(*agent).is_ok_and(|agent_transform| {
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
            if let Some(mut automatic_close) = automatic_close {
                automatic_close.0 = policy.auto_close_ticks;
            }
        } else {
            requests.write(SetGateState {
                gate: gate_entity,
                open: true,
            });
        }
    }
}
