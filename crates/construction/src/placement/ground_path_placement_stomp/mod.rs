use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_zoo_state::{
    economy::money_types::Money,
    topology::topology_edit_types::{TopologyEdit, TopologyEditPrepared, TopologySnapshot},
};

pub(super) use openzt2_zoo_state::placement::ground_path_placement_stomp::{
    GroundPathFootprintOverlaps, GroundPathPlacementAction, GroundPathPlacementStomp,
};

#[derive(Default)]
pub(super) struct GroundPathStompRemovals(BTreeMap<u64, TopologySnapshot>);

impl GroundPathStompRemovals {
    pub(super) fn collect(
        &mut self,
        overlaps: &GroundPathFootprintOverlaps<'_, '_, '_>,
        cells: &[IVec2],
    ) {
        self.0.extend(
            cells
                .iter()
                .flat_map(|cell| overlaps.actions_at(*cell))
                .filter_map(|action| match action {
                    GroundPathPlacementAction::Delete(_, snapshot) => {
                        Some((snapshot.id().0, snapshot))
                    }
                    GroundPathPlacementAction::Allow | GroundPathPlacementAction::Prevent => None,
                }),
        );
    }

    pub(super) fn prepare(
        self,
        transaction: Entity,
        commands: &mut Commands,
        prepared: &mut MessageWriter<TopologyEditPrepared>,
    ) {
        if !self.0.is_empty() {
            commands.entity(transaction).insert(TopologyEdit {
                created: Box::new([]),
                removed: self.0.into_values().collect(),
            });
            prepared.write(TopologyEditPrepared {
                transaction,
                cost: Money(0),
            });
        }
    }
}
