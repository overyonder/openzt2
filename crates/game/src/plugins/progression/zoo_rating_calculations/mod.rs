use openzt2_game_data::world_definitions::catalogue_and_progression::zoo_rating_fame_and_award_definition_types::{
    Comparison, RatingInputKind,
};

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use super::rating_types::ZooRating;

pub(crate) fn calculate_mean_permille<I>(values: I) -> u16
where
    I: Iterator<Item = u16>,
{
    let (sum, count) = values.fold((0_u64, 0_u64), |(sum, count), value| {
        (sum + u64::from(value.min(1000)), count + 1)
    });
    if count == 0 {
        0
    } else {
        ((sum + count / 2) / count).min(1000) as u16
    }
}

pub(crate) fn normalize_value_to_permille(value: i64, minimum: i32, maximum: i32) -> u16 {
    let minimum = i64::from(minimum);
    let maximum = i64::from(maximum);
    if maximum <= minimum {
        return u16::from(value >= maximum) * 1000;
    }
    let clamped = value.clamp(minimum, maximum) - minimum;
    let span = maximum - minimum;
    ((clamped * 1000 + span / 2) / span) as u16
}

pub(crate) fn calculate_weighted_zoo_rating(
    values_and_weights: impl Iterator<Item = (u16, i16)>,
    minimum: u16,
    maximum: u16,
) -> u16 {
    let (weighted_sum, total_weight) = values_and_weights.fold(
        (0_i64, 0_i64),
        |(weighted_sum, total_weight), (value, weight)| {
            if weight <= 0 {
                (weighted_sum, total_weight)
            } else {
                (
                    weighted_sum + i64::from(value.min(1000)) * i64::from(weight),
                    total_weight + i64::from(weight),
                )
            }
        },
    );
    let raw_rating = if total_weight == 0 {
        0
    } else {
        ((weighted_sum + total_weight / 2) / total_weight).clamp(0, 1000) as u16
    };
    raw_rating.clamp(minimum.min(1000), maximum.min(1000).max(minimum.min(1000)))
}

pub(crate) fn smooth_zoo_rating_toward_target(
    current: u16,
    target: u16,
    smoothing_ticks: u32,
) -> u16 {
    if smoothing_ticks <= 1 || current == target {
        return target;
    }
    let delta = i64::from(target) - i64::from(current);
    let divisor = i64::from(smoothing_ticks);
    let step = if delta > 0 {
        (delta + divisor - 1) / divisor
    } else {
        -((-delta + divisor - 1) / divisor)
    };
    (i64::from(current) + step).clamp(0, 1000) as u16
}

pub(crate) fn select_zoo_rating_input(rating: &ZooRating, kind: &RatingInputKind) -> Option<u16> {
    Some(match kind {
        RatingInputKind::AnimalWelfare => rating.animal_welfare_permille,
        RatingInputKind::GuestSatisfaction => rating.guest_satisfaction_permille,
        RatingInputKind::Education => rating.education_permille,
        RatingInputKind::Variety => rating.variety_permille,
        RatingInputKind::Scenery => rating.scenery_permille,
        RatingInputKind::Finance => rating.finance_permille,
        RatingInputKind::Cleanliness => {
            return rating
                .cleanliness_available
                .then_some(rating.cleanliness_permille);
        }
    })
}

pub(crate) fn normalize_zoo_rating_input_for_kind(
    definitions: WorldDefinitionsView<'_>,
    rating_kind: &RatingInputKind,
    source_value: i64,
) -> u16 {
    definitions
        .rating_definitions()
        .flat_map(|definition| &definition.inputs)
        .find(|input| &input.kind == rating_kind)
        .map_or(0, |input| {
            normalize_value_to_permille(source_value, input.minimum, input.maximum)
        })
}

pub(crate) fn zoo_rating_satisfies_authored_comparison(
    current_rating: u16,
    authored_value: i32,
    comparison: &Comparison,
) -> bool {
    let current_rating = i32::from(current_rating);
    match comparison {
        Comparison::AtLeast => current_rating >= authored_value,
        Comparison::AtMost => current_rating <= authored_value,
        Comparison::Equal => current_rating == authored_value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_zoo_rating_calculations_round_and_smooth_without_overshoot() {
        assert_eq!(calculate_mean_permille([0, 1000, 1000].into_iter()), 667);
        assert_eq!(normalize_value_to_permille(50, 0, 100), 500);
        assert_eq!(normalize_value_to_permille(-20, 0, 100), 0);
        assert_eq!(
            calculate_weighted_zoo_rating([(1000, 1), (0, 3)].into_iter(), 0, 1000),
            250
        );
        assert_eq!(smooth_zoo_rating_toward_target(100, 900, 4), 300);
        assert_eq!(smooth_zoo_rating_toward_target(900, 100, 4), 700);
        assert_eq!(smooth_zoo_rating_toward_target(999, 1000, 100), 1000);
    }
}
