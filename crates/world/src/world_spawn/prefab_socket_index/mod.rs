use bevy::{ecs::entity::EntityHashSet, platform::collections::HashMap, prelude::*};
use openzt2_game_data::AssetId;
use openzt2_world_state::animation_playback::traversal_animation_path_types::TraversalAnimationPath;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;

use super::{
    named_physical_presentation_projection::NamedPresentationNodeAttachment,
    physical_presentation_state::ObjectPhysicalPresentationControllers,
};

/// A model or gameplay entity with authored descendants that can be referenced by name.
#[derive(Component)]
pub(crate) struct PrefabSocketIndexRoot;

#[derive(Component, Default, PartialEq, Eq)]
pub struct PrefabSocketIndex {
    named: HashMap<AssetId, Vec<Entity>>,
    authored: HashMap<AssetId, Entity>,
    paths: HashMap<AssetId, Entity>,
}

impl PrefabSocketIndex {
    pub fn named(&self, key: AssetId) -> impl Iterator<Item = Entity> + '_ {
        self.named.get(&key).into_iter().flatten().copied()
    }

    pub fn authored(&self, key: AssetId) -> Option<Entity> {
        self.authored.get(&key).copied()
    }
    pub fn path(&self, key: AssetId) -> Option<Entity> {
        self.paths.get(&key).copied()
    }
}

pub(super) fn project_prefab_socket_indexes(
    mut commands: Commands,
    new_holders: Query<&NamedPresentationNodeAttachment, Changed<NamedPresentationNodeAttachment>>,
    new_controllers: Query<
        Entity,
        Or<(
            Added<ObjectPhysicalPresentationControllers>,
            Added<PrefabSocketIndexRoot>,
        )>,
    >,
    changed_nodes: Query<
        Entity,
        Or<(
            Changed<Children>,
            Changed<ChildOf>,
            Changed<Name>,
            Changed<PrefabAuthoredAttachmentIdentifier>,
            Added<TraversalAnimationPath>,
        )>,
    >,
    mut removed_children: RemovedComponents<Children>,
    mut removed_names: RemovedComponents<Name>,
    mut removed_paths: RemovedComponents<TraversalAnimationPath>,
    mut removed_attachments: RemovedComponents<PrefabAuthoredAttachmentIdentifier>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    sources: Query<(
        Option<&Name>,
        Option<&PrefabAuthoredAttachmentIdentifier>,
        Has<TraversalAnimationPath>,
    )>,
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
        .chain(removed_paths.read())
    {
        affected.extend(
            std::iter::once(node)
                .chain(parents.iter_ancestors::<ChildOf>(node))
                .filter(|&entity| indexes.contains(entity)),
        );
    }
    for owner in affected {
        let mut next = PrefabSocketIndex::default();
        for node in
            std::iter::once(owner).chain(children.iter_descendants_depth_first::<Children>(owner))
        {
            let Ok((name, authored, path)) = sources.get(node) else {
                continue;
            };
            let authored = authored.filter(|authored| authored.0 != AssetId::default());
            if let Some(authored) = authored {
                next.authored.entry(authored.0).or_insert(node);
                next.named.entry(authored.0).or_default().push(node);
            }
            if let Some(name) = name {
                let key = AssetId::from_key(&name.as_str().to_ascii_lowercase());
                if path {
                    next.paths.entry(key).or_insert(node);
                }
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
