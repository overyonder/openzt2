use bevy::prelude::*;

#[derive(Component, Clone, Debug)]
#[relationship(relationship_target = AnimationRigJoints, allow_self_referential)]
pub struct AnimationJointTarget {
    #[relationship]
    pub animation_playback_controller_entity: Entity,
    pub joint_asset_key: String,
}

#[derive(Component, Debug)]
#[relationship_target(relationship = AnimationJointTarget)]
pub struct AnimationRigJoints(Vec<Entity>);

impl AnimationRigJoints {
    pub fn joints(&self) -> &[Entity] {
        &self.0
    }
}
