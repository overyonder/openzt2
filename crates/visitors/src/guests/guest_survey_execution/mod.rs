use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooCalendar,
    world_spawn::world_load_completion_marker::WorldLoadCompleted,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_trigger_types::BehaviorSubjectTrigger,
    guests::{
        guest_simulation_types::{AdjustGuestSurveyData, GuestArchetype, GuestFoodPurchaseCounts},
        guest_survey_types::{
            GuestCriticalNeedObservations, GuestSurvey, GuestSurveyExperience, GuestSurveyRating,
        },
    },
};

pub(super) fn initialize_guest_survey(
    roots: Query<(), With<WorldLoadCompleted>>,
    survey: Option<Res<GuestSurvey>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    mut commands: Commands,
) {
    if survey.is_some() || roots.is_empty() {
        return;
    }
    let Some(policy) = active
        .get(&definitions)
        .and_then(|definitions| definitions.guest_generation())
    else {
        return;
    };
    commands.insert_resource(GuestSurvey {
        critical_need_countdown_seconds: policy.need_survey.check_interval_ns as f32 * 1e-9,
        ..Default::default()
    });
}

pub(super) fn advance_guest_survey_day(
    calendar: Res<ZooCalendar>,
    survey: Option<ResMut<GuestSurvey>>,
) {
    if let Some(mut survey) = survey {
        survey.advance_day_of_month(calendar.day);
    }
}

pub(super) fn route_guest_survey_triggers(
    mut triggers: MessageReader<BehaviorSubjectTrigger>,
    mut requests: MessageWriter<AdjustGuestSurveyData>,
) {
    let survey = AssetId::from_key("adjust_surveydata");
    for trigger in triggers.read().filter(|trigger| trigger.trigger == survey) {
        requests.write(AdjustGuestSurveyData {
            guest: trigger.subject,
        });
    }
}

pub(crate) fn adjust_guest_survey_data(
    mut requests: MessageReader<AdjustGuestSurveyData>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    survey: Option<ResMut<GuestSurvey>>,
    guests: Query<(
        &GuestArchetype,
        &GuestCriticalNeedObservations,
        &GuestSurveyExperience,
        Option<&GuestFoodPurchaseCounts>,
    )>,
) {
    let Some(mut survey) = survey else {
        return;
    };
    let Some(definitions) = active.get(&definitions) else {
        return;
    };
    let Some(policy) = definitions
        .guest_generation()
        .map(|policy| policy.need_survey)
    else {
        return;
    };
    for request in requests.read() {
        let Ok((archetype, counters, experience, food)) = guests.get(request.guest) else {
            continue;
        };
        if definitions.find_guest(archetype.0).is_none() {
            continue;
        }
        let ratings = super::guest_survey_scoring::score_guest_survey(
            counters,
            *experience,
            food.unwrap_or(&GuestFoodPurchaseCounts::default()),
            policy,
        );
        for (rating, value) in [
            GuestSurveyRating::Recommendation,
            GuestSurveyRating::Happiness,
            GuestSurveyRating::Entertainment,
            GuestSurveyRating::Education,
            GuestSurveyRating::Food,
            GuestSurveyRating::Drink,
            GuestSurveyRating::Dessert,
            GuestSurveyRating::FavoriteFood,
            GuestSurveyRating::Donation,
            GuestSurveyRating::Gift,
            GuestSurveyRating::Bathroom,
        ]
        .into_iter()
        .zip(ratings)
        {
            survey.record_rating(rating, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_survey_trigger_records_each_channel_once_before_the_next_action() {
        let mut app = App::new();
        app.init_resource::<Assets<WorldDefinitionAsset>>()
            .insert_resource(GuestSurvey {
                critical_need_countdown_seconds: 5.0,
                previous_day_of_month: 9,
                current_day_of_month: 1,
                ..Default::default()
            })
            .add_message::<BehaviorSubjectTrigger>()
            .add_message::<AdjustGuestSurveyData>()
            .add_systems(
                Update,
                (route_guest_survey_triggers, adjust_guest_survey_data).chain(),
            );
        let document = super::super::viewing_reaction_composition_tests::policy_document();
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldDefinitionAsset>>()
            .add(WorldDefinitionAsset::from_test_document(document.clone()));
        let mut active = WorldDefinitions::default();
        active.index_test_document(handle, &document);
        app.insert_resource(active);
        let guest = app
            .world_mut()
            .spawn((
                GuestArchetype(AssetId::from_key("viewing-test")),
                GuestCriticalNeedObservations::default(),
                GuestSurveyExperience {
                    happiness_sum: 100.0,
                    happiness_samples: 2,
                    entertainment_sum: 25.0,
                    entertainment_views: 1,
                    education_points: 25.0,
                    saw_favorite_animal: true,
                    ..Default::default()
                },
            ))
            .id();
        app.world_mut().write_message(BehaviorSubjectTrigger {
            subject: guest,
            trigger: AssetId::from_key("adjust_surveydata"),
        });
        app.update();
        let survey = app.world().resource::<GuestSurvey>();
        assert!(survey
            .ratings
            .iter()
            .all(|history| history.daily_counts[0] == 1 && history.cumulative_count == 0));
        assert_eq!(
            survey.ratings[GuestSurveyRating::Recommendation as usize].daily_ratings[0],
            100.0
        );
        assert_eq!(
            survey.ratings[GuestSurveyRating::Happiness as usize].daily_ratings[0],
            50.0
        );
        assert_eq!(survey.critical_need_countdown_seconds, 5.0);
        app.world_mut().entity_mut(guest).despawn();
        app.update();
        assert!(app
            .world()
            .resource::<GuestSurvey>()
            .ratings
            .iter()
            .all(|history| history.daily_counts[0] == 1));
    }
}
