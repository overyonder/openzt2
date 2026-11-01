use bevy::prelude::*;

pub const GUEST_SURVEY_DAYS: usize = 31;
pub const GUEST_SURVEY_RATINGS: usize = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum GuestSurveyRating {
    Recommendation,
    Happiness,
    Entertainment,
    Education,
    Food,
    Drink,
    Dessert,
    FavoriteFood,
    Donation,
    Gift,
    Bathroom,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GuestSurveyRatingHistory {
    pub cumulative_rating: f32,
    pub cumulative_count: u32,
    pub daily_ratings: [f32; GUEST_SURVEY_DAYS],
    pub daily_counts: [u32; GUEST_SURVEY_DAYS],
}

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct GuestSurvey {
    pub critical_need_countdown_seconds: f32,
    pub previous_day_of_month: i32,
    pub current_day_of_month: i32,
    pub ratings: [GuestSurveyRatingHistory; GUEST_SURVEY_RATINGS],
}

impl Default for GuestSurvey {
    fn default() -> Self {
        Self {
            critical_need_countdown_seconds: 0.0,
            previous_day_of_month: -1,
            current_day_of_month: -1,
            ratings: [GuestSurveyRatingHistory::default(); GUEST_SURVEY_RATINGS],
        }
    }
}

impl GuestSurvey {
    /// Called once for the guest whose thinker is executing.
    pub fn sample_guest_needs_now(&mut self, delta_seconds: f32, interval_seconds: f32) -> bool {
        if self.critical_need_countdown_seconds > 0.0 {
            self.critical_need_countdown_seconds -= delta_seconds;
            false
        } else {
            self.critical_need_countdown_seconds = interval_seconds;
            true
        }
    }

    pub fn advance_day_of_month(&mut self, day: u8) {
        let Some(index) = day
            .checked_sub(1)
            .map(usize::from)
            .filter(|day| *day < GUEST_SURVEY_DAYS)
        else {
            return;
        };
        if self.current_day_of_month == i32::from(day) {
            return;
        }
        self.previous_day_of_month = self.current_day_of_month;
        self.current_day_of_month = i32::from(day);
        for history in &mut self.ratings {
            history.daily_ratings[index] = 0.0;
            history.daily_counts[index] = 0;
            history.cumulative_rating = 0.0;
            history.cumulative_count = 0;
            for index in 0..GUEST_SURVEY_DAYS {
                history.cumulative_rating += history.daily_ratings[index];
                history.cumulative_count = history
                    .cumulative_count
                    .wrapping_add(history.daily_counts[index]);
            }
        }
    }

    pub fn rating_permille(&self, rating: GuestSurveyRating) -> Option<u16> {
        let history = &self.ratings[rating as usize];
        let scale = match rating {
            GuestSurveyRating::FavoriteFood | GuestSurveyRating::Donation => 1_000.0,
            _ => 10.0,
        };
        (history.cumulative_count != 0).then(|| {
            (f64::from(history.cumulative_rating) / f64::from(history.cumulative_count) * scale)
                .clamp(0.0, 1_000.0) as u16
        })
    }

    pub fn record_rating(&mut self, rating: GuestSurveyRating, value: f32) {
        let Some(index) = self
            .current_day_of_month
            .checked_sub(1)
            .and_then(|day| usize::try_from(day).ok())
            .filter(|day| *day < GUEST_SURVEY_DAYS)
        else {
            return;
        };
        let history = &mut self.ratings[rating as usize];
        history.daily_ratings[index] += value;
        history.daily_counts[index] = history.daily_counts[index].wrapping_add(1);
    }
}

/// Critical observations belong to each guest's authored need points.
#[derive(Component, Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GuestCriticalNeedObservations(pub Vec<(openzt2_game_data::AssetId, f32)>);

impl GuestCriticalNeedObservations {
    pub fn observe(&mut self, need: openzt2_game_data::AssetId) {
        if let Some((_, observations)) = self.0.iter_mut().find(|(id, _)| *id == need) {
            *observations += 1.0;
        } else {
            self.0.push((need, 1.0));
        }
    }

    pub fn observations(&self, need: openzt2_game_data::AssetId) -> f32 {
        self.0
            .iter()
            .find(|(id, _)| *id == need)
            .map_or(0.0, |(_, count)| *count)
    }
}

/// Inputs accumulated by the guest's thinker, accepted views and authored facts.
#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct GuestSurveyExperience {
    pub happiness_sum: f32,
    pub happiness_samples: u32,
    pub entertainment_sum: f32,
    pub entertainment_views: u32,
    pub education_points: f32,
    pub found_donation_box: f32,
    pub missed_donation_box: f32,
    pub saw_favorite_animal: bool,
}

impl GuestSurveyExperience {
    pub fn observe_happiness(&mut self, source_value: f32) {
        self.happiness_sum += source_value;
        self.happiness_samples = self.happiness_samples.wrapping_add(1);
    }

    pub fn observe_entertainment(&mut self, score: f32) {
        self.entertainment_sum += score;
        if score != 0.0 {
            self.entertainment_views = self.entertainment_views.wrapping_add(1);
        }
    }

    pub fn mean_happiness(self) -> f32 {
        (f64::from(self.happiness_sum) / f64::from(self.happiness_samples.max(1))) as f32
    }

    pub fn mean_entertainment(self) -> f32 {
        if self.entertainment_views == 0 {
            0.0
        } else {
            (f64::from(self.entertainment_sum) / f64::from(self.entertainment_views)) as f32
        }
    }

    pub fn mean_education(self) -> f32 {
        if self.entertainment_views == 0 {
            self.education_points
        } else {
            (f64::from(self.education_points) / f64::from(self.entertainment_views)) as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_happiness_and_rating_channel_units_are_preserved() {
        let mut experience = GuestSurveyExperience::default();
        for _ in 0..3 {
            experience.observe_happiness(0.25);
        }
        assert_eq!(experience.happiness_sum, 0.75);
        assert_eq!(experience.mean_happiness(), 0.25);
        let mut survey = GuestSurvey::default();
        for (kind, sum) in [
            (GuestSurveyRating::Education, 150.0),
            (GuestSurveyRating::Entertainment, 150.0),
            (GuestSurveyRating::FavoriteFood, 1.5),
            (GuestSurveyRating::Donation, 1.5),
        ] {
            let history = &mut survey.ratings[kind as usize];
            history.cumulative_rating = sum;
            history.cumulative_count = 2;
            assert_eq!(survey.rating_permille(kind), Some(750));
        }
    }

    #[test]
    fn shared_countdown_samples_only_on_a_later_guest_call_after_crossing_zero() {
        let mut survey = GuestSurvey {
            critical_need_countdown_seconds: 0.1,
            ..Default::default()
        };
        assert!(!survey.sample_guest_needs_now(0.2, 5.0));
        assert_eq!(survey.critical_need_countdown_seconds, -0.1);
        assert!(survey.sample_guest_needs_now(0.2, 5.0));
        assert_eq!(survey.critical_need_countdown_seconds, 5.0);
    }

    #[test]
    fn same_day_preserves_loaded_history_and_new_day_recycles_only_its_slot() {
        let mut survey = GuestSurvey {
            previous_day_of_month: 9,
            current_day_of_month: 1,
            critical_need_countdown_seconds: 5.0,
            ..Default::default()
        };
        let history = &mut survey.ratings[GuestSurveyRating::Food as usize];
        history.daily_ratings[0] = 10.0;
        history.daily_counts[0] = 2;
        history.daily_ratings[1] = 90.0;
        history.daily_counts[1] = 3;
        history.daily_ratings[30] = 20.0;
        history.daily_counts[30] = 1;
        history.cumulative_rating = 7.0;
        history.cumulative_count = 5;
        let before = survey;
        survey.advance_day_of_month(1);
        assert_eq!(survey, before);
        survey.advance_day_of_month(2);
        let history = &survey.ratings[GuestSurveyRating::Food as usize];
        assert_eq!(history.daily_ratings[1], 0.0);
        assert_eq!(history.daily_counts[1], 0);
        assert_eq!(history.cumulative_rating, 30.0);
        assert_eq!(history.cumulative_count, 3);
        assert_eq!(survey.previous_day_of_month, 1);
        assert_eq!(survey.current_day_of_month, 2);
        assert_eq!(survey.critical_need_countdown_seconds, 5.0);
        survey.record_rating(GuestSurveyRating::Food, 25.0);
        assert_eq!(
            survey.ratings[GuestSurveyRating::Food as usize].cumulative_rating,
            30.0
        );
        assert_eq!(
            survey.ratings[GuestSurveyRating::Food as usize].daily_ratings[1],
            25.0
        );
    }
}
