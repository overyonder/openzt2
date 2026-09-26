use bevy::prelude::*;

use crate::plugins::animation_graph::animation_graph_playback_message_types::AnimationGraphNodePlaybackRequest;

use super::{
    gate_operation_types::{Gate, GateAutoClose, GateMechanism, SetGateState},
    topology_edit_types::TopologyChanged,
    topology_graph_types::{FenceEdge, TopologyNode},
    topology_grid_geometry::calculate_topology_edge_cell_bounds,
};

pub(super) fn apply_requested_gate_state_and_animation(
    mut requests: MessageReader<SetGateState>,
    mut gates: Query<(
        &mut Gate,
        &FenceEdge,
        Option<&GateMechanism>,
        Option<&mut GateAutoClose>,
    )>,
    nodes: Query<&TopologyNode>,
    mut changed: MessageWriter<TopologyChanged>,
    mut animations: MessageWriter<AnimationGraphNodePlaybackRequest>,
) {
    for request in requests.read() {
        let Ok((mut gate, edge, mechanism, automatic_close)) = gates.get_mut(request.gate) else {
            continue;
        };
        if gate.locked || gate.open == request.open {
            continue;
        }
        let (Ok(first_node), Ok(second_node)) = (nodes.get(edge.a), nodes.get(edge.b)) else {
            continue;
        };
        gate.open = request.open;
        if let Some(mechanism) = mechanism {
            let animation_node = if request.open {
                mechanism.open_animation
            } else {
                mechanism.close_animation
            };
            if animation_node != openzt2_game_data::AssetId::default() {
                animations.write(AnimationGraphNodePlaybackRequest::new(
                    request.gate,
                    animation_node.to_lowercase_hexadecimal_string(),
                ));
            }
            if let Some(mut automatic_close) = automatic_close {
                automatic_close.0 = request
                    .open
                    .then_some(mechanism.auto_close_ticks)
                    .unwrap_or(0);
            }
        }
        changed.write(TopologyChanged {
            transaction: None,
            bounds: calculate_topology_edge_cell_bounds(first_node.cell, second_node.cell),
        });
    }
}
