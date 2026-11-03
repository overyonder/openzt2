//! Runs authored behavior scripts using the existing Lua loader and module owner.

use super::behavior_clothing_delivery::{
    script_context::ClothingScriptContext,
    state::{ClothingScriptTexture, PendingClothingDelivery},
};
use bevy::prelude::*;
use mlua::{AnyUserData, Function, Lua, Table};
use openzt2_game_assets::lua_script::lua_script_module_execution::execute_lua_script_module_once_per_virtual_machine;
use openzt2_game_assets::lua_script::lua_script_module_execution::install_lua_script_dependency_loading_functions;
use openzt2_game_assets::{
    behavior::behavior_asset_types::BehaviorDocumentAsset,
    lua_script::{
        lua_script_archive_path_index::LuaScriptArchivePathIndex,
        lua_script_asset_loading::LuaScriptAsset,
    },
};
use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::physical_presentation_state::PhysicalPresentationOperation;
use openzt2_world_state::world_spawn::physical_presentation_state::PhysicalPresentationRequest;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::behavior_task_execution_types::advance_behavior_task_to_next_action;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

mod world_access;
pub(super) use world_access::remove_settled_behavior_script_transactions;
use world_access::{BehaviorScriptWorldAccess, BehaviorScriptWorldChanges};

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
    clothing: ClothingScriptContext,
    mut actors: Query<
        (Entity, &mut BehaviorTaskExecutionState),
        (
            Without<PendingBehaviorTaskFailure>,
            Without<PendingClothingDelivery>,
        ),
    >,
    entities: Query<()>,
    mut world: BehaviorScriptWorldAccess,
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
        let output_requests = &output;
        let textures = RefCell::new(Vec::new());
        let crated = RefCell::new(Vec::new());
        let script_changes = RefCell::new(BehaviorScriptWorldChanges::default());
        let changes = &script_changes;
        let execution_id = task.execution_id;
        let world_view = &world;
        let result = vm.lua.scope(|scope| -> mlua::Result<Option<String>> {
            install_lua_script_dependency_loading_functions(
                &vm.lua,
                scope,
                &scripts,
                &script_index,
                &vm.loaded,
                &path_stack,
            )?;
            for name in ["LUALOG", "SYSERROR", "BFLOG"] {
                vm.lua.globals().set(
                    name,
                    scope.create_function(move |_, message: mlua::Value| {
                        debug!(function = name, ?message, "behavior script diagnostic");
                        Ok(())
                    })?,
                )?;
            }
            vm.lua.globals().set(
                "queryObject",
                scope.create_function(move |lua, name: String| {
                    let manager = lua.create_table()?;
                    match name.as_str() {
                        "BFGManager" => {
                            manager.set(
                                "BFG_GET_ENTITIES",
                                scope.create_function(
                                    move |lua, (_, binder_type): (Table, String)| {
                                        let found = lua.create_table()?;
                                        for (index, entity) in world_view
                                            .entities_of_type(&binder_type)
                                            .into_iter()
                                            .enumerate()
                                        {
                                            let row = lua.create_table()?;
                                            row.set(
                                                "value",
                                                lua.create_userdata(
                                                    BehaviorScriptEntityReference(entity),
                                                )?,
                                            )?;
                                            found.set(index + 1, row)?;
                                        }
                                        Ok(found)
                                    },
                                )?,
                            )?;
                            // `findType` (`entity.lua`) prefers a selected entity,
                            // which behavior scripts do not have.
                            manager.set(
                                "BFG_GET_SELECTED_ENTITY",
                                scope.create_function(|_, _: Table| Ok(mlua::Value::Nil))?,
                            )?;
                        }
                        // `justgavebirth.lua` reports an endangered birth.
                        // ponytail: births already raise `AnimalBorn`; the
                        // endangered-birth message and mother name are not shown.
                        "ZTWorldMgr" => {
                            for method in ["ZT_SET_BIRTH_MOTHER_NAME", "ZT_NOTIFY_OF_BIRTH"] {
                                manager.set(
                                    method,
                                    scope.create_function(|_, _: mlua::MultiValue| Ok(()))?,
                                )?;
                            }
                        }
                        "ZTEconomyMgr" => manager.set(
                            "ZT_GENERIC_TRANSACTION",
                            scope.create_function(move |_, (_, transaction): (Table, Table)| {
                                let category: String = transaction.get("string")?;
                                let amount: f64 = transaction.get("float")?;
                                if category != "destroy" || amount <= 0.0 {
                                    return Err(mlua::Error::runtime(format!(
                                        "unmapped behavior transaction {category} {amount}"
                                    )));
                                }
                                changes.borrow_mut().incomes.push(amount);
                                Ok(())
                            })?,
                        )?,
                        _ => {
                            return Err(mlua::Error::runtime(format!(
                                "behavior queryObject does not expose {name}"
                            )));
                        }
                    }
                    Ok(manager)
                })?,
            )?;
            vm.lua.globals().set(
                "resolveTable",
                scope.create_function(|lua, reference: AnyUserData| {
                    let entity = reference.borrow::<BehaviorScriptEntityReference>()?.0;
                    if !entities.contains(entity)
                        && !world_view.can_resolve(actor, execution_id, entity)
                    {
                        return Ok(None);
                    }
                    let table = lua.create_table()?;
                    let subject_entity = |subject: &Table| -> mlua::Result<Entity> {
                        let reference: AnyUserData = subject.get("entity")?;
                        Ok(reference.borrow::<BehaviorScriptEntityReference>()?.0)
                    };
                    table.set(
                        "BFG_GET_ENTITY_POSITION",
                        scope.create_function(move |lua, subject: Table| {
                            let entity = subject_entity(&subject)?;
                            let position = world_view
                                .position(actor, execution_id, entity)
                                .ok_or_else(|| mlua::Error::runtime("entity has no position"))?;
                            // Native positions are ground x/y with height in z.
                            let table = lua.create_table()?;
                            table.set("x", position.x)?;
                            table.set("y", position.z)?;
                            table.set("z", position.y)?;
                            Ok(table)
                        })?,
                    )?;
                    table.set(
                        "BFG_GET_BINDER_TYPE",
                        scope.create_function(move |_, subject: Table| {
                            let entity = subject_entity(&subject)?;
                            world_view
                                .binder_type(actor, execution_id, entity)
                                .ok_or_else(|| mlua::Error::runtime("entity has no binder type"))
                        })?,
                    )?;
                    table.set(
                        "BFG_GET_ATTR_FLOAT",
                        scope.create_function(move |_, (subject, name): (Table, String)| {
                            let entity = subject_entity(&subject)?;
                            if name != "f_PooLevel" {
                                return Err(mlua::Error::runtime(format!(
                                    "unmapped behavior float attribute {name}"
                                )));
                            }
                            Ok(changes
                                .borrow()
                                .poo_levels
                                .iter()
                                .rev()
                                .find_map(|(written, level)| (*written == entity).then_some(*level))
                                .unwrap_or_else(|| world_view.poo_level(entity)))
                        })?,
                    )?;
                    table.set(
                        "BFG_SET_ATTR_FLOAT",
                        scope.create_function(move |_, (subject, value): (Table, Table)| {
                            let entity = subject_entity(&subject)?;
                            let name: String = value.get(1)?;
                            let level: f32 = value.get(2)?;
                            if name != "f_PooLevel" || !level.is_finite() {
                                return Err(mlua::Error::runtime(format!(
                                    "unmapped behavior float attribute {name}"
                                )));
                            }
                            changes.borrow_mut().poo_levels.push((entity, level));
                            Ok(())
                        })?,
                    )?;
                    table.set(
                        "sendMessage",
                        scope.create_function(
                            move |lua, (subject, message, value): (Table, String, mlua::Value)| {
                                let output = output_requests;
                                let entity = subject_entity(&subject)?;
                                let text = || -> mlua::Result<String> {
                                    lua.unpack::<String>(value.clone())
                                };
                                match message.as_str() {
                                    "BFG_SETPHYSANIM" | "BFG_PUSHPHYSANIM" => {
                                        let state =
                                            AssetId::from_key(&text()?.to_ascii_lowercase());
                                        output.borrow_mut().push(PhysicalPresentationRequest {
                                            owner: entity,
                                            operation: if message == "BFG_SETPHYSANIM" {
                                                PhysicalPresentationOperation::Set(state)
                                            } else {
                                                PhysicalPresentationOperation::Push(state)
                                            },
                                        });
                                        Ok(mlua::Value::Nil)
                                    }
                                    // `canreproduce.lua`: pregnancy tokens and young children.
                                    "BFAI_HAS_TOKEN" => Ok(mlua::Value::Boolean(
                                        world_view.has_token(entity, &text()?),
                                    )),
                                    // `justgavebirth.lua`: parents, mates and children.
                                    "BFAI_GET_RELATED_ENTITY" => {
                                        match world_view.related(entity, &text()?) {
                                            Some(related) => {
                                                Ok(mlua::Value::UserData(lua.create_userdata(
                                                    BehaviorScriptEntityReference(related),
                                                )?))
                                            }
                                            None => Ok(mlua::Value::Nil),
                                        }
                                    }
                                    "BFAI_GET_RELATED_ENTITIES"
                                        if matches!(text()?.as_str(), "child" | "mate") =>
                                    {
                                        let related = if text()? == "child" {
                                            world_view.children(entity)
                                        } else {
                                            world_view.related(entity, "mate").into_iter().collect()
                                        };
                                        let found = lua.create_table()?;
                                        for (index, child) in related.into_iter().enumerate() {
                                            let row = lua.create_table()?;
                                            row.set(
                                                "value",
                                                lua.create_userdata(
                                                    BehaviorScriptEntityReference(child),
                                                )?,
                                            )?;
                                            found.set(index + 1, row)?;
                                        }
                                        Ok(mlua::Value::Table(found))
                                    }
                                    // `setmates.lua`: one mate per animal.
                                    "BFAI_CLEAR_RELATIONSHIP" if text()? == "mate" => {
                                        changes.borrow_mut().mates.push((entity, None));
                                        Ok(mlua::Value::Nil)
                                    }
                                    "BFAI_ADD_RELATIONSHIP" => {
                                        let relationship: Table = lua.unpack(value.clone())?;
                                        let relation: String = relationship.get("relation")?;
                                        // Kinship is derived from `Parents`, which the birth
                                        // that runs this script already set.
                                        if matches!(
                                            relation.as_str(),
                                            "mother"
                                                | "father"
                                                | "child"
                                                | "sibling"
                                                | "grandchild"
                                                | "grandMM"
                                                | "grandMF"
                                                | "grandFM"
                                                | "grandFF"
                                        ) {
                                            return Ok(mlua::Value::Nil);
                                        }
                                        if relation != "mate" {
                                            return Err(mlua::Error::runtime(format!(
                                                "unmapped behavior relationship {relation}"
                                            )));
                                        }
                                        let mate = subject_entity(&relationship.get("target")?)?;
                                        changes.borrow_mut().mates.push((entity, Some(mate)));
                                        Ok(mlua::Value::Nil)
                                    }
                                    _ => Err(mlua::Error::runtime(format!(
                                        "unmapped behavior entity message {message}"
                                    ))),
                                }
                            },
                        )?,
                    )?;
                    table.set(
                        "BFG_GET_ATTR_BOOLEAN",
                        scope.create_function(move |_, (subject, name): (Table, String)| {
                            let entity = subject_entity(&subject)?;
                            match name.as_str() {
                                "b_Adult" => Ok(world_view.is_adult(entity)),
                                "b_Male" => Ok(world_view.is_male(entity)),
                                _ => Err(mlua::Error::runtime(format!(
                                    "unmapped behavior boolean attribute {name}"
                                ))),
                            }
                        })?,
                    )?;
                    table.set(
                        "BFG_GET_ATTR_STRING",
                        scope.create_function(|_, (subject, name): (Table, String)| {
                            let reference: AnyUserData = subject.get("entity")?;
                            let entity = reference.borrow::<BehaviorScriptEntityReference>()?.0;
                            world_view
                                .animal_string(entity, &name)
                                .or_else(|| clothing.attribute(entity, &name))
                                .ok_or_else(|| {
                                    mlua::Error::runtime(format!("missing string attribute {name}"))
                                })
                        })?,
                    )?;
                    table.set(
                        "BFG_SET_TEXTURE",
                        scope.create_function(|_, (subject, texture): (Table, Table)| {
                            let reference: AnyUserData = subject.get("entity")?;
                            let entity = reference.borrow::<BehaviorScriptEntityReference>()?.0;
                            let physical_object: String = texture.get("physObj")?;
                            let layer: String = texture.get("layer")?;
                            if entity != actor
                                || !physical_object.eq_ignore_ascii_case("mainObj")
                                || !layer.eq_ignore_ascii_case("decal0")
                            {
                                return Err(mlua::Error::runtime(
                                    "unsupported clothing texture recipient",
                                ));
                            }
                            textures.borrow_mut().push(ClothingScriptTexture {
                                material: texture.get("material")?,
                                image: texture.get("image")?,
                            });
                            Ok(())
                        })?,
                    )?;
                    // `CrateAnimal.lua` crates a darted escaped animal.
                    table.set(
                        "BFG_SEND_SIGNAL_SHARED",
                        scope.create_function(|_, (subject, signal): (Table, String)| {
                            if signal != "ZT_CRATE_ENTITY" {
                                return Err(mlua::Error::runtime(format!(
                                    "unmapped behavior entity signal {signal}"
                                )));
                            }
                            let reference: AnyUserData = subject.get("entity")?;
                            crated
                                .borrow_mut()
                                .push(reference.borrow::<BehaviorScriptEntityReference>()?.0);
                            Ok(())
                        })?,
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
        for entity in crated.into_inner() {
            commands.entity(entity).insert(WorldEntityIsCrated);
        }
        world.apply(&mut commands, actor, script_changes.into_inner());
        match result {
            Ok(Some(result)) if result != "error" && result != "failure" => {
                let textures = textures.into_inner();
                if textures.is_empty() {
                    advance_behavior_task_to_next_action(&mut task, clock.tick);
                } else {
                    match clothing.prepare(&task, textures) {
                        Ok(delivery) => {
                            commands.entity(actor).insert(delivery);
                        }
                        Err(error) => {
                            warn!(?actor, ?error, "clothing script delivery failed");
                            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                        }
                    }
                }
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
