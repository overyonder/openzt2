use std::{cell::RefCell, collections::BTreeSet};

use bevy::prelude::*;
use mlua::{Function, Lua, MultiValue, Scope, Table, Value};

use crate::lua_script::{
    lua_script_archive_path_index::LuaScriptArchivePathIndex,
    lua_script_asset_loading::LuaScriptAsset,
};

pub fn install_lua_script_dependency_loading_functions<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    assets: &'env Assets<LuaScriptAsset>,
    index: &'env LuaScriptArchivePathIndex,
    included_assets: &'env RefCell<BTreeSet<bevy::asset::AssetId<LuaScriptAsset>>>,
    active_paths: &'env RefCell<Vec<String>>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    const NATIVE_FILE_READERS: &str = "openzt2 native Lua file readers";
    let native = match lua.named_registry_value::<Option<Table>>(NATIVE_FILE_READERS)? {
        Some(native) => native,
        None => {
            let native = lua.create_table()?;
            native.set("dofile", globals.get::<Function>("dofile")?)?;
            native.set("loadfile", globals.get::<Function>("loadfile")?)?;
            lua.set_named_registry_value(NATIVE_FILE_READERS, native.clone())?;
            native
        }
    };
    let read_stdin: Function = native.get("dofile")?;
    let compile_stdin: Function = native.get("loadfile")?;
    globals.set(
        "include",
        scope.create_function(move |lua, requested: String| {
            let (path, handle) = resolve_lua_file(lua, index, active_paths, &requested)?;
            execute_lua_script_module_once_per_virtual_machine(
                lua,
                path,
                handle,
                assets,
                included_assets,
                active_paths,
            )
        })?,
    )?;
    globals.set(
        "dofile",
        scope.create_function(move |lua, requested: Option<String>| {
            let Some(requested) = requested else {
                return read_stdin.call::<MultiValue>(());
            };
            let (path, handle) = resolve_lua_file(lua, index, active_paths, &requested)?;
            let function = compile_lua_file(lua, path, handle, assets)?;
            active_paths.borrow_mut().push(path.to_owned());
            let result = function.call::<MultiValue>(());
            active_paths.borrow_mut().pop();
            result
        })?,
    )?;
    globals.set(
        "loadfile",
        scope.create_function(move |lua, requested: Option<String>| {
            let Some(requested) = requested else {
                return compile_stdin.call::<MultiValue>(());
            };
            let chunk = resolve_lua_file(lua, index, active_paths, &requested)
                .and_then(|(path, handle)| compile_lua_file(lua, path, handle, assets));
            match chunk {
                Ok(function) => Ok(MultiValue::from_vec(vec![Value::Function(function)])),
                Err(error) => Ok(MultiValue::from_vec(vec![
                    Value::Nil,
                    Value::String(lua.create_string(error.to_string())?),
                ])),
            }
        })?,
    )?;
    Ok(())
}

fn resolve_lua_file<'a>(
    lua: &Lua,
    index: &'a LuaScriptArchivePathIndex,
    active_paths: &RefCell<Vec<String>>,
    requested: &str,
) -> mlua::Result<(&'a str, &'a Handle<LuaScriptAsset>)> {
    let mut level = 1;
    while let Some(source) = lua.inspect_stack(level, |frame| {
        frame.source().source.and_then(|source| {
            let path = source.strip_prefix('@')?;
            index.contains_path(path).then(|| path.to_owned())
        })
    }) {
        if let Some(source) = source {
            return index.resolve(&source, requested).ok_or_else(|| {
                mlua::Error::runtime(format!(
                    "Lua file {requested} does not resolve from {source}"
                ))
            });
        }
        level += 1;
    }
    let paths = active_paths.borrow();
    let source = paths.last().map_or("", String::as_str);
    index.resolve(source, requested).ok_or_else(|| {
        mlua::Error::runtime(format!(
            "Lua file {requested} does not resolve from {source}"
        ))
    })
}

fn compile_lua_file(
    lua: &Lua,
    path: &str,
    handle: &Handle<LuaScriptAsset>,
    assets: &Assets<LuaScriptAsset>,
) -> mlua::Result<Function> {
    let asset = assets
        .get(handle)
        .ok_or_else(|| mlua::Error::runtime(format!("Lua file {path} is not loaded")))?;
    lua.load(asset.normalized_source_or_bytecode_bytes())
        .set_name(format!("@{path}"))
        .into_function()
}

pub fn execute_lua_script_module_once_per_virtual_machine(
    lua: &Lua,
    path: &str,
    handle: &Handle<LuaScriptAsset>,
    assets: &Assets<LuaScriptAsset>,
    included_assets: &RefCell<BTreeSet<bevy::asset::AssetId<LuaScriptAsset>>>,
    active_paths: &RefCell<Vec<String>>,
) -> mlua::Result<()> {
    let id = handle.id();
    if included_assets.borrow().contains(&id) {
        return Ok(());
    }
    let function = compile_lua_file(lua, path, handle, assets)?;
    included_assets.borrow_mut().insert(id);
    active_paths.borrow_mut().push(path.to_owned());
    let result = function.call::<()>(());
    active_paths.borrow_mut().pop();
    result.inspect_err(|_| {
        included_assets.borrow_mut().remove(&id);
    })
}
