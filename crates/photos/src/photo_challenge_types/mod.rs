//! Persistent state for individual photo challenges and grouped challenge sets.

use bevy::prelude::*;
use openzt2_game_data::{world_scenario::PHOTO_DISTINCT_SUBJECT_CAPACITY, AssetId};

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PhotoChallengeSetProgress {
    pub(crate) completed_members: u16,
    pub(crate) rating_stars: u16,
    pub(crate) selected_subjects: [AssetId; PHOTO_DISTINCT_SUBJECT_CAPACITY],
    pub(crate) selected_subject_count: u8,
}
