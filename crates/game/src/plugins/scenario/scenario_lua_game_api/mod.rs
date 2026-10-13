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
    shown_rules: &'env RefCell<Vec<super::scenario_objective_types::ShowScenarioRuleRequest>>,
    available_content: &'env RefCell<Vec<openzt2_game_data::AssetId>>,
    requested_panel: &'env RefCell<
        Option<super::challenge_offer_types::ScenarioChallengePanelRequest>,
    >,
    current_photo_challenge_set: &'env RefCell<Option<openzt2_game_data::AssetId>>,
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
        // Shipped scripts compare this with months elapsed since the zoo
        // opened: `checkchallenges` treats any value above zero as an
        // established zoo, and `checkTimeLimit` adds months without wrapping.
        scope.create_function(move |_, ()| Ok(queries.zoo_months_since_calendar_epoch))?,
    )?;
    globals.set(
        "getCurrentTimeOfDay",
        // The shipped helper returns `ZT_GET_CURRENT_DAYOFMONTH`, the fraction
        // of the current month elapsed.
        scope.create_function(move |_, ()| {
            Ok(queries
                .zoo_month_progress
                .map(|progress| progress.elapsed_fraction()))
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
    install_scenario_rule_visibility_command(scope, &scenario_manager, scenario, shown_rules)?;
    scenario_manager.set(
        "BFS_ADD_SCENARIO_OBJECT",
        scope.create_function(move |_, (_, definition): (Table, String)| {
            if definition.trim().is_empty() {
                return Err(mlua::Error::runtime(
                    "BFS_ADD_SCENARIO_OBJECT requires a definition name",
                ));
            }
            available_content
                .borrow_mut()
                .push(openzt2_game_data::AssetId::from_key(&definition));
            Ok(())
        })?,
    )?;
    let economy_manager = lua.create_table()?;
    economy_manager.set(
        "ZT_GET_INFO",
        scope.create_function(move |_, (_, request): (Table, Table)| {
            use crate::plugins::economy::monthly_finance_types::{FinanceMetric, FinancePeriod};

            if request.get::<String>("_type")? != "ZTEconomyInfo"
                || request.get::<String>("period")? != "lifetime"
                || request.get::<String>("category")? != "admissions"
                || request.get::<String>("type")? != "totalUsers"
            {
                return Err(mlua::Error::runtime(
                    "ZT_GET_INFO only exposes lifetime admissions totalUsers",
                ));
            }
            if !queries.finance_history.has_initialized_month() {
                return Err(mlua::Error::runtime("finance history is not initialized"));
            }
            Ok(queries
                .finance_history
                .finance_metric_value(FinanceMetric::TotalUsers, FinancePeriod::Lifetime))
        })?,
    )?;
    let ui_root =
        super::challenge_lua_ui::create_challenge_ui_root(lua, scope, scenario, requested_panel)?;
    let photo_challenges =
        super::scenario_lua_photo_challenge_queries::create_scenario_lua_photo_challenge_component(
            lua,
            scope,
            queries,
            current_photo_challenge_set,
        )?;
    let entity_manager = super::scenario_lua_entity_queries::create_scenario_lua_entity_manager(
        lua, scope, queries,
    )?;
    globals.set(
        "queryObject",
        scope.create_function(move |_, name: String| match name.as_str() {
            "BFScenarioMgr" => Ok(Value::Table(scenario_manager.clone())),
            "ZTEconomyMgr" => Ok(Value::Table(economy_manager.clone())),
            "BFGManager" => Ok(Value::Table(entity_manager.clone())),
            "UIRoot" => Ok(Value::Table(ui_root.clone())),
            "ZTPhotoChallengesComponent" => Ok(Value::Table(photo_challenges.clone())),
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

fn install_scenario_rule_visibility_command<'scope, 'env: 'scope>(
    scope: &'scope Scope<'scope, 'env>,
    scenario_manager: &Table,
    scenario: openzt2_game_data::AssetId,
    shown_rules: &'env RefCell<Vec<super::scenario_objective_types::ShowScenarioRuleRequest>>,
) -> mlua::Result<()> {
    scenario_manager.set(
        "BFS_SHOWRULE",
        scope.create_function(move |_, (_, name): (Table, String)| {
            shown_rules.borrow_mut().push(
                super::scenario_objective_types::ShowScenarioRuleRequest {
                    scenario,
                    rule: openzt2_game_data::AssetId::from_key(&name),
                },
            );
            Ok(())
        })?,
    )
}
