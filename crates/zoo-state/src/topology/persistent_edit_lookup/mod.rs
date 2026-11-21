use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId, world_membership_types::WorldMember,
};

/// Resolves saved edit operands in their own world, including after recreation.
#[derive(Resource, Default)]
pub struct PersistentEditLookup(HashMap<(Entity, u64), Entity>);

impl PersistentEditLookup {
    pub fn get(&self, world: Entity, id: PersistentId) -> Option<Entity> {
        self.0.get(&(world, id.0)).copied()
    }
}

pub struct PersistentEditLookupPlugin;

impl Plugin for PersistentEditLookupPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PersistentEditLookup>()
            .add_observer(insert_id)
            .add_observer(insert_member)
            .add_observer(discard_id)
            .add_observer(discard_member);
    }
}

fn insert_id(
    inserted: On<Insert<PersistentId>>,
    entities: Query<(&PersistentId, &WorldMember)>,
    mut lookup: ResMut<PersistentEditLookup>,
) {
    index(inserted.entity, &entities, &mut lookup);
}

fn insert_member(
    inserted: On<Insert<WorldMember>>,
    entities: Query<(&PersistentId, &WorldMember)>,
    mut lookup: ResMut<PersistentEditLookup>,
) {
    index(inserted.entity, &entities, &mut lookup);
}

fn index(
    entity: Entity,
    entities: &Query<(&PersistentId, &WorldMember)>,
    lookup: &mut PersistentEditLookup,
) {
    if let Ok((id, member)) = entities.get(entity) {
        lookup.0.insert((member.root, id.0), entity);
    }
}

fn discard_id(
    discarded: On<Discard<PersistentId>>,
    entities: Query<(&PersistentId, &WorldMember)>,
    mut lookup: ResMut<PersistentEditLookup>,
) {
    discard(discarded.entity, &entities, &mut lookup);
}

fn discard_member(
    discarded: On<Discard<WorldMember>>,
    entities: Query<(&PersistentId, &WorldMember)>,
    mut lookup: ResMut<PersistentEditLookup>,
) {
    discard(discarded.entity, &entities, &mut lookup);
}

fn discard(
    entity: Entity,
    entities: &Query<(&PersistentId, &WorldMember)>,
    lookup: &mut PersistentEditLookup,
) {
    if let Ok((id, member)) = entities.get(entity) {
        let key = (member.root, id.0);
        if lookup.0.get(&key) == Some(&entity) {
            lookup.0.remove(&key);
        }
    }
}
