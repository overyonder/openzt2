use bevy::prelude::*;

/// Links a tank to a fence segment. Separate relationship entities let a
/// shared wall border more than one tank.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct TankBoundaryMember;

/// Tank endpoint of a [`TankBoundaryMember`] relationship entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = TankBoundaryMemberships)]
pub(super) struct TankBoundaryOf(pub Entity);

/// Inverse membership collection maintained by Bevy on the tank entity.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = TankBoundaryOf)]
pub(super) struct TankBoundaryMemberships(Vec<Entity>);

/// Fence endpoint of a [`TankBoundaryMember`] relationship entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = TankSegmentMemberships)]
pub(super) struct TankBoundarySegment(pub Entity);

/// Inverse membership collection maintained by Bevy on a fence entity.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = TankBoundarySegment)]
pub(super) struct TankSegmentMemberships(Vec<Entity>);
