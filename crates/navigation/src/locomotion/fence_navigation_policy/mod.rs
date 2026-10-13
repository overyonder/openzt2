use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_zoo_state::topology::gate_actor_type_queries::GateActorTypeQueries;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_zoo_state::topology::topology_grid_geometry::horizontal_world_segments_intersect;

use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;

#[derive(SystemParam)]
pub(super) struct FenceNavigationPolicy<'w, 's> {
    pub(super) actor_types: GateActorTypeQueries<'w, 's>,
    fences: Query<'w, 's, (&'static FenceEdge, Option<&'static Gate>)>,
    gates: Query<'w, 's, (&'static FenceEdge, &'static Gate)>,
    nodes: Query<'w, 's, &'static TopologyNode>,
    grid: Res<'w, TopologyGrid>,
}

impl FenceNavigationPolicy<'_, '_> {
    pub(super) fn world_segment_blocks_actor(
        &self,
        from: Vec3,
        to: Vec3,
        capabilities: NavFlags,
        actor_types: &[AssetId],
    ) -> bool {
        self.fences.iter().any(|(edge, gate)| {
            let (Ok(a), Ok(b)) = (self.nodes.get(edge.a), self.nodes.get(edge.b)) else {
                return false;
            };
            horizontal_world_segments_intersect(
                from.xz(),
                to.xz(),
                self.grid.cell_translation(a.cell).xz(),
                self.grid.cell_translation(b.cell).xz(),
            ) && self.definition_blocks_actor(edge, gate, capabilities, actor_types, true)
        })
    }

    pub(super) fn fence_blocks_actor(
        &self,
        fence_entity: Entity,
        capabilities: NavFlags,
        actor_types: &[AssetId],
    ) -> bool {
        let Ok((edge, gate)) = self.fences.get(fence_entity) else {
            return false;
        };
        self.definition_blocks_actor(edge, gate, capabilities, actor_types, true)
    }

    /// Route planning can anticipate an eligible automatic opening. Actual
    /// movement still requires the gate's live open bit and collision exception.
    pub(super) fn gate_blocks_movement(
        &self,
        actor: Entity,
        capabilities: NavFlags,
        from: Vec3,
        to: Vec3,
    ) -> bool {
        self.gates.iter().any(|(edge, gate)| {
            let (Ok(a), Ok(b)) = (self.nodes.get(edge.a), self.nodes.get(edge.b)) else {
                return false;
            };
            horizontal_world_segments_intersect(
                from.xz(),
                to.xz(),
                self.grid.cell_translation(a.cell).xz(),
                self.grid.cell_translation(b.cell).xz(),
            ) && self.definition_blocks_actor(
                edge,
                Some(gate),
                capabilities,
                self.actor_types.actor_types(actor),
                false,
            )
        })
    }

    fn definition_blocks_actor(
        &self,
        edge: &FenceEdge,
        gate: Option<&Gate>,
        capabilities: NavFlags,
        actor_types: &[AssetId],
        anticipates_opening: bool,
    ) -> bool {
        let Some(definition) = self
            .actor_types
            .definitions()
            .and_then(|definitions| definitions.find_fence(edge.definition))
        else {
            return true;
        };
        if let Some(gate) = gate {
            let policy = &definition.gate_policy;
            let passes_open_gate = policy.open_passage_types.as_ref().is_none_or(|types| {
                types
                    .iter()
                    .any(|exception| actor_types.contains(&exception.entity_type))
            });
            let can_open_gate = anticipates_opening
                && !gate.locked
                && policy.trigger_distance_cm != 0
                && policy
                    .open_for_entity_type
                    .is_none_or(|required| actor_types.contains(&required));
            if passes_open_gate && (gate.open || can_open_gate) {
                return false;
            }
        }
        definition.blocks.raw_flag_bits() & capabilities.0 & NavFlags::ELIGIBILITY_BITS != 0
    }
}
