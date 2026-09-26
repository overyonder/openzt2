use bevy::prelude::*;

/// Zoo-local cumulative births carrying the canonical endangered marker.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TotalEndangeredAnimalBirthCount(pub u32);

/// Persistent profile-scoped challenge completion totals.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProfileChallengeCompletionCounts {
    pub all: u32,
    pub photo: u32,
    pub marine_animal: u32,
    pub marine_show: u32,
    pub endangered: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletedChallengeFamily {
    All,
    Photo,
    MarineAnimal,
    MarineShow,
    Endangered,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordCompletedChallengeRequest {
    pub family: CompletedChallengeFamily,
}
