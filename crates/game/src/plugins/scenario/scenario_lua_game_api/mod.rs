use std::cell::RefCell;

use mlua::{Lua, MultiValue, Scope, Table, Value};

use crate::plugins::lua_value_conversion::convert_integral_lua_number_to_i64;

use super::scenario_lua_types::ScenarioLuaQueryFacts;

pub(super) fn install_scenario_game_query_and_command_functions<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    queries: ScenarioLuaQueryFacts<'env>,
    economy: &'env RefCell<
        Vec<crate::plugins::economy::scenario_economy_command_types::ScenarioEconomyOperation>,
    >,
    award_points: &'env RefCell<Vec<i32>>,
    challenge_offers: &'env RefCell<Vec<String>>,
    scenario: openzt2_game_data::AssetId,
    requested_panel: &'env RefCell<
        Option<super::challenge_offer_types::ScenarioChallengePanelRequest>,
    >,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let state = globals
        .get::<Option<Table>>("__openzt2_scenario_state")?
        .unwrap_or(lua.create_table()?);
    globals.set("__openzt2_scenario_state", state.clone())?;
    globals.set(
        "getglobalvar",
        scope.create_function({
            let state = state.clone();
            move |_, name: String| state.get::<Value>(name)
        })?,
    )?;
    globals.set(
        "setglobalvar",
        scope.create_function(move |_, (name, value): (String, Value)| state.set(name, value))?,
    )?;
    globals.set(
        "howMuchCash",
        scope.create_function(move |_, ()| Ok(queries.zoo_cash.0 .0 / 100))?,
    )?;
    globals.set(
        "getAdmissionPrice",
        scope.create_function(move |_, ()| Ok(queries.guest_admission_price.0 .0 / 100))?,
    )?;
    globals.set(
        "getHalfStars",
        scope.create_function(move |_, ()| Ok(i64::from(queries.zoo_fame.half_stars)))?,
    )?;
    globals.set(
        "currenttimeofday",
        scope.create_function(move |_, ()| {
            Ok(queries
                .zoo_clock
                .map_or(0, |clock| u64::from(clock.tick_in_day)))
        })?,
    )?;
    globals.set(
        "current_dayofmonth",
        scope.create_function(move |_, ()| {
            Ok(queries.zoo_calendar.map_or(0, |calendar| calendar.day))
        })?,
    )?;
    globals.set(
        "getCurrentMonth",
        scope.create_function(move |_, ()| {
            Ok(queries.zoo_calendar.map_or(0, |calendar| calendar.month))
        })?,
    )?;
    globals.set(
        "getCurrentTimeOfDay",
        scope.create_function(move |_, ()| {
            Ok(queries
                .zoo_clock
                .map_or(0, |clock| u64::from(clock.tick_in_day)))
        })?,
    )?;
    globals.set(
        "countType",
        scope.create_function(move |_, kind: String| {
            Ok(queries
                .live_world_entity_counts_by_definition_identifier
                .get(&openzt2_game_data::AssetId::from_key(&kind))
                .copied()
                .unwrap_or(0))
        })?,
    )?;
    globals.set(
        "howManyInTableExist",
        scope.create_function(move |_, kinds: Table| {
            kinds
                .sequence_values::<String>()
                .try_fold(0_u32, |count, kind| {
                    kind.map(|kind| {
                        count
                            + u32::from(
                                queries
                                    .live_world_entity_counts_by_definition_identifier
                                    .get(&openzt2_game_data::AssetId::from_key(&kind))
                                    .copied()
                                    .unwrap_or(0)
                                    > 0,
                            )
                    })
                })
        })?,
    )?;
    globals.set(
        "BFS_ADDSCENARIO",
        scope.create_function_mut(move |_, values: MultiValue| {
            let path = values
                .into_iter()
                .filter_map(|value| match value {
                    Value::String(value) => value.to_str().ok().map(|value| value.to_owned()),
                    _ => None,
                })
                .next_back()
                .ok_or_else(|| {
                    mlua::Error::runtime("BFS_ADDSCENARIO requires a scenario source path")
                })?;
            challenge_offers.borrow_mut().push(path);
            Ok(())
        })?,
    )?;

    let scenario_manager = lua.create_table()?;
    scenario_manager.set(
        "BFS_ADDSCENARIO",
        globals.get::<mlua::Function>("BFS_ADDSCENARIO")?,
    )?;
    let ui_root =
        super::challenge_lua_ui::create_challenge_ui_root(lua, scope, scenario, requested_panel)?;
    globals.set(
        "queryObject",
        scope.create_function(move |lua, name: String| match name.as_str() {
            "BFScenarioMgr" => Ok(Value::Table(scenario_manager.clone())),
            "UIRoot" => Ok(Value::Table(ui_root.clone())),
            "ZTPhotoChallengesComponent" => Ok(Value::String(lua.create_string(&name)?)),
            _ => Err(mlua::Error::runtime(format!(
                "queryObject does not expose {name}"
            ))),
        })?,
    )?;
    globals.set(
        "BFLOG",
        scope.create_function(|_, values: MultiValue| {
            for value in values {
                if let Value::String(message) = value {
                    bevy::log::trace!(message = %message.to_string_lossy(), "scenario Lua log");
                }
            }
            Ok(())
        })?,
    )?;

    use crate::plugins::economy::scenario_economy_command_types::ScenarioEconomyOperation;
    globals.set(
        "giveCash",
        scope.create_function_mut(move |_, values: MultiValue| {
            economy
                .borrow_mut()
                .push(ScenarioEconomyOperation::GrantCash(
                    convert_lua_command_arguments_to_money(values)?,
                ));
            Ok(())
        })?,
    )?;
    globals.set(
        "takeCash",
        scope.create_function_mut(move |_, values: MultiValue| {
            economy
                .borrow_mut()
                .push(ScenarioEconomyOperation::TakeCash(
                    convert_lua_command_arguments_to_money(values)?,
                ));
            Ok(())
        })?,
    )?;
    globals.set(
        "setAdmissionPrice",
        scope.create_function_mut(move |_, values: MultiValue| {
            economy
                .borrow_mut()
                .push(ScenarioEconomyOperation::SetAdmission(
                    convert_lua_command_arguments_to_money(values)?,
                ));
            Ok(())
        })?,
    )?;
    globals.set(
        "incrementAwardPoints",
        scope.create_function_mut(move |_, values: MultiValue| {
            award_points
                .borrow_mut()
                .push(convert_lua_command_arguments_to_i32(values)?);
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(super) fn convert_lua_rule_result_to_objective_status(
    value: Value,
) -> super::scenario_objective_types::ScenarioObjectiveStatus {
    use super::scenario_objective_types::ScenarioObjectiveStatus;
    match value {
        Value::Integer(value) if value < 0 => ScenarioObjectiveStatus::Failed,
        Value::Number(value) if value < 0.0 => ScenarioObjectiveStatus::Failed,
        Value::Nil | Value::Boolean(false) | Value::Integer(0) => ScenarioObjectiveStatus::Active,
        Value::Number(0.0) => ScenarioObjectiveStatus::Active,
        _ => ScenarioObjectiveStatus::Satisfied,
    }
}

fn convert_lua_command_arguments_to_money(
    values: MultiValue,
) -> mlua::Result<crate::plugins::economy::money_types::Money> {
    values
        .into_iter()
        .filter_map(convert_integral_lua_number_to_i64)
        .next_back()
        .and_then(|dollars| dollars.checked_mul(100))
        .map(crate::plugins::economy::money_types::Money)
        .ok_or_else(|| mlua::Error::runtime("money command requires whole dollars"))
}

fn convert_lua_command_arguments_to_i32(values: MultiValue) -> mlua::Result<i32> {
    values
        .into_iter()
        .filter_map(convert_integral_lua_number_to_i64)
        .filter_map(|value| i32::try_from(value).ok())
        .next_back()
        .ok_or_else(|| mlua::Error::runtime("command requires a 32-bit integer"))
}
