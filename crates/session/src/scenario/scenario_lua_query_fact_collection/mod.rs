use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory;
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
        Option<&'static MonthlyFinanceHistory>,
        Option<&'static ZooClock>, Option<&'static ZooCalendar>,
    ), With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>>,
    world_definitions: Res<'w, WorldDefinitions>,
    world_definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    selected_entity: WorldUiSelection<'w, 's>,
    persistent_ids: Res<'w, openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndex>,
    contained_crated_animals: super::scenario_lua_types::ScenarioLuaCrateQuery<'w, 's>,
    containers: openzt2_zoo_state::animal_behavior::interaction_container_membership::InteractionContainerMemberships<'w, 's>,
    authored_float_attributes: Query<'w, 's, &'static openzt2_zoo_state::authored_float_attributes::AuthoredFloatAttributes>,
    live_world_entities: ScenarioLuaLiveEntityQuery<'w, 's>,
    scenario_document_assets: Res<'w, Assets<WorldScenarioDocumentAsset>>,
    current_photo_challenge_sets: ScenarioLuaCurrentPhotoChallengeSetQuery<'w, 's>,
    photo_catalogues: Query<'w, 's, &'static openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeCatalogueIndex>,
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
    guest_view_sessions: Query<
        'w,
        's,
        (
            &'static openzt2_interface_state::immersive_modes::immersive_mode_state_types::GuestViewTileCount,
            &'static openzt2_interface_state::immersive_modes::immersive_mode_state_types::ImmersiveCamera,
        ),
        With<openzt2_interface_state::immersive_modes::immersive_mode_state_types::ActiveImmersiveMode>,
    >,
    guest_view_cameras: Query<
        'w,
        's,
        &'static openzt2_world_state::world_spawn::world_membership_types::WorldMember,
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
            .and_then(|(_, clock, _)| clock)
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
        let (history, clock, calendar) = selected
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
            world_variable: None,
            economy: None,
            tank_in_world: !self.tanks.is_empty(),
            tanks,
            world_session_mode: selected.map(|(_, identity, _)| identity.mode),
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
            authored_float_attributes: &self.authored_float_attributes,
            live_world_entities,
            world_definitions: self.world_definitions.get(&self.world_definition_assets),
            scenario_document_assets: &self.scenario_document_assets,
            current_photo_challenge_sets,
            photo_set: None,
            photo_catalogue: selected.and_then(|(root, ..)| self.photo_catalogues.get(root).ok()),
            photo_challenges,
            selected_entity: self.selected_entity.subject(),
            fence_edges,
            path_tiles,
            biome_cells: selected.and_then(|(root, ..)| self.biome_cells.get(root).ok()),
            ui_receiver: None,
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
            guest_view_tile_count: selected
                .and_then(|(root, ..)| {
                    self.guest_view_sessions.iter().find_map(|(count, camera)| {
                        (self.guest_view_cameras.get(camera.0).ok()?.root == root)
                            .then_some(count.0)
                    })
                })
                .unwrap_or_default(),
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

        let (Some(history), Some(clock), Some(calendar)) = self.zoo_state.get(root).ok()? else {
            return None;
        };
        let months_since_opening = self.count_zoo_months_since_calendar_epoch(calendar)?;
        let month_progress = self.measure_zoo_month_progress(clock, calendar)?;
        Some(ScenarioInstantChallengeEligibility {
            admitted_guests: if history.has_initialized_month() {
                match history
                    .finance_metric_value(FinanceMetric::TotalUsers, FinancePeriod::Lifetime)
                {
                    openzt2_zoo_state::economy::monthly_finance_types::FinanceValue::Users(
                        users,
                    ) => users,
                    _ => 0,
                }
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
        let definitions = self.world_definitions.get(&self.world_definition_assets)?;
        let cadence = openzt2_simulation_time::simulation_time_world_lifecycle::
            zoo_day_cadence_from_world_definitions(definitions)?;
        let day_fraction = cadence.day_fraction(*clock)?;
        let days = days_in_month(
            &definitions.timing().calendar,
            u32::from(calendar.year),
            calendar.month,
        );
        Some(ScenarioZooMonthProgress(
            (f64::from(calendar.day.saturating_sub(1)) + day_fraction) / f64::from(days.max(1)),
        ))
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ScenarioZooMonthProgress(f64);

impl ScenarioZooMonthProgress {
    pub(super) fn elapsed_fraction(self) -> f64 {
        self.0
    }
}

/// Inputs to the instant-challenge condition in `checkchallenges`.
#[derive(Debug, Clone, Copy, PartialEq)]
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
        self.admitted_guests == 0 && self.months_since_opening == 0 && self.month_progress.0 <= 0.1
    }
}
