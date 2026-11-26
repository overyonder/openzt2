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
    lua_script::{lua_script_asset_loading::LuaScriptAsset, lua_script_sources::LuaScriptSources},
};
use openzt2_game_data::AssetId;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::physical_presentation_state::PhysicalPresentationOperation;
use openzt2_world_state::world_spawn::physical_presentation_state::PhysicalPresentationRequest;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::WhenBehaviorAction;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorScript, RunningBehaviorInstruction,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

pub(super) mod script_particles;
mod world_access;
pub use world_access::BreedingCenterEndangeredBirths;
pub(super) use world_access::remove_settled_behavior_script_transactions;
use world_access::{
    BehaviorScriptEffect, BehaviorScriptPlacement, BehaviorScriptStructuralEdits,
    BehaviorScriptWorldAccess, DiseaseAttribute, ScriptFloatAttribute, ScriptPlacedObject,
    need_distress,
};

#[derive(Default)]
pub(super) struct BehaviorScriptContexts {
    contexts: BTreeMap<AssetId, BehaviorScriptVirtualMachine>,
}

struct BehaviorScriptVirtualMachine {
    lua: Lua,
    loaded: RefCell<BTreeSet<AssetId>>,
}

#[derive(Clone, Copy)]
struct BehaviorScriptEntityReference(Entity);
impl mlua::UserData for BehaviorScriptEntityReference {}

pub(super) fn invalidate_changed_behavior_script_contexts(
    mut contexts: NonSendMut<BehaviorScriptContexts>,
    mut events: MessageReader<AssetEvent<LuaScriptAsset>>,
    server: Res<AssetServer>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } = event {
            let path = server
                .get_path(*id)
                .map(|path| AssetId::from_virtual_path(&path.path().to_string_lossy()));
            contexts.contexts.retain(|_, context| {
                path.is_some_and(|path| !context.loaded.borrow().contains(&path))
            });
        }
    }
}

pub(super) fn execute_authored_behavior_script_actions(
    activation: On<WhenBehaviorAction<RunBehaviorScript>>,
    mut commands: Commands,
    mut contexts: NonSendMut<BehaviorScriptContexts>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_sources: Res<LuaScriptSources>,
    clock: Single<&ZooClock>,
    clothing: ClothingScriptContext,
    mut actors: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorScript,
            ),
        ),
        (
            Without<PendingBehaviorTaskFailure>,
            Without<PendingClothingDelivery>,
        ),
    >,
    entities: Query<()>,
    mut world: BehaviorScriptWorldAccess,
) {
    let mut structural_edits = BehaviorScriptStructuralEdits::default();
    for (actor, (task, instruction, running_action)) in
        std::iter::once(actors.get_mut(activation.entity)).flatten()
    {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(document) = documents.get(&task.document) else {
            continue;
        };
        if !instruction.is_current(task) {
            continue;
        }
        let RunBehaviorScript {
            context,
            file,
            function,
            parameters,
        } = running_action;
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
        let textures = RefCell::new(Vec::new());
        let texture_writes = &textures;
        let clothing_context = &clothing;
        let entity_lookup = &entities;
        let script_effects = RefCell::new(Vec::new());
        let effects = &script_effects;
        let pending_edits = &structural_edits;
        let execution_id = task.execution_id;
        let script_world = RefCell::new(&mut world);
        let world_view = &script_world;
        let script_commands = RefCell::new(&mut commands);
        let command_queue = &script_commands;
        let placed_objects = RefCell::new(Vec::<ScriptPlacedObject>::new());
        let placed = &placed_objects;
        let result = vm.lua.scope(|scope| -> mlua::Result<Option<String>> {
            install_lua_script_dependency_loading_functions(
                &vm.lua,
                scope,
                &scripts,
                &script_sources,
                &vm.loaded,
                &path_stack,
            )?;
            vm.lua.globals().set("loadComponent", scope.create_function(|lua, value: Table| {
                let kind = value.get::<String>("_xml")?;
                if !kind.eq_ignore_ascii_case("BFAIToken") {
                    return Ok(None);
                }
                let token = lua.create_table()?;
                token.set("Name", value.get::<String>("Name")?)?;
                Ok(Some(token))
            })?)?;
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
                                            .borrow()
                                            .entities_of_type(actor, &binder_type)
                                            .into_iter()
                                            .chain(placed.borrow().iter().filter(|object| object.binder.eq_ignore_ascii_case(&binder_type)).map(|object| object.entity))
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
                        "ZTWorldMgr" => {
                            manager.set("ZT_PLACEOBJECT", scope.create_function(move |_, (_manager, request): (Table, Table)| {
                                let request = BehaviorScriptPlacement {
                                    kind: request.get("placementobject")?,
                                    position: Vec3::new(request.get("x")?, request.get("z")?, request.get("y")?),
                                    rotation_degrees: request.get::<Option<f32>>("r")?.unwrap_or_default(),
                                    ignore_land_collision: request.get::<Option<bool>>("ignoreLandCollision")?.unwrap_or_default(),
                                    ignore_water_adjacent: request.get::<Option<bool>>("ignoreWaterAdjacent")?.unwrap_or_default(),
                                };
                                if let Some(object) = world_view.borrow_mut().place_object(&mut command_queue.borrow_mut(), actor, request) {
                                    placed.borrow_mut().push(object);
                                    Ok(true)
                                } else {
                                    Ok(false)
                                }
                            })?)?;
                            manager.set(
                                "ZT_SET_BIRTH_MOTHER_NAME",
                                scope.create_function(move |_, arguments: mlua::MultiValue| {
                                    effects.borrow_mut().push(
                                        BehaviorScriptEffect::BirthMotherName(birth_notice_text(
                                            arguments,
                                        )),
                                    );
                                    Ok(())
                                })?,
                            )?;
                            manager.set(
                                "ZT_NOTIFY_OF_BIRTH",
                                scope.create_function(move |_, arguments: mlua::MultiValue| {
                                    use openzt2_game_data::species::Sex;
                                    let sex = match birth_notice_text(arguments).as_deref() {
                                        Some("male") => Some(Sex::Male),
                                        Some("female") => Some(Sex::Female),
                                        _ => None,
                                    };
                                    effects
                                        .borrow_mut()
                                        .push(BehaviorScriptEffect::NotifyBirth(sex));
                                    Ok(())
                                })?,
                            )?;
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
                                effects
                                    .borrow_mut()
                                    .push(BehaviorScriptEffect::Income(amount));
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
                scope.create_function(move |lua, reference: AnyUserData| {
                    let entity = reference.borrow::<BehaviorScriptEntityReference>()?.0;
                    if !entity_lookup.contains(entity)
                        && !world_view.borrow().can_resolve(actor, execution_id, entity)
                        && !placed.borrow().iter().any(|object| object.entity == entity)
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
                                .borrow()
                                .position(actor, execution_id, entity)
                                .or_else(|| placed.borrow().iter().find(|object| object.entity == entity).map(|object| object.position))
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
                                .borrow()
                                .binder_type(actor, execution_id, entity)
                                .or_else(|| placed.borrow().iter().find(|object| object.entity == entity).map(|object| object.binder.clone()))
                                .ok_or_else(|| mlua::Error::runtime("entity has no binder type"))
                        })?,
                    )?;
                    table.set(
                        "BFG_GET_ATTR_FLOAT",
                        scope.create_function(move |_, (subject, name): (Table, String)| {
                            let entity = subject_entity(&subject)?;
                            let Some(attribute) = ScriptFloatAttribute::from_name(&name) else {
                                return Ok(0.0);
                            };
                            Ok(effects
                                .borrow()
                                .iter()
                                .rev()
                                .find_map(|effect| match *effect {
                                    BehaviorScriptEffect::Float(written, field, value)
                                        if written == entity && field == attribute =>
                                    {
                                        Some(value)
                                    }
                                    BehaviorScriptEffect::Need(ref request) if request.animal == entity && attribute == ScriptFloatAttribute::AnimalNeed(request.need) => Some(need_distress(request.value_q16)),
                                    _ => None,
                                })
                                .unwrap_or_else(|| world_view.borrow().float(entity, attribute)))
                        })?,
                    )?;
                    table.set(
                        "BFG_SET_ATTR_FLOAT",
                        scope.create_function(move |lua, (subject, value, level): (Table, mlua::Value, Option<f32>)| {
                            let entity = subject_entity(&subject)?;
                            let (name, level) = if let mlua::Value::Table(value) = value {
                                if value.contains_key("name")? {
                                    (value.get::<String>("name")?, value.get::<f32>("value")?)
                                } else {
                                    (value.get::<String>(1)?, value.get::<f32>(2)?)
                                }
                            } else {
                                (lua.unpack::<String>(value)?, level.ok_or_else(|| mlua::Error::runtime("missing float value"))?)
                            };
                            let attribute = ScriptFloatAttribute::from_name(&name)
                                .filter(|_| level.is_finite())
                                .ok_or_else(|| {
                                    mlua::Error::runtime(format!(
                                        "invalid behavior float attribute {name}"
                                    ))
                                })?;
                            if let ScriptFloatAttribute::AnimalNeed(need) = attribute {
                                let request = world_view.borrow().request_need(entity, need, level);
                                if let Some(request) = request {
                                    effects.borrow_mut().push(BehaviorScriptEffect::Need(request));
                                    return Ok(true);
                                }
                                return Ok(false);
                            }
                            if let ScriptFloatAttribute::Authored(attribute) = attribute {
                                if world_view.borrow().authored_float(entity, attribute).is_none() {
                                    return Ok(false);
                                }
                            }
                            if !world_view
                                .borrow_mut()
                                .set_existing_float(entity, attribute, level)
                            {
                                if matches!(attribute, ScriptFloatAttribute::GoodNeedPoints | ScriptFloatAttribute::BadNeedPoints) {
                                    return Ok(false);
                                }
                                effects
                                    .borrow_mut()
                                    .push(BehaviorScriptEffect::Float(entity, attribute, level));
                            }
                            Ok(true)
                        })?,
                    )?;
                    table.set(
                        "BFG_GET_PRIMARY_BIOME",
                        scope.create_function(move |_, subject: Table| {
                            Ok(world_view.borrow().primary_biome(subject_entity(&subject)?).unwrap_or_default())
                        })?,
                    )?;
                    table.set(
                        "BFG_GET_TRAVERSABLE_AREA",
                        scope.create_function(move |lua, subject: Table| {
                            let Some(area) = world_view.borrow().traversable_area(subject_entity(&subject)?) else {
                                return Ok(None);
                            };
                            let result = lua.create_table()?;
                            result.set("area", lua.create_userdata(BehaviorScriptEntityReference(area))?)?;
                            result.set("BFG_TA_HAS_BIOME_FEATURE", scope.create_function(move |_, (_subject, value): (Table, mlua::Value)| {
                                let feature = match value {
                                    mlua::Value::Table(value) => value.get::<String>("featureName")?,
                                    mlua::Value::String(value) => value.to_string_lossy(),
                                    _ => return Ok(false),
                                };
                                Ok(world_view.borrow().area_has_feature(area, &feature))
                            })?)?;
                            Ok(Some(result))
                        })?,
                    )?;
                    table.set(
                        "BFG_SEND_TOKEN",
                        scope.create_function(move |_, (subject, token): (Table, Table)| {
                            let entity = subject_entity(&subject)?;
                            let name: String = token.get("Name")?;
                            effects.borrow_mut().push(BehaviorScriptEffect::Token(entity, AssetId::from_key(&name.to_ascii_lowercase()), None));
                            Ok(true)
                        })?,
                    )?;
                    table.set(
                        "sendMessage",
                        scope.create_function(
                            move |lua, (subject, message, value): (Table, String, mlua::Value)| {
                                let entity = subject_entity(&subject)?;
                                let text = || -> mlua::Result<String> {
                                    lua.unpack::<String>(value.clone())
                                };
                                match message.as_str() {
                                    "BFG_PLAY_PARTICLE" => {
                                        effects.borrow_mut().push(BehaviorScriptEffect::Particle(
                                            script_particles::ScriptParticle {
                                                entity,
                                                command: text()?,
                                            },
                                        ));
                                        Ok(mlua::Value::Nil)
                                    }
                                    "BFG_SETPHYSANIM" | "BFG_PUSHPHYSANIM" => {
                                        let state =
                                            AssetId::from_key(&text()?.to_ascii_lowercase());
                                        effects.borrow_mut().push(
                                            BehaviorScriptEffect::Presentation(
                                                PhysicalPresentationRequest {
                                                    owner: entity,
                                                    operation: if message == "BFG_SETPHYSANIM" {
                                                        PhysicalPresentationOperation::Set(state)
                                                    } else {
                                                        PhysicalPresentationOperation::Push(state)
                                                    },
                                                },
                                            ),
                                        );
                                        Ok(mlua::Value::Nil)
                                    }
                                    // `canreproduce.lua`: pregnancy tokens and young children.
                                    "BFAI_HAS_TOKEN" => Ok(mlua::Value::Boolean(
                                        {
                                            let name = text()?;
                                            let key = AssetId::from_key(&name.to_ascii_lowercase());
                                            effects.borrow().iter().any(|effect| matches!(effect, BehaviorScriptEffect::Token(target, token, _) if *target == entity && *token == key))
                                                || world_view.borrow().has_token(entity, &name)
                                        },
                                    )),
                                    // `justgavebirth.lua`: parents, mates and children.
                                    "BFAI_GET_RELATED_ENTITY" => {
                                        let relation = text()?;
                                        let related = if relation == "mate" {
                                            pending_edits.mate(
                                                &world_view.borrow(),
                                                &effects.borrow(),
                                                entity,
                                            )
                                        } else {
                                            world_view
                                                .borrow()
                                                .related(
                                                    entity,
                                                    AssetId::from_key(
                                                        &relation.to_ascii_lowercase(),
                                                    ),
                                                    &effects.borrow(),
                                                )
                                                .into_iter()
                                                .next()
                                        };
                                        match related {
                                            Some(related) => {
                                                Ok(mlua::Value::UserData(lua.create_userdata(
                                                    BehaviorScriptEntityReference(related),
                                                )?))
                                            }
                                            None => Ok(mlua::Value::Nil),
                                        }
                                    }
                                    "BFAI_GET_RELATED_ENTITIES" => {
                                        let relation = text()?;
                                        let related = if relation.eq_ignore_ascii_case("mate") {
                                            pending_edits
                                                .mate(
                                                    &world_view.borrow(),
                                                    &effects.borrow(),
                                                    entity,
                                                )
                                                .into_iter()
                                                .collect()
                                        } else {
                                            world_view.borrow().related(
                                                entity,
                                                AssetId::from_key(&relation.to_ascii_lowercase()),
                                                &effects.borrow(),
                                            )
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
                                    "BFAI_CLEAR_RELATIONSHIP" => {
                                        let relation = text()?;
                                        let change = if relation.eq_ignore_ascii_case("mate") {
                                            BehaviorScriptEffect::Mate(entity, None)
                                        } else {
                                            BehaviorScriptEffect::Kinship(
                                                entity,
                                                AssetId::from_key(&relation.to_ascii_lowercase()),
                                                None,
                                            )
                                        };
                                        effects.borrow_mut().push(change);
                                        Ok(mlua::Value::Nil)
                                    }
                                    "BFAI_ADD_RELATIONSHIP" => {
                                        let relationship: Table = lua.unpack(value.clone())?;
                                        let relation: String = relationship.get("relation")?;
                                        let relative =
                                            subject_entity(&relationship.get("target")?)?;
                                        let change = if relation.eq_ignore_ascii_case("mate") {
                                            BehaviorScriptEffect::Mate(entity, Some(relative))
                                        } else {
                                            BehaviorScriptEffect::Kinship(
                                                entity,
                                                AssetId::from_key(&relation.to_ascii_lowercase()),
                                                Some(relative),
                                            )
                                        };
                                        effects.borrow_mut().push(change);
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
                            Ok(world_view.borrow().boolean(entity, &name))
                        })?,
                    )?;
                    table.set(
                        "BFG_GET_ATTR_STRING",
                        scope.create_function(move |_, (subject, name): (Table, String)| {
                            let reference: AnyUserData = subject.get("entity")?;
                            let entity = reference.borrow::<BehaviorScriptEntityReference>()?.0;
                            world_view
                                .borrow()
                                .animal_string(entity, &name)
                                .or_else(|| clothing_context.attribute(entity, &name))
                                .ok_or_else(|| {
                                    mlua::Error::runtime(format!("missing string attribute {name}"))
                                })
                        })?,
                    )?;
                    table.set(
                        "BFG_SET_TEXTURE",
                        scope.create_function(move |_, (subject, texture): (Table, Table)| {
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
                            texture_writes.borrow_mut().push(ClothingScriptTexture {
                                material: texture.get("material")?,
                                image: texture.get("image")?,
                            });
                            Ok(())
                        })?,
                    )?;
                    // `disease.lua`'s `makeAnimalSick` and `cureAnimal` set
                    // these two attributes and nothing else.
                    table.set(
                        "BFG_SET_ATTR_BOOLEAN",
                        scope.create_function(
                            move |_, (subject, name, value): (Table, String, bool)| {
                                let entity = subject_entity(&subject)?;
                                if name != "b_Disease" {
                                    return Err(mlua::Error::runtime(format!(
                                        "unmapped behavior boolean attribute {name}"
                                    )));
                                }
                                effects.borrow_mut().push(BehaviorScriptEffect::Disease(
                                    entity,
                                    DiseaseAttribute::Diseased(value),
                                ));
                                Ok(())
                            },
                        )?,
                    )?;
                    table.set(
                        "BFG_SET_ATTR_STRING",
                        scope.create_function(
                            move |_, (subject, name, value): (Table, String, String)| {
                                let entity = subject_entity(&subject)?;
                                if name != "s_DiseaseName" {
                                    return Err(mlua::Error::runtime(format!(
                                        "unmapped behavior string attribute {name}"
                                    )));
                                }
                                effects.borrow_mut().push(BehaviorScriptEffect::Disease(
                                    entity,
                                    DiseaseAttribute::Name(value),
                                ));
                                Ok(())
                            },
                        )?,
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
                            effects.borrow_mut().push(BehaviorScriptEffect::Crate(
                                reference.borrow::<BehaviorScriptEntityReference>()?.0,
                            ));
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
            for (name, entity) in [
                ("Subject", Some(actor)),
                ("Target", task.target()),
                ("Object", task.object),
            ] {
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
        world.apply(
            &mut commands,
            actor,
            script_effects.into_inner(),
            &mut structural_edits,
        );
        match result {
            Ok(Some(result)) if result != "error" && result != "failure" => {
                let textures = textures.into_inner();
                if textures.is_empty() {
                    instruction.complete(actor, clock.tick, &mut commands);
                } else {
                    match clothing.prepare(task, instruction, textures) {
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

fn birth_notice_text(arguments: mlua::MultiValue) -> Option<String> {
    arguments
        .into_iter()
        .skip(1)
        .find_map(|argument| match argument {
            mlua::Value::String(value) => value.to_str().ok().map(|text| text.to_string()),
            mlua::Value::Table(value) => value
                .get::<String>("string")
                .ok()
                .or_else(|| value.get::<String>("value").ok()),
            _ => None,
        })
}
