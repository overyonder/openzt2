use avian3d::prelude::Collisions;
use bevy::prelude::*;

use super::locomotion_types::{ContactSteering, NavAgent};

#[allow(clippy::type_complexity)]
/// Derives steering constraints from touching contacts with finite normals.
pub(super) fn project_avian_contacts_into_navigation_steering(
    time: Res<Time<Fixed>>,
    collisions: Collisions,
    mut navigation_agent_contact_steering: Query<&mut ContactSteering, With<NavAgent>>,
) {
    for mut contact_steering in &mut navigation_agent_contact_steering {
        contact_steering.direction = Vec3::ZERO;
    }

    for collision_pair in collisions
        .iter()
        .filter(|collision_pair| collision_pair.generates_constraints())
    {
        let first_colliding_entity = collision_pair.body1.unwrap_or(collision_pair.collider1);
        let second_colliding_entity = collision_pair.body2.unwrap_or(collision_pair.collider2);
        if first_colliding_entity == second_colliding_entity {
            continue;
        }
        for contact_manifold in collision_pair
            .manifolds
            .iter()
            .filter(|contact_manifold| contact_manifold.normal.is_finite())
        {
            let mut horizontal_contact_normal = contact_manifold.normal;
            horizontal_contact_normal.y = 0.0;
            let horizontal_contact_normal = horizontal_contact_normal.normalize_or_zero();
            if horizontal_contact_normal == Vec3::ZERO {
                continue;
            }
            if let Ok(mut contact_steering) =
                navigation_agent_contact_steering.get_mut(first_colliding_entity)
            {
                contact_steering.direction -= horizontal_contact_normal;
            }
            if let Ok(mut contact_steering) =
                navigation_agent_contact_steering.get_mut(second_colliding_entity)
            {
                contact_steering.direction += horizontal_contact_normal;
            }
        }
    }

    let fixed_delta_seconds = time.delta_secs().max(0.0);
    for mut contact_steering in &mut navigation_agent_contact_steering {
        let navigation_agent_is_obstructed =
            contact_steering.direction.length_squared() > f32::EPSILON;
        contact_steering.direction = contact_steering.direction.normalize_or_zero();
        if navigation_agent_is_obstructed {
            contact_steering.factor = (contact_steering.factor
                - ContactSteering::DECAY_PER_SECOND * fixed_delta_seconds)
                .max(ContactSteering::MINIMUM_FACTOR);
            contact_steering.recovery_delay_s = ContactSteering::RECOVERY_DELAY_SECONDS;
        } else if contact_steering.recovery_delay_s > 0.0 {
            contact_steering.recovery_delay_s =
                (contact_steering.recovery_delay_s - fixed_delta_seconds).max(0.0);
        } else {
            contact_steering.factor = (contact_steering.factor
                + ContactSteering::RECOVERY_PER_SECOND * fixed_delta_seconds)
                .min(1.0);
        }
    }
}
