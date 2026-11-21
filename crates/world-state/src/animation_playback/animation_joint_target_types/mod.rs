use bevy::prelude::*;

#[derive(Component, Clone, Debug)]
#[relationship(relationship_target = AnimationRigJoints, allow_self_referential)]
pub struct AnimationJointTarget {
    #[relationship]
    pub animation_playback_controller_entity: Entity,
    pub joint_asset_key: String,
    pub authored_joint_id: openzt2_game_data::AssetId,
}

#[derive(Component, Debug)]
#[relationship_target(relationship = AnimationJointTarget)]
pub struct AnimationRigJoints(Vec<Entity>);

impl AnimationRigJoints {
    pub fn joints(&self) -> &[Entity] {
        &self.0
    }
}
