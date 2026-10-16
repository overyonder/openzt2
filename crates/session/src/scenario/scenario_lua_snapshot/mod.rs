use super::{
    scenario_lua_snapshot_types::{ScenarioLuaSavedState, ScenarioLuaSavedValue},
    scenario_lua_types::ScenarioLuaVirtualMachine,
};
use bevy::prelude::*;
use mlua::{Lua, Value};
use openzt2_game_assets::{
    lua_script::{
        lua_script_archive_path_index::LuaScriptArchivePathIndex,
        lua_script_asset_loading::LuaScriptAsset,
        lua_script_module_execution::{
            execute_lua_script_module_once_per_virtual_machine,
            install_lua_script_dependency_loading_functions,
        },
    },
    world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};
use std::{cell::RefCell, collections::BTreeSet};

const MAX_VALUES: usize = 65_536;
const MAX_DEPTH: usize = 32;
const MAX_BYTES: usize = 1_048_576;

fn standard_global_names() -> mlua::Result<BTreeSet<Vec<u8>>> {
    Lua::new()
        .globals()
        .pairs::<mlua::String, Value>()
        .map(|entry| entry.map(|(key, _)| key.as_bytes().to_vec()))
        .collect()
}

fn save_value(
    value: Value,
    depth: usize,
    values: &mut usize,
    tables: &mut BTreeSet<usize>,
) -> mlua::Result<ScenarioLuaSavedValue> {
    *values += 1;
    if *values > MAX_VALUES || depth > MAX_DEPTH {
        return Err(mlua::Error::runtime("scenario state capacity exceeded"));
    }
    Ok(match value {
        Value::Boolean(value) => ScenarioLuaSavedValue::Boolean(value),
        Value::Integer(value) => ScenarioLuaSavedValue::Integer(value),
        Value::Number(value) if value.is_finite() => ScenarioLuaSavedValue::Number(value),
        Value::String(value) if value.as_bytes().len() <= MAX_BYTES => {
            ScenarioLuaSavedValue::Bytes(value.as_bytes().to_vec())
        }
        Value::Table(table) => {
            if table.contains_key(super::scenario_lua_entity_queries::ENTITY_FIELD)?
                || table
                    .contains_key(super::scenario_lua_entity_queries::PERSISTENT_ENTITY_FIELD)?
            {
                let id = table
                    .get::<u64>(super::scenario_lua_entity_queries::PERSISTENT_ENTITY_FIELD)?;
                if matches!(id, 0 | u64::MAX) {
                    return Err(mlua::Error::runtime("invalid persistent scenario entity"));
                }
                return Ok(ScenarioLuaSavedValue::PersistentEntity(id));
            }
            if table.metatable().is_some() || !tables.insert(table.to_pointer() as usize) {
                return Err(mlua::Error::runtime(
                    "scenario data has shared, cyclic, or metatable-backed tables",
                ));
            }
            let mut entries = Vec::new();
            for entry in table.pairs::<Value, Value>() {
                let (key, value) = entry?;
                if !matches!(
                    key,
                    Value::String(_) | Value::Integer(_) | Value::Number(_) | Value::Boolean(_)
                ) {
                    return Err(mlua::Error::runtime("unsupported scenario data key"));
                }
                let saved_value =
                    save_value(value, depth + 1, values, tables).map_err(|error| {
                        mlua::Error::runtime(format!("scenario table entry {key:?}: {error}"))
                    })?;
                entries.push((save_value(key, depth + 1, values, tables)?, saved_value));
            }
            ScenarioLuaSavedValue::Table(entries)
        }
        value => {
            return Err(mlua::Error::runtime(format!(
                "unsupported scenario data value of type {}",
                value.type_name()
            )));
        }
    })
}

pub(super) fn capture_runtime(
    context: &ScenarioLuaVirtualMachine,
) -> mlua::Result<ScenarioLuaSavedState> {
    let standard = standard_global_names()?;
    let mut values = 0;
    let mut tables = BTreeSet::new();
    let mut globals = Vec::new();
    for entry in context
        .lua_virtual_machine
        .globals()
        .pairs::<mlua::String, Value>()
    {
        let (key, value) = entry?;
        let key = key.as_bytes().to_vec();
        // Lunify spills registers into globals while translating Lua 5.0 loops.
        // These are invocation scratch values, not authored scenario state.
        if standard.contains(&key)
            || key.starts_with(b"__%lunify%__temp")
            || matches!(value, Value::Function(_))
        {
            continue;
        }
        let saved_value = save_value(value, 0, &mut values, &mut tables).map_err(|error| {
            mlua::Error::runtime(format!(
                "scenario global {}: {error}",
                String::from_utf8_lossy(&key)
            ))
        })?;
        globals.push((key, saved_value));
    }
    globals.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(ScenarioLuaSavedState {
        modules: context.executed_source_paths.clone(),
        globals,
    })
}

fn restore_value(
    lua: &Lua,
    value: &ScenarioLuaSavedValue,
    depth: usize,
    count: &mut usize,
) -> mlua::Result<Value> {
    *count += 1;
    if *count > MAX_VALUES || depth > MAX_DEPTH {
        return Err(mlua::Error::runtime("scenario state capacity exceeded"));
    }
    Ok(match value {
        ScenarioLuaSavedValue::Boolean(value) => Value::Boolean(*value),
        ScenarioLuaSavedValue::Integer(value) => Value::Integer(*value),
        ScenarioLuaSavedValue::Number(value) if value.is_finite() => Value::Number(*value),
        ScenarioLuaSavedValue::Bytes(value) if value.len() <= MAX_BYTES => {
            Value::String(lua.create_string(value)?)
        }
        ScenarioLuaSavedValue::Table(entries) => {
            let table = lua.create_table()?;
            for (key, value) in entries {
                if matches!(key, ScenarioLuaSavedValue::Bytes(bytes) if bytes == super::scenario_lua_entity_queries::ENTITY_FIELD.as_bytes()
                    || bytes == super::scenario_lua_entity_queries::PERSISTENT_ENTITY_FIELD.as_bytes())
                {
                    return Err(mlua::Error::runtime(
                        "saved scenario data contains a live entity handle",
                    ));
                }
                if matches!(
                    key,
                    ScenarioLuaSavedValue::Table(_) | ScenarioLuaSavedValue::PersistentEntity(_)
                ) {
                    return Err(mlua::Error::runtime("unsupported scenario data key"));
                }
                let key = restore_value(lua, key, depth + 1, count)?;
                if !matches!(table.raw_get::<Value>(key.clone())?, Value::Nil) {
                    return Err(mlua::Error::runtime("duplicate scenario data key"));
                }
                table.raw_set(key, restore_value(lua, value, depth + 1, count)?)?;
            }
            Value::Table(table)
        }
        ScenarioLuaSavedValue::PersistentEntity(id) if !matches!(*id, 0 | u64::MAX) => {
            let reference = lua.create_table()?;
            reference.set(
                super::scenario_lua_entity_queries::PERSISTENT_ENTITY_FIELD,
                *id,
            )?;
            Value::Table(reference)
        }
        _ => return Err(mlua::Error::runtime("invalid scenario data value")),
    })
}

pub(super) fn prepare_runtime(
    saved: &ScenarioLuaSavedState,
    document: bevy::asset::AssetId<WorldScenarioDocumentAsset>,
    scripts: &Assets<LuaScriptAsset>,
    index: &LuaScriptArchivePathIndex,
) -> mlua::Result<ScenarioLuaVirtualMachine> {
    let lua = Lua::new();
    let loaded = RefCell::new(BTreeSet::new());
    let stack = RefCell::new(Vec::new());
    if saved.modules.len() > 1024 {
        return Err(mlua::Error::runtime("too many scenario modules"));
    }
    lua.scope(|scope| {
        install_lua_script_dependency_loading_functions(
            &lua, scope, scripts, index, &loaded, &stack,
        )?;
        for module in &saved.modules {
            let (path, handle) = index
                .resolve(module, module)
                .ok_or_else(|| mlua::Error::runtime("saved scenario module is unavailable"))?;
            execute_lua_script_module_once_per_virtual_machine(
                &lua, path, handle, scripts, &loaded, &stack,
            )?;
        }
        Ok(())
    })?;
    restore_globals(&lua, saved)?;
    Ok(ScenarioLuaVirtualMachine {
        lua_virtual_machine: lua,
        scenario_document_asset_identifier: document,
        loaded_lua_script_asset_identifiers: loaded.into_inner(),
        executed_source_paths: saved.modules.clone(),
    })
}

fn restore_globals(lua: &Lua, saved: &ScenarioLuaSavedState) -> mlua::Result<()> {
    let standard = standard_global_names()?;
    let globals = lua.globals();
    let remove = globals
        .pairs::<mlua::String, Value>()
        .filter_map(|entry| match entry {
            Ok((key, value))
                if !standard.contains(&key.as_bytes().to_vec())
                    && !matches!(value, Value::Function(_)) =>
            {
                Some(Ok(key))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<mlua::Result<Vec<_>>>()?;
    for key in remove {
        globals.raw_set(key, Value::Nil)?;
    }
    let mut count = 0;
    let mut keys = BTreeSet::new();
    for (key, value) in &saved.globals {
        if standard.contains(key) || key.len() > MAX_BYTES || !keys.insert(key) {
            return Err(mlua::Error::runtime("invalid scenario global key"));
        }
        globals.raw_set(
            lua.create_string(key)?,
            restore_value(lua, value, 0, &mut count)?,
        )?;
    }
    Ok(())
}

pub(crate) struct PreparedScenarioLuaRuntime(ScenarioLuaVirtualMachine);

impl super::scenario_lua_types::ScenarioLuaVirtualMachineContexts {
    pub(crate) fn capture_saved_scenario_state(
        &self,
        scenario: openzt2_game_data::AssetId,
    ) -> mlua::Result<Option<ScenarioLuaSavedState>> {
        self.virtual_machines_by_scenario_identifier
            .get(&scenario)
            .map(capture_runtime)
            .transpose()
    }

    pub(crate) fn prepare_saved_scenario_state(
        saved: &ScenarioLuaSavedState,
        document: bevy::asset::AssetId<WorldScenarioDocumentAsset>,
        scripts: &Assets<LuaScriptAsset>,
        index: &LuaScriptArchivePathIndex,
    ) -> mlua::Result<PreparedScenarioLuaRuntime> {
        prepare_runtime(saved, document, scripts, index).map(PreparedScenarioLuaRuntime)
    }

    pub(crate) fn install_saved_scenario_state(
        &mut self,
        scenario: openzt2_game_data::AssetId,
        prepared: PreparedScenarioLuaRuntime,
    ) {
        self.virtual_machines_by_scenario_identifier
            .insert(scenario, prepared.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_rule_arguments_and_globals_do_not_repeat_completed_effects() {
        let script = r#"
            function evaluate(data)
                if not data.timer then data.timer = 6; data.monthfraction = 0.375 end
                if not data.paid then rewards = (rewards or 0) + 100; data.paid = true end
                return data.timer, data.monthfraction, rewards, releaseanimals
            end
        "#;
        let lua = Lua::new();
        lua.load(script).exec().unwrap();
        let arguments = lua.create_table().unwrap();
        let rule_key = [7_u8; 16];
        let data = lua.create_table().unwrap();
        let animal = lua.create_table().unwrap();
        animal
            .set(
                super::super::scenario_lua_entity_queries::ENTITY_FIELD,
                99_u64,
            )
            .unwrap();
        animal
            .set(
                super::super::scenario_lua_entity_queries::PERSISTENT_ENTITY_FIELD,
                42_u64,
            )
            .unwrap();
        animal.set("_this", animal.clone()).unwrap();
        animal
            .set("method", lua.create_function(|_, ()| Ok(())).unwrap())
            .unwrap();
        data.set("animal", animal).unwrap();
        arguments
            .set(lua.create_string(rule_key).unwrap(), data.clone())
            .unwrap();
        lua.globals()
            .set("__openzt2_rule_arguments", arguments)
            .unwrap();
        lua.globals().set("releaseanimals", true).unwrap();
        let converter_temporary = lua.create_table().unwrap();
        converter_temporary
            .set(
                "BFG_GET_ENTITIES_DIRECT",
                lua.create_function(|_, ()| Ok(())).unwrap(),
            )
            .unwrap();
        lua.globals()
            .set("__%lunify%__temp32_0", converter_temporary)
            .unwrap();
        let evaluate: mlua::Function = lua.globals().get("evaluate").unwrap();
        let before: (i64, f64, i64, bool) = evaluate.call(data).unwrap();
        let context = ScenarioLuaVirtualMachine {
            lua_virtual_machine: lua,
            scenario_document_asset_identifier: Handle::<WorldScenarioDocumentAsset>::default()
                .id(),
            loaded_lua_script_asset_identifiers: BTreeSet::new(),
            executed_source_paths: Vec::new(),
        };
        let saved = capture_runtime(&context).unwrap();
        let encoded = postcard::to_allocvec(&saved).unwrap();
        let saved: ScenarioLuaSavedState = postcard::from_bytes(&encoded).unwrap();
        let restored = Lua::new();
        restored.load(script).exec().unwrap();
        restore_globals(&restored, &saved).unwrap();
        let arguments: mlua::Table = restored.globals().get("__openzt2_rule_arguments").unwrap();
        let data: mlua::Table = arguments
            .get(restored.create_string(rule_key).unwrap())
            .unwrap();
        let evaluate: mlua::Function = restored.globals().get("evaluate").unwrap();
        let animal: mlua::Table = data.get("animal").unwrap();
        assert_eq!(
            animal
                .get::<u64>(super::super::scenario_lua_entity_queries::PERSISTENT_ENTITY_FIELD)
                .unwrap(),
            42
        );
        assert!(matches!(
            animal
                .get::<Value>(super::super::scenario_lua_entity_queries::ENTITY_FIELD)
                .unwrap(),
            Value::Nil
        ));
        assert_eq!(
            evaluate.call::<(i64, f64, i64, bool)>(data).unwrap(),
            before
        );
        context
            .lua_virtual_machine
            .load("loop = {}; loop.self = loop")
            .exec()
            .unwrap();
        assert!(capture_runtime(&context).is_err());
    }
}
