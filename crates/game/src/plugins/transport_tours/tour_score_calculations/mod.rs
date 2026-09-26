use super::tour_scoring_types::TourScore;

pub(crate) fn add_observation_value_to_tour_score(
    tour_score: &mut TourScore,
    observation_value: f32,
) {
    if !tour_score.value.is_finite() || !observation_value.is_finite() {
        return;
    }
    tour_score.value = (tour_score.value + observation_value).max(0.0);
    tour_score.observations = tour_score.observations.saturating_add(1);
}
