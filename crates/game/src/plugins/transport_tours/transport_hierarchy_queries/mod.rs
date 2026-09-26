use bevy::prelude::*;

pub(crate) fn entity_is_descendant_of_transport_owner(
    mut candidate_entity: Entity,
    transport_owner: Entity,
    entity_parents: &Query<&ChildOf>,
) -> bool {
    for _ in 0..64 {
        if candidate_entity == transport_owner {
            return true;
        }
        let Ok(parent) = entity_parents.get(candidate_entity) else {
            return false;
        };
        candidate_entity = parent.parent();
    }
    false
}
