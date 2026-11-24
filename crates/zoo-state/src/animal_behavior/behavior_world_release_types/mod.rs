use bevy::prelude::*;

#[derive(Component, Clone, Copy)]
pub enum BehaviorWorldReleaseMotion {
    Drop,
    Fall { speed_mps: f32 },
}

/// Optional animation keys authored on the released subject's definition.
#[derive(Component, Clone, Debug, Default)]
pub struct FallReleaseAnimationClips {
    pub airborne: Option<String>,
    pub land_impact: Option<String>,
    pub water_impact: Option<String>,
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
