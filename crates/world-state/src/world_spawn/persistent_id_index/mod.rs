use crate::world_spawn::{persistent_id_types::PersistentId, world_membership_types::WorldMember};
use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    platform::collections::HashMap,
    prelude::*,
};

/// Resolves persistent IDs to their live entities within a zoo.
#[derive(Resource, Default)]
pub struct PersistentIdIndex(HashMap<(Entity, u64), Entity>);

impl PersistentIdIndex {
    pub fn get(&self, world: Entity, id: PersistentId) -> Option<Entity> {
        self.0.get(&(world, id.0)).copied()
    }
}

pub struct PersistentIdIndexPlugin;

impl Plugin for PersistentIdIndexPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PersistentIdIndex>()
            .add_observer(insert_id)
            .add_observer(insert_member)
            .add_observer(discard_id)
            .add_observer(discard_member);
    }
}

fn insert_id(
    inserted: On<Insert<PersistentId>>,
    entities: Query<(&PersistentId, &WorldMember), Allow<Disabled>>,
    mut lookup: ResMut<PersistentIdIndex>,
) {
    index(inserted.entity, &entities, &mut lookup);
}

fn insert_member(
    inserted: On<Insert<WorldMember>>,
    entities: Query<(&PersistentId, &WorldMember), Allow<Disabled>>,
    mut lookup: ResMut<PersistentIdIndex>,
) {
    index(inserted.entity, &entities, &mut lookup);
}

fn index(
    entity: Entity,
    entities: &Query<(&PersistentId, &WorldMember), Allow<Disabled>>,
    lookup: &mut PersistentIdIndex,
) {
    if let Ok((id, member)) = entities.get(entity) {
        lookup.0.insert((member.root, id.0), entity);
    }
}

fn discard_id(
    discarded: On<Discard<PersistentId>>,
    entities: Query<(&PersistentId, &WorldMember), Allow<Disabled>>,
    mut lookup: ResMut<PersistentIdIndex>,
) {
    discard(discarded.entity, &entities, &mut lookup);
}

fn discard_member(
    discarded: On<Discard<WorldMember>>,
    entities: Query<(&PersistentId, &WorldMember), Allow<Disabled>>,
    mut lookup: ResMut<PersistentIdIndex>,
) {
    discard(discarded.entity, &entities, &mut lookup);
}

fn discard(
    entity: Entity,
    entities: &Query<(&PersistentId, &WorldMember), Allow<Disabled>>,
    lookup: &mut PersistentIdIndex,
) {
    if let Ok((id, member)) = entities.get(entity) {
        let key = (member.root, id.0);
        if lookup.0.get(&key) == Some(&entity) {
            lookup.0.remove(&key);
        }
    }
}
