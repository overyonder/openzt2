use bevy::{
    ecs::{entity::EntityHashSet, system::SystemParam},
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    application_schedule::FixedGameSet,
    world_spawn::{
        prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier,
        prefab_presentation_types::PrefabPresentation,
        world_membership_types::{DefinitionId, WorldMember},
    },
};

use openzt2_world_state::animation_playback::traversal_animation_path_types::TraversalAnimationPath;

#[derive(Component, Debug, Default, PartialEq, Eq)]
struct BehaviorNodeIndex {
    named: HashMap<AssetId, Entity>,
    sockets: HashMap<AssetId, Entity>,
    paths: HashMap<AssetId, Entity>,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct ProjectBehaviorNamedNodes;

type PresentationOwners = (
    With<WorldMember>,
    Or<(
        With<DefinitionId>,
        With<PrefabPresentation>,
        With<BehaviorNodeIndex>,
    )>,
);
type ChangedNodeInputs = Or<(
    Changed<Children>,
    Changed<Name>,
    Changed<PrefabAuthoredAttachmentIdentifier>,
    Added<GlobalTransform>,
    Added<TraversalAnimationPath>,
)>;
type AuthoredNode = (
    Option<&'static Name>,
    Option<&'static PrefabAuthoredAttachmentIdentifier>,
    Has<TraversalAnimationPath>,
    Has<GlobalTransform>,
);

pub(super) fn register_behavior_named_nodes(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        project_behavior_named_nodes
            .in_set(FixedGameSet::Clock)
            .run_if(in_state(GamePhase::InGame)),
    )
    .add_systems(
        PostUpdate,
        project_behavior_named_nodes
            .in_set(ProjectBehaviorNamedNodes)
            .after(bevy::transform::TransformSystems::Propagate)
            .run_if(in_state(GamePhase::InGame)),
    );
}

#[derive(SystemParam)]
struct RemovedNodeInputs<'w, 's> {
    name: RemovedComponents<'w, 's, Name>,
    socket: RemovedComponents<'w, 's, PrefabAuthoredAttachmentIdentifier>,
    path: RemovedComponents<'w, 's, TraversalAnimationPath>,
    children: RemovedComponents<'w, 's, Children>,
    transform: RemovedComponents<'w, 's, GlobalTransform>,
}

#[allow(clippy::too_many_arguments)]
fn project_behavior_named_nodes(
    changed: Query<Entity, ChangedNodeInputs>,
    owners: Query<(), PresentationOwners>,
    roots: Query<
        Entity,
        (
            PresentationOwners,
            With<Children>,
            Without<BehaviorNodeIndex>,
        ),
    >,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    nodes: Query<AuthoredNode, Or<(With<Name>, With<PrefabAuthoredAttachmentIdentifier>)>>,
    mut indexes: Query<&mut BehaviorNodeIndex>,
    mut removed: RemovedNodeInputs,
    mut commands: Commands,
) {
    let mut dirty = roots.iter().collect::<EntityHashSet>();
    for entity in changed
        .iter()
        .chain(removed.name.read())
        .chain(removed.socket.read())
        .chain(removed.path.read())
        .chain(removed.children.read())
        .chain(removed.transform.read())
    {
        dirty.extend(
            std::iter::once(entity)
                .chain(parents.iter_ancestors::<ChildOf>(entity))
                .filter(|&ancestor| owners.contains(ancestor)),
        );
    }
    let mut dirty = dirty.into_iter().collect::<Vec<_>>();
    dirty.sort_unstable();
    for root in dirty {
        let mut index = BehaviorNodeIndex::default();
        for child in children.iter_descendants_depth_first::<Children>(root) {
            let Ok((name, identifier, path, has_pose)) = nodes.get(child) else {
                continue;
            };
            if let Some(identifier) = identifier {
                index.sockets.entry(identifier.0).or_insert(child);
            }
            if !has_pose {
                continue;
            }
            if let Some(identifier) = identifier {
                index.named.entry(identifier.0).or_insert(child);
            }
            if let Some(name) = name {
                let key = name_key(name.as_str());
                index.named.entry(key).or_insert(child);
                if path {
                    index.paths.entry(key).or_insert(child);
                }
            }
        }
        if let Ok(mut current) = indexes.get_mut(root) {
            current.set_if_neq(index);
        } else {
            commands.entity(root).try_insert(index);
        }
    }
}

fn name_key(name: &str) -> AssetId {
    AssetId::from_key(&name.to_ascii_lowercase())
}

#[derive(SystemParam)]
pub(super) struct NamedNodeTransforms<'w, 's> {
    indexes: Query<'w, 's, &'static BehaviorNodeIndex>,
    transforms: Query<'w, 's, &'static GlobalTransform>,
}

impl NamedNodeTransforms<'_, '_> {
    pub(super) fn pose(&self, root: Entity, name: &str) -> Option<&GlobalTransform> {
        self.entity_and_pose(root, name)
            .map(|(_, transform)| transform)
    }

    pub(super) fn entity_and_pose(
        &self,
        root: Entity,
        name: &str,
    ) -> Option<(Entity, &GlobalTransform)> {
        let index = self.indexes.get(root).ok()?;
        let entity = *index.named.get(&name_key(name.trim()))?;
        Some((entity, self.transforms.get(entity).ok()?))
    }

    pub(super) fn socket(&self, root: Entity, name: AssetId) -> Option<Entity> {
        self.indexes.get(root).ok()?.sockets.get(&name).copied()
    }

    pub(super) fn path(&self, root: Entity, name: &str) -> Option<Entity> {
        let index = self.indexes.get(root).ok()?;
        let entity = *index.paths.get(&name_key(name))?;
        self.transforms.contains(entity).then_some(entity)
    }
}
