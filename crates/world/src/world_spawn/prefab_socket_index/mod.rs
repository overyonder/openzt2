use bevy::{ecs::entity::EntityHashSet, platform::collections::HashMap, prelude::*};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;

use super::{
    named_physical_presentation_projection::PendingNamedPresentationNodeAttachment,
    physical_presentation_state::ObjectPhysicalPresentationControllers,
};

#[derive(Component, Default, PartialEq, Eq)]
pub(super) struct PrefabSocketIndex {
    named: HashMap<AssetId, Vec<Entity>>,
    authored: HashMap<AssetId, Entity>,
}

impl PrefabSocketIndex {
    pub(super) fn named(&self, key: AssetId) -> impl Iterator<Item = Entity> + '_ {
        self.named.get(&key).into_iter().flatten().copied()
    }

    pub(super) fn authored(&self, key: AssetId) -> Option<Entity> {
        self.authored.get(&key).copied()
    }
}

pub(super) fn project_prefab_socket_indexes(
    mut commands: Commands,
    new_holders: Query<
        &PendingNamedPresentationNodeAttachment,
        Changed<PendingNamedPresentationNodeAttachment>,
    >,
    new_controllers: Query<Entity, Added<ObjectPhysicalPresentationControllers>>,
    changed_nodes: Query<
        Entity,
        Or<(
            Changed<Children>,
            Changed<Name>,
            Changed<PrefabAuthoredAttachmentIdentifier>,
        )>,
    >,
    mut removed_children: RemovedComponents<Children>,
    mut removed_names: RemovedComponents<Name>,
    mut removed_attachments: RemovedComponents<PrefabAuthoredAttachmentIdentifier>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    sources: Query<(Option<&Name>, Option<&PrefabAuthoredAttachmentIdentifier>)>,
    mut indexes: Query<&mut PrefabSocketIndex>,
) {
    let mut affected = new_holders
        .iter()
        .map(|attachment| attachment.owner)
        .chain(new_controllers.iter())
        .collect::<EntityHashSet>();
    for node in changed_nodes
        .iter()
        .chain(removed_children.read())
        .chain(removed_names.read())
        .chain(removed_attachments.read())
    {
        affected.extend(
            std::iter::once(node)
                .chain(parents.iter_ancestors::<ChildOf>(node))
                .filter(|&entity| indexes.contains(entity)),
        );
    }
    for owner in affected {
        let mut next = PrefabSocketIndex::default();
        for node in children.iter_descendants_depth_first::<Children>(owner) {
            let Ok((name, authored)) = sources.get(node) else {
                continue;
            };
            if let Some(authored) = authored {
                next.authored.entry(authored.0).or_insert(node);
                next.named.entry(authored.0).or_default().push(node);
            }
            if let Some(name) = name {
                let key = AssetId::from_key(&name.as_str().to_ascii_lowercase());
                if authored.is_none_or(|authored| authored.0 != key) {
                    next.named.entry(key).or_default().push(node);
                }
            }
        }
        if let Ok(mut index) = indexes.get_mut(owner) {
            index.set_if_neq(next);
        } else {
            commands.entity(owner).try_insert(next);
        }
    }
}
