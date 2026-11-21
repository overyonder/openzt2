use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::ui_document::action::construction::UiConstructionConfirmation;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologyConfirmation {
    Construction(UiConstructionConfirmation),
    TransportDeletion,
}

#[derive(Component, Debug, Clone, Copy)]
pub enum ConfirmedTopologyAction {
    Delete,
    Commit,
    CancelPreview,
    Consumed,
}

#[derive(Component)]
#[relationship(relationship_target = PendingTopologyConfirmations)]
pub struct TopologyConfirmationOperand(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = TopologyConfirmationOperand, linked_spawn)]
pub struct PendingTopologyConfirmations(Vec<Entity>);

#[derive(EntityEvent)]
pub struct ResolveTopologyConfirmation {
    pub entity: Entity,
    pub accepted: bool,
}

#[derive(SystemParam)]
pub struct TopologyConfirmationOperations<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    parents: Query<'w, 's, &'static ChildOf>,
    operations: Query<'w, 's, &'static TopologyConfirmation>,
}

impl TopologyConfirmationOperations<'_, '_> {
    pub fn get(&self, node: Entity, wanted: TopologyConfirmation) -> Option<Entity> {
        std::iter::once(node)
            .chain(self.parents.iter_ancestors::<ChildOf>(node))
            .find_map(|owner| {
                self.children.get(owner).ok()?.iter().find(|&entity| {
                    self.operations
                        .get(entity)
                        .is_ok_and(|actual| *actual == wanted)
                })
            })
    }
}
