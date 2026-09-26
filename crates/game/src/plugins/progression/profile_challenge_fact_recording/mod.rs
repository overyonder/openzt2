use bevy::prelude::*;

use crate::plugins::animal_lifecycle::{
    animal_birth_and_release_contracts::AnimalBorn, types::EndangeredSpecies,
};

use super::profile_challenge_types::{
    CompletedChallengeFamily, ProfileChallengeCompletionCounts, RecordCompletedChallengeRequest,
    TotalEndangeredAnimalBirthCount,
};

pub(super) fn record_endangered_animal_births_for_profile_challenges(
    mut animal_births: MessageReader<AnimalBorn>,
    endangered_animals: Query<(), With<EndangeredSpecies>>,
    mut total_endangered_animal_births: ResMut<TotalEndangeredAnimalBirthCount>,
) {
    for animal_birth in animal_births.read() {
        if endangered_animals.contains(animal_birth.child) {
            total_endangered_animal_births.0 = total_endangered_animal_births.0.saturating_add(1);
        }
    }
}

pub(super) fn record_completed_profile_challenge_family_counts(
    mut completed_challenges: MessageReader<RecordCompletedChallengeRequest>,
    mut completion_counts: ResMut<ProfileChallengeCompletionCounts>,
) {
    for completed_challenge in completed_challenges.read() {
        let completed_family_count = match completed_challenge.family {
            CompletedChallengeFamily::All => &mut completion_counts.all,
            CompletedChallengeFamily::Photo => &mut completion_counts.photo,
            CompletedChallengeFamily::MarineAnimal => &mut completion_counts.marine_animal,
            CompletedChallengeFamily::MarineShow => &mut completion_counts.marine_show,
            CompletedChallengeFamily::Endangered => &mut completion_counts.endangered,
        };
        *completed_family_count = completed_family_count.saturating_add(1);
    }
}
