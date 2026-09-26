use bevy::prelude::*;

use crate::plugins::locomotion::locomotion_types::SpatialGrid;

use super::gate_operation_types::{Gate, GateAutoClose, GateMechanism, SetGateState};

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

/// Requests that gates stay open while an agent is inside their trigger radius.
pub(super) fn request_open_gates_for_agents_within_authored_trigger_distance(
    spatial_grid: Res<SpatialGrid>,
    agent_transforms: Query<&GlobalTransform>,
    mut gates: Query<(
        Entity,
        &Gate,
        &GateMechanism,
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

    for (gate_entity, gate, mechanism, transform, automatic_close) in &mut gates {
        if gate.locked || mechanism.trigger_distance_cm == 0 {
            continue;
        }
        let position = transform.translation();
        let center = Vec2::new(position.x, position.z);
        let radius = f32::from(mechanism.trigger_distance_cm) * 0.01;
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
                    agent_transforms.get(*agent).is_ok_and(|agent_transform| {
                        let agent_position = agent_transform.translation();
                        Vec2::new(agent_position.x, agent_position.z).distance_squared(center)
                            <= radius_squared
                    })
                })
            })
        });
        if !occupied {
            continue;
        }
        if gate.open {
            if let Some(mut automatic_close) = automatic_close {
                automatic_close.0 = mechanism.auto_close_ticks;
            }
        } else {
            requests.write(SetGateState {
                gate: gate_entity,
                open: true,
            });
        }
    }
}
