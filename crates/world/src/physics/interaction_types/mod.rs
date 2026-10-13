//! General physics identity, movement, impulse, contact, and failure data.

use bevy::prelude::*;

/// Read-only gameplay predicate derived from Avian's authoritative velocity.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MovingFromAvianLinearVelocity;

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct ApplyPhysicsImpulseRequest {
    pub(crate) entity: Entity,
    pub(crate) impulse_ns: Vec3,
    pub(crate) point_world: Vec3,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct PhysicsContactFact {
    pub(crate) a: Entity,
    pub(crate) b: Entity,
    pub(crate) point: Vec3,
    pub(crate) normal: Vec3,
    pub(crate) impulse_ns: f32,
    pub(crate) kind: PhysicsContactFactKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicsContactFactKind {
    Impact,
    Enter,
    Exit,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhysicsInteractionFailure {
    pub(crate) entity: Entity,
    pub(crate) reason: PhysicsInteractionFailureReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicsInteractionFailureReason {
    MissingBody,
    InvalidTarget,
}
