use avian3d::prelude::Collisions;
use bevy::{ecs::entity::EntityHashMap, prelude::*};

use openzt2_zoo_state::locomotion::locomotion_types::ContactSteering;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;

#[derive(Component, Default)]
pub(super) struct ContactRecovery(Timer);

#[allow(clippy::type_complexity)]
/// Derives steering constraints from touching contacts with finite normals.
pub(super) fn project_avian_contacts_into_navigation_steering(
    time: Res<Time<Fixed>>,
    collisions: Collisions,
    mut agents: Query<(Entity, &mut ContactSteering, &mut ContactRecovery), With<NavAgent>>,
) {
    let mut directions = EntityHashMap::<Vec3>::default();
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
            *directions.entry(first_colliding_entity).or_default() -= horizontal_contact_normal;
            *directions.entry(second_colliding_entity).or_default() += horizontal_contact_normal;
        }
    }

    let fixed_delta_seconds = time.delta_secs().max(0.0);
    for (entity, mut steering, mut recovery) in &mut agents {
        let direction = directions.get(&entity).copied().unwrap_or_default();
        if steering.is_changed() || recovery.is_added() {
            recovery.0 = super::fixed_interval_timer(steering.recovery_delay_s);
        }
        let mut next = *steering;
        next.direction = direction.normalize_or_zero();
        if direction.length_squared() > f32::EPSILON {
            next.factor = (next.factor - ContactSteering::DECAY_PER_SECOND * fixed_delta_seconds)
                .max(ContactSteering::MINIMUM_FACTOR);
            recovery.0 = super::fixed_interval_timer(ContactSteering::RECOVERY_DELAY_SECONDS);
        } else if !recovery.0.remaining().is_zero() {
            recovery.0.tick(time.delta());
        } else {
            next.factor =
                (next.factor + ContactSteering::RECOVERY_PER_SECOND * fixed_delta_seconds).min(1.0);
        }
        next.recovery_delay_s = recovery.0.remaining_secs();
        steering.set_if_neq(next);
    }
}
