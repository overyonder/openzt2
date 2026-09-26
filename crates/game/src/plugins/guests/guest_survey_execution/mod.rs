use super::guest_definition_queries::find_guest_definition;
use super::guest_definition_queries::guest_need_rows;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestDefinition;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestNeedDefinition;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestNeedSurveySignals;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use super::guest_simulation_calculations::survey_rate_permille;
use super::guest_simulation_types::AdjustGuestSurveyData;
use super::guest_simulation_types::Guest;
use super::guest_simulation_types::GuestArchetype;
use super::guest_simulation_types::GuestDessert;
use super::guest_simulation_types::GuestGift;
use super::guest_simulation_types::GuestHunger;
use super::guest_simulation_types::GuestRestroom;
use super::guest_simulation_types::GuestSurvey;
use super::guest_simulation_types::GuestThirst;

pub(super) fn initialize_guest_survey(
    roots: Query<(), With<WorldLoadCompleted>>,
    survey: Option<Res<GuestSurvey>>,
    mut commands: Commands,
) {
    if survey.is_none() && !roots.is_empty() {
        commands.insert_resource(GuestSurvey::default());
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn adjust_guest_survey_data(
    mut requests: MessageReader<AdjustGuestSurveyData>,
    clock: Res<ZooClock>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    survey: Option<ResMut<GuestSurvey>>,
    guests: Query<(
        &GuestArchetype,
        &GuestHunger,
        &GuestThirst,
        &GuestDessert,
        &GuestGift,
        &GuestRestroom,
    )>,
) {
    let Some(mut survey) = survey else { return };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(policy) = definitions
        .guest_generation()
        .map(|policy| &policy.need_survey)
    else {
        return;
    };
    reset_survey_month(&mut survey, clock.absolute_day / 30);
    let monitored = policy.monitored;
    let hits = requests
        .read()
        .filter_map(|request| guests.get(request.guest).ok())
        .filter(|(archetype, hunger, thirst, dessert, gift, restroom)| {
            find_guest_definition(definitions, archetype.0).is_some_and(|(_, definition)| {
                guest_has_authored_critical_survey_need(
                    definition, monitored, hunger, thirst, dessert, gift, restroom,
                )
            })
        })
        .count() as u16;
    update_critical_survey_score(&mut survey, hits, policy.maximum_critical_hits_per_month);
}

#[allow(clippy::type_complexity)]

pub(crate) fn survey_guest_needs(
    clock: Res<ZooClock>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    survey: Option<ResMut<GuestSurvey>>,
    guests: Query<
        (
            &GuestArchetype,
            &GuestHunger,
            &GuestThirst,
            &GuestDessert,
            &GuestGift,
            &GuestRestroom,
        ),
        With<Guest>,
    >,
) {
    let Some(mut survey) = survey else { return };
    let Some(asset) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(policy) = asset.guest_generation() else {
        return;
    };
    if clock.tick < survey.next_tick {
        return;
    }
    let fixed_hz = u64::from(asset.timing().fixed_hz.max(1));
    let interval = policy
        .need_survey
        .check_interval_ns
        .saturating_mul(fixed_hz)
        .saturating_add(999_999_999)
        / 1_000_000_000;
    survey.next_tick = clock.tick.saturating_add(interval.max(1));
    reset_survey_month(&mut survey, clock.absolute_day / 30);
    let monitored = policy.need_survey.monitored;
    let hits = guests
        .iter()
        .filter(|(archetype, hunger, thirst, dessert, gift, restroom)| {
            find_guest_definition(asset, archetype.0).is_some_and(|(_, definition)| {
                guest_has_authored_critical_survey_need(
                    definition, monitored, hunger, thirst, dessert, gift, restroom,
                )
            })
        })
        .count() as u16;
    let maximum_critical_hits = policy.need_survey.maximum_critical_hits_per_month;
    update_critical_survey_score(&mut survey, hits, maximum_critical_hits);
}

fn reset_survey_month(survey: &mut GuestSurvey, month: u32) {
    if survey.month != month {
        survey.month = month;
        survey.critical_hits = 0;
        survey.education_points = 0;
        survey.entertainment_points = 0;
    }
}

fn update_critical_survey_score(survey: &mut GuestSurvey, hits: u16, maximum_critical_hits: u16) {
    survey.critical_hits = survey
        .critical_hits
        .saturating_add(hits)
        .min(maximum_critical_hits);
    survey.critical_need_rate_permille = survey_rate_permille(
        u32::from(maximum_critical_hits.saturating_sub(survey.critical_hits)),
        u32::from(maximum_critical_hits),
    );
}

fn guest_has_authored_critical_survey_need(
    definition: &GuestDefinition,
    monitored: GuestNeedSurveySignals,
    hunger: &GuestHunger,
    thirst: &GuestThirst,
    dessert: &GuestDessert,
    gift: &GuestGift,
    restroom: &GuestRestroom,
) -> bool {
    let Some(needs) = guest_need_rows(definition) else {
        return false;
    };
    (monitored.contains_all(GuestNeedSurveySignals::HUNGER)
        && guest_need_pressure_reached_authored_threshold(hunger.value, needs[0]))
        || (monitored.contains_all(GuestNeedSurveySignals::THIRST)
            && guest_need_pressure_reached_authored_threshold(thirst.value, needs[1]))
        || (monitored.contains_all(GuestNeedSurveySignals::DESSERT)
            && guest_need_pressure_reached_authored_threshold(dessert.value, needs[2]))
        || (monitored.contains_all(GuestNeedSurveySignals::GIFT)
            && guest_need_pressure_reached_authored_threshold(gift.value, needs[3]))
        || (monitored.contains_all(GuestNeedSurveySignals::BATHROOM)
            && guest_need_pressure_reached_authored_threshold(restroom.value, needs[5]))
}

fn guest_need_pressure_reached_authored_threshold(
    wellness_permille: u16,
    need: &GuestNeedDefinition,
) -> bool {
    1_000_u16.saturating_sub(wellness_permille) >= need.critical_threshold
}
