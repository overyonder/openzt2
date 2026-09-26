//! Runs authored behavior scripts using the existing Lua loader and module owner.

use crate::{
    assets::{
        behavior::behavior_asset_types::BehaviorDocumentAsset,
        lua_script::{
            lua_script_archive_path_index::LuaScriptArchivePathIndex,
            lua_script_asset_loading::LuaScriptAsset,
        },
    },
    plugins::{
        behavior_task_execution_types::{
            advance_behavior_task_to_next_action, find_current_behavior_task_action,
            BehaviorTaskExecutionState,
        },
        lua_script_module_execution::{
            execute_lua_script_module_once_per_virtual_machine,
            install_lua_script_dependency_loading_functions,
        },
        simulation_time::simulation_clock_types::ZooClock,
        world_spawn::physical_presentation_state::{
            PhysicalPresentationOperation, PhysicalPresentationRequest,
        },
    },
};
use bevy::prelude::*;
use mlua::{AnyUserData, Function, Lua, Table};
use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

#[derive(Default)]
pub(super) struct BehaviorScriptContexts {
    contexts: BTreeMap<AssetId, BehaviorScriptVirtualMachine>,
}

struct BehaviorScriptVirtualMachine {
    lua: Lua,
    loaded: RefCell<BTreeSet<bevy::asset::AssetId<LuaScriptAsset>>>,
}

#[derive(Clone, Copy)]
struct BehaviorScriptEntityReference(Entity);
impl mlua::UserData for BehaviorScriptEntityReference {}

pub(super) fn invalidate_changed_behavior_script_contexts(
    mut contexts: NonSendMut<BehaviorScriptContexts>,
    mut events: MessageReader<AssetEvent<LuaScriptAsset>>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } = event {
            contexts
                .contexts
                .retain(|_, context| !context.loaded.borrow().contains(id));
        }
    }
}

pub(super) fn execute_authored_behavior_script_actions(
    mut commands: Commands,
    mut contexts: NonSendMut<BehaviorScriptContexts>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    clock: Res<ZooClock>,
    mut actors: Query<(Entity, &mut BehaviorTaskExecutionState)>,
    entities: Query<()>,
    mut presentation_requests: MessageWriter<PhysicalPresentationRequest>,
) {
    for (actor, mut task) in &mut actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(document) = documents.get(&task.document) else {
            continue;
        };
        let Some(BehaviorAction::Script {
            context,
            file,
            function,
            parameters,
        }) = find_current_behavior_task_action(document, &task)
        else {
            continue;
        };
        let Some((path, handle)) = document.script_dependency(file) else {
            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if scripts.get(handle).is_none() {
            continue;
        }
        let vm =
            contexts
                .contexts
                .entry(*context)
                .or_insert_with(|| BehaviorScriptVirtualMachine {
                    lua: Lua::new(),
                    loaded: RefCell::new(BTreeSet::new()),
                });
        let path_stack = RefCell::new(Vec::new());
        let output = RefCell::new(Vec::new());
        let result = vm.lua.scope(|scope| -> mlua::Result<Option<String>> {
            install_lua_script_dependency_loading_functions(
                &vm.lua,
                scope,
                &scripts,
                &script_index,
                &vm.loaded,
                &path_stack,
            )?;
            for name in ["LUALOG", "SYSERROR"] {
                vm.lua.globals().set(
                    name,
                    scope.create_function(move |_, message: mlua::Value| {
                        debug!(function = name, ?message, "behavior script diagnostic");
                        Ok(())
                    })?,
                )?;
            }
            vm.lua.globals().set(
                "resolveTable",
                scope.create_function(|lua, reference: AnyUserData| {
                    let entity = reference.borrow::<BehaviorScriptEntityReference>()?.0;
                    if !entities.contains(entity) {
                        return Ok(None);
                    }
                    let table = lua.create_table()?;
                    table.set(
                        "sendMessage",
                        scope.create_function(
                            |_, (subject, message, value): (Table, String, String)| {
                                let reference: AnyUserData = subject.get("entity")?;
                                let entity = reference.borrow::<BehaviorScriptEntityReference>()?.0;
                                let state = AssetId::from_key(&value.to_ascii_lowercase());
                                let operation = match message.as_str() {
                                    "BFG_SETPHYSANIM" => PhysicalPresentationOperation::Set(state),
                                    "BFG_PUSHPHYSANIM" => {
                                        PhysicalPresentationOperation::Push(state)
                                    }
                                    _ => {
                                        return Err(mlua::Error::runtime(format!(
                                            "unmapped behavior entity message {message}"
                                        )));
                                    }
                                };
                                output.borrow_mut().push(PhysicalPresentationRequest {
                                    owner: entity,
                                    operation,
                                });
                                Ok(())
                            },
                        )?,
                    )?;
                    table.set("entity", reference)?;
                    Ok(Some(table))
                })?,
            )?;
            execute_lua_script_module_once_per_virtual_machine(
                &vm.lua,
                path,
                handle,
                &scripts,
                &vm.loaded,
                &path_stack,
            )?;
            let arguments = vm.lua.create_table()?;
            let mut index = 1;
            // Native BFBehScriptParams serializes present Subject, Target and
            // Object children in that order, followed by string parameters.
            for (name, entity) in [("Subject", Some(actor)), ("Target", task.target())] {
                if let Some(entity) = entity {
                    let argument = vm.lua.create_table()?;
                    argument.set("name", name)?;
                    argument.set(
                        "value",
                        vm.lua
                            .create_userdata(BehaviorScriptEntityReference(entity))?,
                    )?;
                    arguments.set(index, argument)?;
                    index += 1;
                }
            }
            for parameter in parameters {
                let argument = vm.lua.create_table()?;
                argument.set("name", "string")?;
                argument.set("value", parameter.as_str())?;
                arguments.set(index, argument)?;
                index += 1;
            }
            path_stack.borrow_mut().push(path.to_owned());
            let function: Function = vm.lua.globals().get(function.as_str())?;
            let result = function.call(arguments);
            path_stack.borrow_mut().pop();
            result
        });
        // Native sends messages as the script executes, including messages
        // preceding a later script error; retain that ordering and effect.
        for request in output.into_inner() {
            presentation_requests.write(request);
        }
        match result {
            Ok(Some(result)) if result != "error" && result != "failure" => {
                advance_behavior_task_to_next_action(&mut task, clock.tick);
            }
            result => {
                warn!(
                    ?actor,
                    script = path,
                    ?result,
                    "authored behavior script failed"
                );
                super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
        }
    }
}
