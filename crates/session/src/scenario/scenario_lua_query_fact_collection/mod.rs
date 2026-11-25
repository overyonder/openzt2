use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use openzt2_zoo_state::economy::guest_admission_types::AdmissionPrice;
use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory;
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;
use openzt2_interface_state::information::entity_selection_types::WorldUiSelection;
use openzt2_zoo_state::progression::fame_types::Fame;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooCalendar;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::simulation_time::zoo_calendar_calculation::days_in_month;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;

use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use super::scenario_lua_types::{
    ScenarioLuaCurrentPhotoChallengeSetQuery, ScenarioLuaLiveEntityQuery,
    ScenarioLuaPhotoChallengeQuery, ScenarioLuaQueryFacts,
};

/// The live game state that scenario Lua queries read.
#[derive(SystemParam)]
pub(super) struct ScenarioLuaQueryFactSources<'w, 's> {
    donation_totals: super::scenario_lua_types::ScenarioLuaDonationTotalsQuery<'w, 's>,
    localization: Res<'w, openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex>,
    localization_assets: Res<'w, Assets<openzt2_game_assets::localization::localization_asset_types::LocalizationAsset>>,
    species: Res<'w, openzt2_game_assets::species::species_asset_types::SpeciesAssets>,
    species_assets:
        Res<'w, Assets<openzt2_game_assets::species::species_asset_types::SpeciesAsset>>,
    tanks: super::scenario_lua_types::ScenarioLuaTankQuery<'w, 's>,
    selected_worlds: Query<
        'w,
        's,
        (Entity, &'static openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity, Option<&'static Fame>),
        With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>,
    >,
    zoo_state: Query<'w, 's, (
        Option<&'static ZooCash>, Option<&'static AdmissionPrice>, Option<&'static MonthlyFinanceHistory>,
        Option<&'static ZooClock>, Option<&'static ZooCalendar>,
    ), With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>>,
    world_definitions: Res<'w, WorldDefinitions>,
    world_definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    selected_entity: WorldUiSelection<'w, 's>,
    persistent_ids: Res<'w, openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndex>,
    contained_crated_animals: super::scenario_lua_types::ScenarioLuaCrateQuery<'w, 's>,
    containers: openzt2_zoo_state::animal_behavior::interaction_container_membership::InteractionContainerMemberships<'w, 's>,
    live_world_entities: ScenarioLuaLiveEntityQuery<'w, 's>,
    scenario_document_assets: Res<'w, Assets<WorldScenarioDocumentAsset>>,
    current_photo_challenge_sets: ScenarioLuaCurrentPhotoChallengeSetQuery<'w, 's>,
    photo_challenges: ScenarioLuaPhotoChallengeQuery<'w, 's>,
    fence_edges: super::scenario_lua_types::ScenarioLuaFenceEdgeQuery<'w, 's>,
    path_tiles: super::scenario_lua_types::ScenarioLuaPathTileQuery<'w, 's>,
    biome_cells: Query<'w, 's, &'static super::scenario_biome_cell_counts::WorldBiomeCellCounts>,
    ui_nodes: super::scenario_lua_types::ScenarioLuaUiNodeQuery<'w, 's>,
    ui_roots: super::scenario_lua_types::ScenarioLuaUiRootQuery<'w, 's>,
    ui_documents: Res<'w, Assets<openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset>>,
    windows: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
    real_time: Res<'w, Time<Real>>,
    construction_tool: Option<Res<'w, openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool>>,
    overhead_cameras: Query<
        'w,
        's,
        &'static openzt2_interface_state::camera::camera_runtime_state_types::OverheadRig,
        With<openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera>,
    >,
}

impl ScenarioLuaQueryFactSources<'_, '_> {
    pub(super) fn contained_entities(&self, container: Entity) -> Vec<Entity> {
        self.containers.members_of_container(container).collect()
    }

    pub(super) fn zoo_clock(&self, root: Entity) -> Option<&ZooClock> {
        self.zoo_state
            .get(root)
            .ok()
            .and_then(|(_, _, _, clock, _)| clock)
    }

    pub(super) fn borrow_scenario_lua_query_facts<'a>(
        &'a self,
        contained_entities: &'a dyn Fn(Entity) -> Vec<Entity>,
    ) -> ScenarioLuaQueryFacts<'a> {
        let live_world_entities: &'a ScenarioLuaLiveEntityQuery<'a, 'a> = &self.live_world_entities;
        let current_photo_challenge_sets: &'a ScenarioLuaCurrentPhotoChallengeSetQuery<'a, 'a> =
            &self.current_photo_challenge_sets;
        let photo_challenges: &'a ScenarioLuaPhotoChallengeQuery<'a, 'a> = &self.photo_challenges;
        let path_tiles: &'a super::scenario_lua_types::ScenarioLuaPathTileQuery<'a, 'a> =
            &self.path_tiles;
        let fence_edges: &'a super::scenario_lua_types::ScenarioLuaFenceEdgeQuery<'a, 'a> =
            &self.fence_edges;
        let tanks: &'a super::scenario_lua_types::ScenarioLuaTankQuery<'a, 'a> = &self.tanks;
        let ui_nodes: &'a super::scenario_lua_types::ScenarioLuaUiNodeQuery<'a, 'a> =
            &self.ui_nodes;
        let ui_roots: &'a super::scenario_lua_types::ScenarioLuaUiRootQuery<'a, 'a> =
            &self.ui_roots;
        let selected = self.selected_worlds.single().ok();
        let (cash, admission_price, history, clock, calendar) = selected
            .and_then(|(root, ..)| self.zoo_state.get(root).ok())
            .unwrap_or_default();
        ScenarioLuaQueryFacts {
            donation_totals: &self.donation_totals,
            localization: self
                .localization
                .borrow_loaded_localization_view(&self.localization_assets),
            spawn_animal: None,
            species: self.species.get(&self.species_assets),
            profile: None,
            zoo_cash: cash.copied().unwrap_or_default(),
            tank_in_world: !self.tanks.is_empty(),
            tanks,
            world_session_mode: selected.map(|(_, identity, _)| identity.mode),
            guest_admission_price: admission_price.copied().unwrap_or_default(),
            finance_history: history,
            zoo_fame: selected.and_then(|(_, _, fame)| fame),
            zoo_clock: clock,
            zoo_calendar: calendar,
            zoo_months_since_calendar_epoch: calendar
                .and_then(|calendar| self.count_zoo_months_since_calendar_epoch(calendar)),
            zoo_month_progress: clock
                .zip(calendar)
                .and_then(|(clock, calendar)| self.measure_zoo_month_progress(clock, calendar)),
            world: selected.map(|(root, ..)| root),
            persistent_ids: &self.persistent_ids,
            contained_crated_animals: &self.contained_crated_animals,
            contained_entities,
            waste: None,
            live_world_entities,
            world_definitions: self.world_definitions.get(&self.world_definition_assets),
            scenario_document_assets: &self.scenario_document_assets,
            current_photo_challenge_sets,
            selected_photo_set: None,
            photo_challenges,
            selected_entity: self.selected_entity.subject(),
            fence_edges,
            path_tiles,
            biome_cells: selected.and_then(|(root, ..)| self.biome_cells.get(root).ok()),
            ui_nodes,
            ui_roots,
            ui_documents: &self.ui_documents,
            ui_canvas_scale: self.windows.single().map_or(Vec2::ONE, |window| {
                Vec2::new(
                    window.physical_width() as f32,
                    window.physical_height() as f32,
                ) / Vec2::new(1024.0, 768.0)
            }),
            real_seconds: self.real_time.elapsed_secs_f64(),
            overhead_camera: self.overhead_cameras.iter().next(),
            construction_tool: self.construction_tool.as_deref().copied(),
        }
    }

    /// The facts `checkchallenges` reads when it decides whether to give an
    /// instant challenge.
    pub(super) fn read_instant_challenge_eligibility(
        &self,
        root: Entity,
    ) -> Option<ScenarioInstantChallengeEligibility> {
        use openzt2_zoo_state::economy::monthly_finance_types::FinanceMetric;
        use openzt2_zoo_state::economy::monthly_finance_types::FinancePeriod;

        let (_, _, Some(history), Some(clock), Some(calendar)) = self.zoo_state.get(root).ok()?
        else {
            return None;
        };
        let months_since_opening = self.count_zoo_months_since_calendar_epoch(calendar)?;
        let month_progress = self.measure_zoo_month_progress(clock, calendar)?;
        Some(ScenarioInstantChallengeEligibility {
            admitted_guests: if history.has_initialized_month() {
                history.finance_metric_value(FinanceMetric::TotalUsers, FinancePeriod::Lifetime)
            } else {
                0
            },
            months_since_opening,
            month_progress,
        })
    }

    /// How far the zoo is through its current calendar month.
    fn measure_zoo_month_progress(
        &self,
        clock: &ZooClock,
        calendar: &ZooCalendar,
    ) -> Option<ScenarioZooMonthProgress> {
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
    fn count_zoo_months_since_calendar_epoch(&self, calendar: &ZooCalendar) -> Option<i64> {
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
