use super::fame_types::Fame;

pub(crate) fn update_current_and_maximum_fame(
    fame: &mut Fame,
    next_half_stars: u8,
    next_percent: f32,
) -> bool {
    let previous_fame = *fame;
    fame.half_stars = next_half_stars;
    fame.maximum_reached = fame.maximum_reached.max(next_half_stars);
    fame.maximum_percent_reached = Some(
        fame.maximum_percent_reached
            .map_or(next_percent, |previous| previous.max(next_percent)),
    );
    *fame != previous_fame
}

pub(crate) fn select_fame_level_for_rating_and_guest_count(
    thresholds: impl Iterator<Item = (u16, u16, u32)>,
    rating_permille: u16,
    guest_count: u32,
) -> u8 {
    thresholds
        .filter(|(_, minimum_rating, minimum_guests)| {
            rating_permille >= *minimum_rating && guest_count >= *minimum_guests
        })
        .map(|(level, _, _)| level.min(u16::from(u8::MAX)) as u8)
        .max()
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct ZooFameCalculationInputs {
    pub animal_count: u32,
    pub species_count: u32,
    pub animal_happiness_permille: u16,
    pub guest_count: u32,
    pub guest_happiness_permille: u16,
    pub education_view_rate_permille: u16,
    pub entertainment_view_rate_permille: u16,
    pub award_and_scenario_points: u32,
    pub animals_released: u32,
}

pub(crate) fn calculate_authentic_zoo_fame(inputs: &ZooFameCalculationInputs) -> (u8, f32) {
    // 1. Animals
    // NUM_ANIMAL_WEIGHT = 10, MAX_NUM_ANIMAL = 35
    let raw_animal_fame = (inputs.animal_count as f32).min(35.0) * 10.0 / 35.0;
    // NUM_SPECIES_WEIGHT = 40, MAX_NUM_SPECIES = 20
    let raw_species_fame = (inputs.species_count as f32).min(20.0) * 40.0 / 20.0;
    let potential_animal_fame = raw_animal_fame + raw_species_fame;
    // MAX_ANIMAL_HAPPINESS = 70 (on 0..100 percentage scale)
    let avg_animal_happiness = (inputs.animal_happiness_permille as f32) / 10.0;
    let adjusted_animal_fame = if inputs.animal_count == 0 {
        0.0
    } else {
        (potential_animal_fame * avg_animal_happiness) / 70.0
    };

    // 2. Guests
    // EDUCATION_WEIGHT = 15, MAX_EDUCATION = 6
    let education_ratio = (inputs.education_view_rate_permille as f32) / 1000.0;
    let raw_education = education_ratio.clamp(0.0, 1.0) * 15.0;
    // ENTERTAINMENT_WEIGHT = 20, MAX_ENTERTAINMENT = 50
    let entertainment_ratio = (inputs.entertainment_view_rate_permille as f32) / 1000.0;
    let raw_entertain = entertainment_ratio.clamp(0.0, 1.0) * 20.0;
    let potential_guest_fame = raw_education + raw_entertain;
    // MAX_GUEST_HAPPINESS = 80
    let avg_guest_happiness = (inputs.guest_happiness_permille as f32) / 10.0;
    let adjusted_guest_fame = if inputs.guest_count == 0 {
        0.0
    } else {
        (potential_guest_fame * avg_guest_happiness) / 80.0
    };

    // 3. Zoo
    // MAX_GOAL_POINTS = 8, GOAL_POINTS_WEIGHT = 15
    let goals = (inputs.award_and_scenario_points as f32).min(8.0) * 15.0 / 8.0;
    // MAX_RELEASE_WILD = 10, RELEASE_WILD_WEIGHT = 15
    let released = (inputs.animals_released as f32).min(10.0) * 15.0 / 10.0;
    let total_zoo_fame = goals + released;

    let total_fame = adjusted_animal_fame + adjusted_guest_fame + total_zoo_fame;
    let clamped_fame = total_fame.clamp(1.0, 100.0);
    let half_stars = (clamped_fame / 10.0).floor().clamp(1.0, 10.0) as u8;

    (half_stars, clamped_fame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_zoo_yields_minimum_fame() {
        let inputs = ZooFameCalculationInputs::default();
        let (half_stars, percent) = calculate_authentic_zoo_fame(&inputs);
        assert_eq!(half_stars, 1);
        assert_eq!(percent, 1.0);
    }

    #[test]
    fn modest_zoo_advances_past_twenty_percent() {
        let inputs = ZooFameCalculationInputs {
            animal_count: 4,
            species_count: 2,
            animal_happiness_permille: 700,
            guest_count: 10,
            guest_happiness_permille: 800,
            education_view_rate_permille: 400,
            entertainment_view_rate_permille: 450,
            award_and_scenario_points: 1,
            animals_released: 0,
        };
        let (half_stars, percent) = calculate_authentic_zoo_fame(&inputs);
        assert_eq!(half_stars, 2);
        assert!(percent >= 20.0);
    }

    #[test]
    fn max_capacity_zoo_yields_five_full_stars() {
        let inputs = ZooFameCalculationInputs {
            animal_count: 35,
            species_count: 20,
            animal_happiness_permille: 700,
            guest_count: 50,
            guest_happiness_permille: 800,
            education_view_rate_permille: 1000,
            entertainment_view_rate_permille: 1000,
            award_and_scenario_points: 8,
            animals_released: 10,
        };
        let (half_stars, percent) = calculate_authentic_zoo_fame(&inputs);
        assert_eq!(half_stars, 10);
        assert_eq!(percent, 100.0);
    }
}
