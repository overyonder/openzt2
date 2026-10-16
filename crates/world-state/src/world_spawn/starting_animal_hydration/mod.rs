use bevy::prelude::*;
use openzt2_game_data::{species::NeedKind, AssetId};

/// An imported identity awaiting canonical animal components and initial needs.
#[derive(Component)]
pub struct PendingStartingAnimal {
    pub variant: AssetId,
    pub initial_needs: Vec<(NeedKind, i32)>,
}
