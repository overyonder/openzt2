use std::collections::{BTreeMap, BTreeSet};

use mlua::Lua;

use crate::assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::plugins::economy::guest_admission_types::AdmissionPrice;
use crate::plugins::economy::scenario_economy_command_types::ScenarioEconomyOperation;
use crate::plugins::economy::zoo_cash_types::ZooCash;
use crate::plugins::progression::fame_types::Fame;
use crate::plugins::simulation_time::simulation_clock_types::ZooCalendar;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;

pub(super) struct ScenarioLuaScriptResult {
    pub(super) condition_status: super::scenario_objective_types::ScenarioObjectiveStatus,
    pub(super) economy_operations: Vec<ScenarioEconomyOperation>,
    pub(super) award_point_adjustments: Vec<i32>,
    pub(super) challenge_offer_source_paths: Vec<String>,
    pub(super) challenge_panel: Option<super::challenge_offer_types::ScenarioChallengePanelRequest>,
}

#[derive(Clone, Copy)]
pub(super) struct ScenarioLuaQueryFacts<'a> {
    pub(super) zoo_cash: &'a ZooCash,
    pub(super) guest_admission_price: &'a AdmissionPrice,
    pub(super) zoo_fame: &'a Fame,
    pub(super) zoo_clock: Option<&'a ZooClock>,
    pub(super) zoo_calendar: Option<&'a ZooCalendar>,
    pub(super) live_world_entity_counts_by_definition_identifier:
        &'a BTreeMap<openzt2_game_data::AssetId, u32>,
}

#[derive(Default)]
pub(super) struct ScenarioLuaVirtualMachineContexts {
    pub(super) virtual_machines_by_scenario_identifier:
        BTreeMap<openzt2_game_data::AssetId, ScenarioLuaVirtualMachine>,
}

pub(super) struct ScenarioLuaVirtualMachine {
    pub(super) lua_virtual_machine: Lua,
    pub(super) scenario_document_asset_identifier: bevy::asset::AssetId<WorldScenarioDocumentAsset>,
    pub(super) loaded_lua_script_asset_identifiers: BTreeSet<bevy::asset::AssetId<LuaScriptAsset>>,
}
