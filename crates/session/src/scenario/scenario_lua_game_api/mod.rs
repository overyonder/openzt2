use std::cell::RefCell;

use mlua::{Lua, MultiValue, Scope, Table, Value};

use openzt2_game_assets::lua_script::lua_value_conversion::convert_integral_lua_number_to_i64;

use super::scenario_lua_types::ScenarioLuaQueryFacts;

pub(super) fn install_scenario_game_query_and_command_functions<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    queries: ScenarioLuaQueryFacts<'env>,
    economy: &'env RefCell<
        Vec<openzt2_zoo_state::economy::scenario_economy_command_types::ScenarioEconomyOperation>,
    >,
    award_points: &'env RefCell<Vec<i32>>,
    challenge_offers: &'env RefCell<Vec<String>>,
    scenario: openzt2_game_data::AssetId,
    shown_rules: &'env RefCell<
        Vec<openzt2_zoo_state::scenario::scenario_objective_types::ShowScenarioRuleRequest>,
    >,
    available_content: &'env RefCell<Vec<openzt2_game_data::AssetId>>,
    requested_panel: &'env RefCell<
        Option<super::challenge_offer_types::ScenarioChallengePanelRequest>,
    >,
    current_photo_challenge_set: &'env RefCell<Option<openzt2_game_data::AssetId>>,
    profile_variables: &'env RefCell<
        Vec<openzt2_zoo_state::progression::profile_scenario_variables::SetProfileScenarioVariable>,
    >,
    persist_profile_variables: &'env std::cell::Cell<bool>,
    ui: &'env RefCell<Vec<super::scenario_lua_ui::ScenarioUiCommand>>,
    entity_operations: &'env super::scenario_lua_entity_commands::PendingEntityOperations,
) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set(
        "getuservar",
        scope.create_function(move |_, name: String| {
            let key = openzt2_game_data::AssetId::from_key(&name);
            let pending = profile_variables.borrow();
            Ok(pending
                .iter()
                .rev()
                .find(|change| change.key == key)
                .map_or_else(
                    || queries.profile_variables.get(key).map(str::to_owned),
                    |change| change.value.clone(),
                ))
        })?,
    )?;
    globals.set("setuservar", scope.create_function(move |_, (name, value): (String, Option<String>)| {
        if name.is_empty() || value.as_ref().is_some_and(|value| value.len() > 1024) {
            return Err(mlua::Error::runtime("invalid profile scenario variable"));
        }
        profile_variables.borrow_mut().push(openzt2_zoo_state::progression::profile_scenario_variables::SetProfileScenarioVariable {
            key: openzt2_game_data::AssetId::from_key(&name), value,
        });
        Ok(())
    })?)?;
    globals.set(
        "saveOptions",
        scope.create_function(move |_, _: MultiValue| {
            persist_profile_variables.set(true);
            Ok(())
        })?,
    )?;
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
        scope.create_function(move |_, (name, value): (String, Value)| {
            if name == "nextScenario" {
                let next = match &value {
                    Value::String(value) => Some(value.to_str()?.to_owned()),
                    Value::Nil => None,
                    _ => return Err(mlua::Error::runtime("nextScenario must be a string or nil")),
                };
                ui.borrow_mut()
                    .push(super::scenario_lua_ui::ScenarioUiCommand::NextScenario(
                        next,
                    ));
            }
            state.set(name, value)
        })?,
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
        "BFS_GETLOCID",
        scope.create_function(move |_, (_, key): (Table, String)| {
            let canonical = openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(&key);
            queries.localization
                .and_then(|localization| localization.find_plain_localized_text(openzt2_game_data::AssetId::from_key(&canonical)))
                .map(str::to_owned)
                .ok_or_else(|| mlua::Error::runtime(format!("missing localization {key}")))
        })?,
    )?;
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
                .push(openzt2_game_data::AssetId::from_key(&openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(&definition)));
            Ok(())
        })?,
    )?;
    super::scenario_lua_ui::install_manager_commands(scope, &scenario_manager, ui)?;
    let economy_manager = lua.create_table()?;
    let status = lua.create_table()?;
    status.set(
        "ZT_GET_DONATIONBYANIMALTYPE",
        scope.create_function(move |_, (_, category): (Table, String)| {
            let category = openzt2_game_data::AssetId::from_key(&category);
            let cents = queries
                .donation_totals
                .iter()
                .try_fold(0_i64, |sum, totals| {
                    sum.checked_add(totals.aggregate_for_category(Some(category))?.amount.0)
                })
                .ok_or_else(|| mlua::Error::runtime("donation total exceeds currency range"))?;
            Ok(cents as f64 / 100.0)
        })?,
    )?;
    economy_manager.set(
        "ZT_GET_INFO",
        scope.create_function(move |_, (_, request): (Table, Table)| {
            use openzt2_zoo_state::economy::monthly_finance_types::FinanceMetric;
            use openzt2_zoo_state::economy::monthly_finance_types::FinancePeriod;

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
    let ui_root = super::scenario_lua_ui::extend_ui_root(lua, scope, ui_root, ui)?;
    let mode =
        super::scenario_lua_ui::create_mode_adapter(lua, scope, ui, queries, entity_operations)?;
    let photo_challenges =
        super::scenario_lua_photo_challenge_queries::create_scenario_lua_photo_challenge_component(
            lua,
            scope,
            queries,
            current_photo_challenge_set,
        )?;
    let entity_manager = super::scenario_lua_entity_queries::create_scenario_lua_entity_manager(
        lua,
        scope,
        queries,
        entity_operations,
    )?;
    let world_manager = super::scenario_lua_entity_queries::create_world_manager(
        lua,
        scope,
        queries,
        entity_operations,
    )?;
    globals.set(
        "queryObject",
        scope.create_function(move |_, name: String| match name.as_str() {
            "BFScenarioMgr" => Ok(Value::Table(scenario_manager.clone())),
            "ZTEconomyMgr" => Ok(Value::Table(economy_manager.clone())),
            "ZTStatus" => Ok(Value::Table(status.clone())),
            "BFGManager" => Ok(Value::Table(entity_manager.clone())),
            "ZTWorldMgr" => Ok(Value::Table(world_manager.clone())),
            "UIRoot" => Ok(Value::Table(ui_root.clone())),
            "ZTMode" => Ok(Value::Table(mode.clone())),
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

    use openzt2_zoo_state::economy::scenario_economy_command_types::ScenarioEconomyOperation;
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
) -> openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus {
    use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;
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
) -> mlua::Result<openzt2_zoo_state::economy::money_types::Money> {
    values
        .into_iter()
        .filter_map(convert_integral_lua_number_to_i64)
        .next_back()
        .and_then(|dollars| dollars.checked_mul(100))
        .map(openzt2_zoo_state::economy::money_types::Money)
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
    shown_rules: &'env RefCell<
        Vec<openzt2_zoo_state::scenario::scenario_objective_types::ShowScenarioRuleRequest>,
    >,
) -> mlua::Result<()> {
    scenario_manager.set(
        "BFS_SHOWRULE",
        scope.create_function(move |_, (_, name): (Table, String)| {
            shown_rules.borrow_mut().push(
                openzt2_zoo_state::scenario::scenario_objective_types::ShowScenarioRuleRequest {
                    scenario,
                    rule: openzt2_game_data::AssetId::from_key(&name),
                },
            );
            Ok(())
        })?,
    )
}
