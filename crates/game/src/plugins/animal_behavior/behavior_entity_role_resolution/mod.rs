//! Resolution of authored behavior entity roles to live Bevy entities.

use bevy::prelude::Entity;
use openzt2_game_data::behavior::action::entity_role::BehaviorEntityRole;

pub(crate) const fn resolve_behavior_entity_role_to_live_entity(
    entity_role: BehaviorEntityRole,
    subject_entity: Entity,
    target_entity: Option<Entity>,
) -> Option<Entity> {
    match entity_role {
        BehaviorEntityRole::Subject | BehaviorEntityRole::SelfEntity => Some(subject_entity),
        BehaviorEntityRole::Target => target_entity,
        // Live task state has no independently resolved authored object.
        // Reject that role instead of applying an object action to the target.
        BehaviorEntityRole::Object => None,
    }
}
