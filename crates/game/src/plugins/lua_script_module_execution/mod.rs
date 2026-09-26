use std::{cell::RefCell, collections::BTreeSet};

use bevy::prelude::*;
use mlua::{Lua, Scope};

use crate::assets::lua_script::{
    lua_script_archive_path_index::LuaScriptArchivePathIndex,
    lua_script_asset_loading::LuaScriptAsset,
};

pub(super) fn install_lua_script_dependency_loading_functions<'scope, 'env: 'scope>(
    lua_virtual_machine: &Lua,
    lua_scope: &'scope Scope<'scope, 'env>,
    lua_script_assets: &'env Assets<LuaScriptAsset>,
    lua_script_path_index: &'env LuaScriptArchivePathIndex,
    loaded_lua_script_asset_identifiers: &'env RefCell<
        BTreeSet<bevy::asset::AssetId<LuaScriptAsset>>,
    >,
    active_lua_script_source_path_stack: &'env RefCell<Vec<String>>,
) -> mlua::Result<()> {
    for lua_dependency_function_name in ["include", "dofile"] {
        lua_virtual_machine.globals().set(
            lua_dependency_function_name,
            lua_scope.create_function(move |lua_virtual_machine, requested_source_path: String| {
                let including_source_path = active_lua_script_source_path_stack
                    .borrow()
                    .last()
                    .cloned()
                    .ok_or_else(|| mlua::Error::runtime("Lua include has no calling module"))?;
                let (resolved_source_path, dependency_asset_handle) = lua_script_path_index
                    .resolve(&including_source_path, &requested_source_path)
                    .ok_or_else(|| {
                        mlua::Error::runtime(format!(
                            "Lua dependency {requested_source_path} does not resolve from {including_source_path}"
                        ))
                    })?;
                execute_lua_script_module_once_per_virtual_machine(
                    lua_virtual_machine,
                    resolved_source_path,
                    dependency_asset_handle,
                    lua_script_assets,
                    loaded_lua_script_asset_identifiers,
                    active_lua_script_source_path_stack,
                )
            })?,
        )?;
    }
    lua_virtual_machine.globals().set(
        "loadfile",
        lua_scope.create_function(|_, _: String| {
            Err::<(), _>(mlua::Error::runtime(
                "Lua loadfile is not exposed by the live asset runtime",
            ))
        })?,
    )?;
    Ok(())
}

pub(super) fn execute_lua_script_module_once_per_virtual_machine(
    lua_virtual_machine: &Lua,
    lua_script_source_path: &str,
    lua_script_asset_handle: &Handle<LuaScriptAsset>,
    lua_script_assets: &Assets<LuaScriptAsset>,
    loaded_lua_script_asset_identifiers: &RefCell<BTreeSet<bevy::asset::AssetId<LuaScriptAsset>>>,
    active_lua_script_source_path_stack: &RefCell<Vec<String>>,
) -> mlua::Result<()> {
    if !loaded_lua_script_asset_identifiers
        .borrow_mut()
        .insert(lua_script_asset_handle.id())
    {
        return Ok(());
    }
    let lua_script_asset = lua_script_assets
        .get(lua_script_asset_handle)
        .ok_or_else(|| mlua::Error::runtime("Lua dependency is not loaded"))?;
    active_lua_script_source_path_stack
        .borrow_mut()
        .push(lua_script_source_path.to_owned());
    let execution_result = lua_virtual_machine
        .load(lua_script_asset.normalized_source_or_bytecode_bytes())
        .set_name(lua_script_source_path)
        .exec();
    active_lua_script_source_path_stack.borrow_mut().pop();
    execution_result.inspect_err(|_| {
        loaded_lua_script_asset_identifiers
            .borrow_mut()
            .remove(&lua_script_asset_handle.id());
    })
}
