use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component)]
pub struct DiseaseResearchFunding(pub f32);

impl Default for DiseaseResearchFunding {
    fn default() -> Self {
        Self(0.5)
    }
}

#[derive(Component)]
pub struct DiseaseExposureDeadline(pub u64);

#[derive(Component)]
pub struct ActiveZooDisease {
    pub definition: AssetId,
    pub cure_due_tick: u64,
}

#[derive(Component)]
#[relationship(relationship_target = AssignedDiseaseTargets)]
pub struct AssignedDiseaseTarget {
    #[relationship]
    pub root: Entity,
    pub disease: AssetId,
}

#[derive(Component)]
#[relationship_target(relationship = AssignedDiseaseTarget, linked_spawn)]
pub struct AssignedDiseaseTargets(Vec<Entity>);
