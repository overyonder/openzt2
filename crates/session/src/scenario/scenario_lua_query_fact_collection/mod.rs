use std::collections::BTreeMap;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use openzt2_zoo_state::economy::guest_admission_types::AdmissionPrice;
use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory;
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;
use openzt2_interface_state::information::entity_selection_types::SelectedEntity;
use openzt2_zoo_state::progression::fame_types::Fame;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooCalendar;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::simulation_time::zoo_calendar_calculation::days_in_month;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;

use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use super::scenario_lua_types::{
    ScenarioLuaCurrentPhotoChallengeSetQuery, ScenarioLuaLiveAnimalQuery,
    ScenarioLuaPhotoChallengeQuery, ScenarioLuaQueryFacts,
};

/// The live game state that scenario Lua queries read.
#[derive(SystemParam)]
pub(super) struct ScenarioLuaQueryFactSources<'w, 's> {
    zoo_cash: Res<'w, ZooCash>,
    guest_admission_price: Res<'w, AdmissionPrice>,
    finance_history: Res<'w, MonthlyFinanceHistory>,
    zoo_fame: Res<'w, Fame>,
    zoo_clock: Option<Res<'w, ZooClock>>,
    zoo_calendar: Option<Res<'w, ZooCalendar>>,
    world_definitions: Res<'w, WorldDefinitions>,
    world_definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    selected_entity: Option<Res<'w, SelectedEntity>>,
    definitions: Query<'w, 's, &'static DefinitionId>,
    live_world_animals: ScenarioLuaLiveAnimalQuery<'w, 's>,
    scenario_document_assets: Res<'w, Assets<WorldScenarioDocumentAsset>>,
    current_photo_challenge_sets: ScenarioLuaCurrentPhotoChallengeSetQuery<'w, 's>,
    photo_challenges: ScenarioLuaPhotoChallengeQuery<'w, 's>,
}

impl ScenarioLuaQueryFactSources<'_, '_> {
    pub(super) fn zoo_clock(&self) -> Option<&ZooClock> {
        self.zoo_clock.as_deref()
    }

    pub(super) fn count_live_world_entities_by_definition(
        &self,
    ) -> BTreeMap<openzt2_game_data::AssetId, u32> {
        self.definitions
            .iter()
            .fold(BTreeMap::new(), |mut counts, id| {
                *counts.entry(id.0).or_default() += 1;
                counts
            })
    }

    pub(super) fn borrow_scenario_lua_query_facts<'a>(
        &'a self,
        live_world_entity_counts_by_definition_identifier: &'a BTreeMap<
            openzt2_game_data::AssetId,
            u32,
        >,
    ) -> ScenarioLuaQueryFacts<'a> {
        let live_world_animals: &'a ScenarioLuaLiveAnimalQuery<'a, 'a> = &self.live_world_animals;
        let current_photo_challenge_sets: &'a ScenarioLuaCurrentPhotoChallengeSetQuery<'a, 'a> =
            &self.current_photo_challenge_sets;
        let photo_challenges: &'a ScenarioLuaPhotoChallengeQuery<'a, 'a> = &self.photo_challenges;
        ScenarioLuaQueryFacts {
            zoo_cash: &self.zoo_cash,
            guest_admission_price: &self.guest_admission_price,
            finance_history: &self.finance_history,
            zoo_fame: &self.zoo_fame,
            zoo_clock: self.zoo_clock.as_deref(),
            zoo_calendar: self.zoo_calendar.as_deref(),
            zoo_months_since_calendar_epoch: self.count_zoo_months_since_calendar_epoch(),
            zoo_month_progress: self.measure_zoo_month_progress(),
            live_world_entity_counts_by_definition_identifier,
            live_world_animals,
            scenario_document_assets: &self.scenario_document_assets,
            current_photo_challenge_sets,
            photo_challenges,
            selected_entity: self
                .selected_entity
                .as_deref()
                .and_then(|selected| selected.0),
        }
    }

    /// The facts `checkchallenges` reads when it decides whether to give an
    /// instant challenge.
    pub(super) fn read_instant_challenge_eligibility(
        &self,
    ) -> Option<ScenarioInstantChallengeEligibility> {
        use openzt2_zoo_state::economy::monthly_finance_types::FinanceMetric;
        use openzt2_zoo_state::economy::monthly_finance_types::FinancePeriod;

        let months_since_opening = self.count_zoo_months_since_calendar_epoch()?;
        let month_progress = self.measure_zoo_month_progress()?;
        Some(ScenarioInstantChallengeEligibility {
            admitted_guests: if self.finance_history.has_initialized_month() {
                self.finance_history
                    .finance_metric_value(FinanceMetric::TotalUsers, FinancePeriod::Lifetime)
            } else {
                0
            },
            months_since_opening,
            month_progress,
        })
    }

    /// How far the zoo is through its current calendar month.
    fn measure_zoo_month_progress(&self) -> Option<ScenarioZooMonthProgress> {
        let clock = self.zoo_clock.as_deref()?;
        let calendar = self.zoo_calendar.as_deref()?;
        let timing = self
            .world_definitions
            .get(&self.world_definition_assets)?
            .timing();
        let ticks_per_day = u64::from(timing.ticks_per_day);
        Some(ScenarioZooMonthProgress {
            ticks_into_month: u64::from(calendar.day.saturating_sub(1)) * ticks_per_day
                + u64::from(clock.tick_in_day),
            ticks_in_month: u64::from(days_in_month(
                &timing.calendar,
                u32::from(calendar.year),
                calendar.month,
            )) * ticks_per_day,
        })
    }

    /// Months the zoo calendar has advanced past its epoch, the zoo's first day.
    fn count_zoo_months_since_calendar_epoch(&self) -> Option<i64> {
        let calendar = self.zoo_calendar.as_deref()?;
        let policy = &self
            .world_definitions
            .get(&self.world_definition_assets)?
            .timing()
            .calendar;
        Some(
            (i64::from(calendar.year) - i64::from(policy.epoch_year)) * 12
                + i64::from(calendar.month)
                - i64::from(policy.epoch_month),
        )
    }
}

/// Zoo time elapsed in, and the length of, the current calendar month.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ScenarioZooMonthProgress {
    pub(super) ticks_into_month: u64,
    pub(super) ticks_in_month: u64,
}

impl ScenarioZooMonthProgress {
    /// The fraction of the month elapsed, in `0.0..1.0`. This is what the
    /// shipped `getCurrentTimeOfDay` returns from `ZT_GET_CURRENT_DAYOFMONTH`:
    /// `decidechallengeset` measures an interval that crosses a month as
    /// `(1 - start) + now`, which holds only for a value that wraps at 1.
    pub(super) fn elapsed_fraction(self) -> f64 {
        self.ticks_into_month as f64 / self.ticks_in_month.max(1) as f64
    }
}

/// Inputs to the instant-challenge condition in `checkchallenges`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ScenarioInstantChallengeEligibility {
    /// Lifetime admissions, as `getNumGuests(-1)` reads them.
    pub(super) admitted_guests: i64,
    /// `getCurrentMonth()`.
    pub(super) months_since_opening: i64,
    /// `getCurrentTimeOfDay()`, which must not exceed 0.1.
    pub(super) month_progress: ScenarioZooMonthProgress,
}

impl ScenarioInstantChallengeEligibility {
    /// The zoo has admitted no guest and is within the first tenth of its
    /// first month.
    pub(super) const fn allows_instant_challenge(self) -> bool {
        self.admitted_guests == 0
            && self.months_since_opening == 0
            && self.month_progress.ticks_into_month.saturating_mul(10)
                <= self.month_progress.ticks_in_month
    }
}
