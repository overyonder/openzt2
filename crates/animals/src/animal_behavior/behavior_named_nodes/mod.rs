use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::AssetId;
use openzt2_world::world_spawn::prefab_socket_index::PrefabSocketIndex;

fn name_key(name: &str) -> AssetId {
    AssetId::from_key(&name.trim().to_ascii_lowercase())
}

#[derive(SystemParam)]
pub(super) struct NamedNodeTransforms<'w, 's> {
    indexes: Query<'w, 's, &'static PrefabSocketIndex>,
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
        self.indexes
            .get(root)
            .ok()?
            .named(name_key(name))
            .find_map(|entity| Some((entity, self.transforms.get(entity).ok()?)))
    }

    pub(super) fn socket(&self, root: Entity, name: AssetId) -> Option<Entity> {
        self.indexes.get(root).ok()?.authored(name)
    }

    pub(super) fn path(&self, root: Entity, name: &str) -> Option<Entity> {
        let entity = self.indexes.get(root).ok()?.path(name_key(name))?;
        self.transforms.contains(entity).then_some(entity)
    }
}
