use bevy::prelude::*;

use super::{
    award_and_progression_fact_types::ScenarioAwardPointTotal, fame_history_types::FameHistory,
    fame_types::Fame, profile_challenge_types::TotalEndangeredAnimalBirthCount,
    rating_types::ZooRating, unlock_types::UnlockedCatalogueDefinitionSet,
};

pub(super) fn reset_all_progression_state_when_game_session_ends(
    mut fame: ResMut<Fame>,
    mut rating: ResMut<ZooRating>,
    mut unlocked_catalogue_definitions: ResMut<UnlockedCatalogueDefinitionSet>,
    mut scenario_award_point_total: ResMut<ScenarioAwardPointTotal>,
    mut total_endangered_animal_births: ResMut<TotalEndangeredAnimalBirthCount>,
    mut fame_history: ResMut<FameHistory>,
) {
    *fame = Fame::default();
    *rating = ZooRating::default();
    unlocked_catalogue_definitions.words.fill(0);
    unlocked_catalogue_definitions.definition_count = 0;
    scenario_award_point_total.0 = 0;
    total_endangered_animal_births.0 = 0;
    fame_history.clear();
}
