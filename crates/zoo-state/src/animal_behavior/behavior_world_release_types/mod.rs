use bevy::prelude::*;

/// Installed by the world placement and motion owner for the supported releases.
#[derive(Resource, Default)]
pub struct BehaviorWorldReleaseSupport {
    pub drop: bool,
    pub fall: bool,
}

#[derive(Component, Clone, Copy)]
pub enum BehaviorWorldReleaseMotion {
    Drop,
    Fall,
}

impl BehaviorWorldReleaseMotion {
    pub fn available(self, support: Option<&BehaviorWorldReleaseSupport>) -> bool {
        support.is_some_and(|support| match self {
            Self::Drop => support.drop,
            Self::Fall => support.fall,
        })
    }
}

/// The item is detached and its last joint pose is available to world physics.
#[derive(EntityEvent)]
pub struct BehaviorWorldObjectReleased {
    #[event_target]
    pub object: Entity,
    pub world: Entity,
    pub pose: Transform,
    pub motion: BehaviorWorldReleaseMotion,
}
