//! Avian movement projection, impulse application, and gameplay contact facts.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::interaction_types::{
    ApplyPhysicsImpulseRequest, MovingFromAvianLinearVelocity, PhysicsContactFact,
    PhysicsContactFactKind, PhysicsInteractionFailure, PhysicsInteractionFailureReason,
};

const MOVING_SPEED_SQUARED: f32 = 0.0001;
pub(super) fn synchronize_derived_moving_components_from_changed_avian_velocities(
    mut commands: Commands,
    bodies: Query<
        (
            Entity,
            &LinearVelocity,
            Option<&MovingFromAvianLinearVelocity>,
        ),
        Changed<LinearVelocity>,
    >,
) {
    for (entity, velocity, moving) in &bodies {
        let is_moving =
            velocity.0.is_finite() && velocity.0.length_squared() > MOVING_SPEED_SQUARED;
        match (is_moving, moving.is_some()) {
            (true, false) => {
                commands
                    .entity(entity)
                    .insert(MovingFromAvianLinearVelocity);
            }
            (false, true) => {
                commands
                    .entity(entity)
                    .remove::<MovingFromAvianLinearVelocity>();
            }
            _ => {}
        }
    }
}

/// Applies validated point impulses only to authoritative dynamic Avian bodies.
pub(super) fn apply_requested_impulses_to_dynamic_avian_bodies(
    mut requests: MessageReader<ApplyPhysicsImpulseRequest>,
    mut bodies: Query<(&RigidBody, Forces)>,
    mut failed: MessageWriter<PhysicsInteractionFailure>,
) {
    for request in requests.read() {
        if !request.impulse_ns.is_finite() || !request.point_world.is_finite() {
            failed.write(PhysicsInteractionFailure {
                entity: request.entity,
                reason: PhysicsInteractionFailureReason::InvalidTarget,
            });
            continue;
        }
        let Ok((body, mut forces)) = bodies.get_mut(request.entity) else {
            failed.write(PhysicsInteractionFailure {
                entity: request.entity,
                reason: PhysicsInteractionFailureReason::MissingBody,
            });
            continue;
        };
        if *body != RigidBody::Dynamic {
            failed.write(PhysicsInteractionFailure {
                entity: request.entity,
                reason: PhysicsInteractionFailureReason::InvalidTarget,
            });
            continue;
        }
        forces.apply_linear_impulse_at_point(request.impulse_ns, request.point_world);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn publish_gameplay_contact_facts_from_avian_collision_events(
    mut starts: MessageReader<CollisionStart>,
    mut ends: MessageReader<CollisionEnd>,
    collisions: Collisions,
    mut facts: MessageWriter<PhysicsContactFact>,
) {
    for event in starts.read() {
        let (a, b) = resolve_collision_event_body_entities(
            event.collider1,
            event.collider2,
            event.body1,
            event.body2,
        );
        let contact = collisions
            .get(event.collider1, event.collider2)
            .and_then(find_strongest_contact_point);
        let (point, normal, impulse_ns) = contact.unwrap_or((Vec3::ZERO, Vec3::ZERO, 0.0));
        facts.write(PhysicsContactFact {
            a,
            b,
            point,
            normal,
            impulse_ns,
            kind: PhysicsContactFactKind::Enter,
        });
        if impulse_ns > 0.0 {
            facts.write(PhysicsContactFact {
                a,
                b,
                point,
                normal,
                impulse_ns,
                kind: PhysicsContactFactKind::Impact,
            });
        }
    }
    for event in ends.read() {
        let (a, b) = resolve_collision_event_body_entities(
            event.collider1,
            event.collider2,
            event.body1,
            event.body2,
        );
        facts.write(PhysicsContactFact {
            a,
            b,
            point: Vec3::ZERO,
            normal: Vec3::ZERO,
            impulse_ns: 0.0,
            kind: PhysicsContactFactKind::Exit,
        });
    }
}

#[inline]
fn resolve_collision_event_body_entities(
    collider1: Entity,
    collider2: Entity,
    body1: Option<Entity>,
    body2: Option<Entity>,
) -> (Entity, Entity) {
    (body1.unwrap_or(collider1), body2.unwrap_or(collider2))
}

fn find_strongest_contact_point(pair: &ContactPair) -> Option<(Vec3, Vec3, f32)> {
    pair.manifolds
        .iter()
        .flat_map(|manifold| {
            manifold
                .points
                .iter()
                .map(move |point| (point.point, manifold.normal, point.normal_impulse))
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
}
