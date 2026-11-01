use openzt2_game_data::{
    AssetId,
    world_definitions::guest_simulation_definitions::{
        GuestNeedSurveyPolicy, GuestNeedSurveySignals,
    },
};
use openzt2_zoo_state::guests::{
    guest_simulation_types::GuestFoodPurchaseCounts,
    guest_survey_types::{
        GUEST_SURVEY_RATINGS, GuestCriticalNeedObservations, GuestSurveyExperience,
    },
};

pub(super) fn score_guest_survey(
    observations: &GuestCriticalNeedObservations,
    experience: GuestSurveyExperience,
    food: &GuestFoodPurchaseCounts,
    policy: GuestNeedSurveyPolicy,
) -> [f32; GUEST_SURVEY_RATINGS] {
    let monitored = [
        ("hunger", GuestNeedSurveySignals::HUNGER),
        ("thirst", GuestNeedSurveySignals::THIRST),
        ("dessert", GuestNeedSurveySignals::DESSERT),
        ("gift", GuestNeedSurveySignals::GIFT),
        ("bathroom", GuestNeedSurveySignals::BATHROOM),
    ];
    let total = monitored
        .iter()
        .filter(|(_, flag)| policy.monitored.contains_all(*flag))
        .map(|(name, _)| observations.observations(AssetId::from_key(name)))
        .sum::<f32>();
    let maximum = policy.maximum_critical_hits_per_month;
    let percent =
        |points: f32, maximum: f32| (f64::from(points) / f64::from(maximum) * 100.0) as f32;
    let need_rate = |name| {
        percent(
            (maximum * 0.5 - observations.observations(AssetId::from_key(name))).max(0.0),
            maximum * 0.5,
        )
    };
    let education_max = policy.maximum_education_points_per_view;
    let entertainment_max = policy.maximum_entertainment_points_per_view;
    let education = percent(
        experience.mean_education().min(education_max),
        education_max,
    );
    let entertainment = percent(
        experience.mean_entertainment().min(entertainment_max),
        entertainment_max,
    );
    let critical = percent((maximum - total).max(0.0), maximum);
    let favorite = if experience.saw_favorite_animal {
        100.0
    } else {
        0.0
    };
    let food_total = food.favorite_q16 as f32 + food.nonfavorite_q16 as f32;
    let favorite_food = if food_total == 0.0 {
        0.0
    } else {
        food.favorite_q16 as f32 / food_total
    };
    let donation_total = experience.found_donation_box + experience.missed_donation_box;
    let donation = if donation_total == 0.0 {
        0.0
    } else {
        experience.found_donation_box / donation_total
    };
    [
        ((f64::from(critical) + f64::from((education + entertainment) * 0.5) + f64::from(favorite))
            / 3.0) as f32,
        experience.mean_happiness(),
        entertainment,
        education.max(0.0),
        need_rate("hunger"),
        need_rate("thirst"),
        need_rate("dessert"),
        favorite_food,
        donation,
        need_rate("gift"),
        need_rate("bathroom"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survey_uses_per_guest_observations_view_means_and_favorite_animal_bonus() {
        let mut observations = GuestCriticalNeedObservations::default();
        for _ in 0..40 {
            observations.observe(AssetId::from_key("hunger"));
        }
        let policy = GuestNeedSurveyPolicy {
            check_interval_ns: 5_000_000_000,
            maximum_critical_hits_per_month: 200.0,
            maximum_education_points_per_view: 100.0,
            maximum_entertainment_points_per_view: 100.0,
            monitored: GuestNeedSurveySignals::HUNGER,
        };
        let experience = GuestSurveyExperience {
            happiness_sum: 80.0,
            happiness_samples: 2,
            entertainment_sum: 200.0,
            entertainment_views: 2,
            education_points: 100.0,
            found_donation_box: 1.0,
            missed_donation_box: 3.0,
            saw_favorite_animal: true,
        };
        let food = GuestFoodPurchaseCounts {
            favorite_q16: 1 << 16,
            nonfavorite_q16: 3 << 16,
        };
        let ratings = score_guest_survey(&observations, experience, &food, policy);
        assert_eq!(ratings[0], 85.0);
        assert_eq!(ratings[1], 40.0);
        assert_eq!(ratings[2], 100.0);
        assert_eq!(ratings[3], 50.0);
        assert_eq!(ratings[4], 60.0);
        assert_eq!(ratings[5], 100.0);
        assert_eq!(ratings[7], 0.25);
        assert_eq!(ratings[8], 0.25);
        let without_favorite = score_guest_survey(
            &observations,
            GuestSurveyExperience {
                saw_favorite_animal: false,
                ..experience
            },
            &food,
            policy,
        );
        assert_eq!(without_favorite[0], 155.0 / 3.0);
    }
}
