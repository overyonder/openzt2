use std::collections::HashMap;

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::AssetId;
use openzt2_interface_state::ui::authored_ui_node_projection_components::{
    UiDocumentMembers, UiDocumentOwner, UiNodeId,
};

#[derive(Component, Default)]
pub(super) struct ProjectedUiTargets(HashMap<AssetId, Vec<Entity>>);

impl ProjectedUiTargets {
    pub(super) fn from_authored_node_entities(
        nodes: impl IntoIterator<Item = (AssetId, Entity)>,
    ) -> Self {
        let mut targets = HashMap::<AssetId, Vec<Entity>>::new();
        for (id, entity) in nodes {
            targets.entry(id).or_default().push(entity);
        }
        Self(targets)
    }
}

#[derive(SystemParam)]
pub(super) struct AuthoredUiTargetLookup<'w, 's> {
    indexes: Query<'w, 's, &'static ProjectedUiTargets>,
    members: Query<'w, 's, &'static UiDocumentMembers>,
    nodes: Query<'w, 's, (&'static UiNodeId, &'static UiDocumentOwner)>,
}

impl AuthoredUiTargetLookup<'_, '_> {
    pub(super) fn candidates(
        &self,
        document: Entity,
        target: AssetId,
    ) -> impl Iterator<Item = Entity> + '_ {
        let matches = move |entity| {
            self.nodes
                .get(entity)
                .is_ok_and(|(id, owner)| id.id == target && owner.0 == document)
        };
        let authored = self
            .indexes
            .get(document)
            .ok()
            .and_then(|index| index.0.get(&target));
        let dynamic = self.members.get(document).ok();
        authored
            .into_iter()
            .flat_map(|nodes| nodes.iter().copied())
            .chain(
                dynamic
                    .into_iter()
                    .flat_map(|members| members.iter())
                    .filter(move |entity| authored.is_none_or(|nodes| !nodes.contains(entity))),
            )
            .filter(move |entity| matches(*entity))
    }
}
