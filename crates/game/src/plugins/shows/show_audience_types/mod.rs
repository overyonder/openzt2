use bevy::prelude::*;

/// Subscription at the guest's current watching target (path, seat, or viewing
/// area). This does not establish which running show owns that target.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = ShowTargetAudience)]
pub(super) struct GuestShowAttendance {
    #[relationship]
    pub(super) target: Entity,
    pub(super) execution_id: u64,
}

#[derive(Component, Default)]
#[relationship_target(relationship = GuestShowAttendance)]
pub(super) struct ShowTargetAudience(Vec<Entity>);
