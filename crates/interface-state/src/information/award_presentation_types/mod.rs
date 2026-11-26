use bevy::prelude::*;
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

#[derive(Component)]
#[relationship(relationship_target = WorldAwardPrograms)]
pub struct AwardProgramOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = AwardProgramOf, linked_spawn)]
pub struct WorldAwardPrograms(Vec<Entity>);

#[derive(Component, Default)]
pub struct AwardGroupPresentation {
    pub document: String,
    pub persistent: bool,
    pub milestones: BTreeMap<usize, AwardMilestonePresentation>,
}

#[derive(Default)]
pub struct AwardMilestonePresentation {
    pub progress: f32,
    pub received: bool,
}

#[derive(Message)]
pub enum AwardProgramEffect {
    ShowAwardPanel {
        root: Entity,
    },
    PlaceCratedObject {
        root: Entity,
        definition: AssetId,
    },
    ShowChallengeWon {
        root: Entity,
        arguments: Vec<String>,
    },
    ReplaceZooWalls {
        root: Entity,
        definition: AssetId,
    },
    AddAmbientSound {
        root: Entity,
        cue: AssetId,
    },
}
