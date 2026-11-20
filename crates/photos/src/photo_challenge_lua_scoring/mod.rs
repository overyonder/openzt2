use std::cell::{Cell, RefCell};

use bevy::prelude::*;
use mlua::{Function, Lua, MultiValue, Scope, Table, Value};
use openzt2_game_data::AssetId;

use openzt2_game_assets::lua_script::{
    lua_script_archive_path_index::LuaScriptArchivePathIndex,
    lua_script_asset_loading::LuaScriptAsset,
    lua_script_module_execution::{
        execute_lua_script_module_once_per_virtual_machine,
        install_lua_script_dependency_loading_functions,
    },
};

use super::{
    photo_capture_types::{
        CapturedPhotoEvidence, PhotoEvidence, PhotoSubjectContainedEntitiesEvidence,
        PhotoSubjectTaskEvidence,
    },
    photo_challenge_lua_scoring_types::PhotoChallengeScorer,
};

impl PhotoChallengeScorer {
    pub(super) fn score(
        &mut self,
        path: &str,
        script: &Handle<LuaScriptAsset>,
        scripts: &Assets<LuaScriptAsset>,
        index: &LuaScriptArchivePathIndex,
        entry: &str,
        evidence: &CapturedPhotoEvidence,
    ) -> mlua::Result<i32> {
        let lua = self.lua.get_or_insert_with(Lua::new);
        let modules = RefCell::new(std::mem::take(&mut self.modules));
        let active_module_paths = RefCell::new(Vec::new());
        let cursor = Cell::new(0);
        let result = lua.scope(|scope| {
            install_photo_scoring_logging(lua)?;
            install_lua_script_dependency_loading_functions(
                lua,
                scope,
                scripts,
                index,
                &modules,
                &active_module_paths,
            )?;
            execute_lua_script_module_once_per_virtual_machine(
                lua,
                path,
                script,
                scripts,
                &modules,
                &active_module_paths,
            )?;
            let exposure = create_lua_photo_exposure(lua, scope, evidence, &cursor)?;
            let score: Function = lua.globals().get(entry)?;
            active_module_paths.borrow_mut().push(path.to_owned());
            let globals = lua.globals();
            let predicates = install_photo_count_predicates(lua, scope, evidence)?;
            let result = score
                .call::<Value>(exposure)
                .and_then(convert_lua_photo_score_to_game_score);
            for (name, function) in predicates {
                if globals.get::<Value>(name)? == Value::Function(function) {
                    globals.set(name, Value::Nil)?;
                }
            }
            result
        });
        self.modules = modules.into_inner();
        result
    }
}

fn install_photo_scoring_logging(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    if !globals.contains_key("BFLOG")? {
        globals.set(
            "BFLOG",
            lua.create_function(|_, arguments: MultiValue| {
                for value in arguments {
                    if let Value::String(message) = value {
                        trace!(message = %message.to_string_lossy(), "photo scorer Lua log");
                    }
                }
                Ok(())
            })?,
        )?;
        globals.set("SYSTRACE", 0)?;
        globals.set("SYSNOTE", 1)?;
    }
    Ok(())
}

fn create_lua_photo_exposure<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    evidence: &'env CapturedPhotoEvidence,
    cursor: &'env Cell<usize>,
) -> mlua::Result<Table> {
    let exposure = lua.create_table()?;
    exposure.set(
        "getType",
        lua.create_function(|_, (_, name): (Table, mlua::String)| Ok(name))?,
    )?;
    let subjects = evidence
        .0
        .iter()
        .map(|subject| {
            let entity = lua.create_table()?;
            entity.set(
                "isKindOf",
                scope.create_function(move |_, (_, name): (Table, String)| {
                    Ok(subject_matches_type(subject, &name))
                })?,
            )?;
            entity.set(
                "getPAC",
                lua.create_function(|_, entity: Table| Ok(entity))?,
            )?;
            entity.set(
                "getCurrTask",
                scope.create_function(move |lua, _: Table| {
                    let name = match &subject.semantics.task {
                        PhotoSubjectTaskEvidence::CurrentTask {
                            authored_task_name, ..
                        } => authored_task_name,
                        _ => "",
                    };
                    lua.create_string(name)
                })?,
            )?;
            entity.set(
                "getActiveSequenceName",
                scope.create_function(move |lua, _: Table| {
                    lua.create_string(subject.semantics.animation.as_deref().unwrap_or_default())
                })?,
            )?;
            entity.set(
                "getIsInWater",
                scope.create_function(move |_, _: Table| Ok(subject.semantics.aquatic))?,
            )?;
            let occupants = match &subject.semantics.contained_entities {
                PhotoSubjectContainedEntitiesEvidence::Contained(occupants) => occupants.as_slice(),
                _ => &[],
            };
            let occupants = occupants
                .iter()
                .map(|occupant| {
                    let entity = lua.create_table()?;
                    entity.set(
                        "isKindOf",
                        scope.create_function(move |_, (_, name): (Table, String)| {
                            Ok(matches_type(
                                occupant.definition,
                                occupant.guest,
                                occupant.animal,
                                &name,
                            ))
                        })?,
                    )?;
                    Ok(entity)
                })
                .collect::<mlua::Result<Vec<_>>>()?;
            let first = occupants.first().cloned();
            let occupant_cursor = std::rc::Rc::new(Cell::new(0_usize));
            let first_cursor = occupant_cursor.clone();
            entity.set(
                "getFirstContainedEntity",
                scope.create_function(move |_, _: Table| {
                    first_cursor.set(1);
                    Ok(first.clone())
                })?,
            )?;
            entity.set(
                "getNextContainedEntity",
                scope.create_function(move |_, _: Table| {
                    let index = occupant_cursor.get();
                    occupant_cursor.set(index.saturating_add(1));
                    Ok(occupants.get(index).cloned())
                })?,
            )?;
            Ok(entity)
        })
        .collect::<mlua::Result<Vec<_>>>()?;
    let first = subjects.first().cloned();
    exposure.set(
        "getFirstInterestingEntity",
        scope.create_function(move |_, _: Table| {
            cursor.set(1);
            Ok(first.clone())
        })?,
    )?;
    exposure.set(
        "getNextInterestingEntity",
        scope.create_function(move |_, _: MultiValue| {
            let index = cursor.get();
            cursor.set(index.saturating_add(1));
            Ok(subjects.get(index).cloned())
        })?,
    )?;
    Ok(exposure)
}

fn install_photo_count_predicates<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    evidence: &'env CapturedPhotoEvidence,
) -> mlua::Result<Vec<(&'static str, Function)>> {
    let globals = lua.globals();
    let mut installed = Vec::new();
    for (name, in_water) in [
        ("countType", false),
        ("numberOfT1", false),
        ("numEntityExistsInWater", true),
    ] {
        if globals.get::<Value>(name)?.is_function() {
            continue;
        }
        let function = scope.create_function(move |_, arguments: MultiValue| {
            let Some(Value::String(requested_type)) = arguments.back() else {
                return Err(mlua::Error::runtime("photo count requires a subject type"));
            };
            let requested_type = requested_type.to_str()?;
            Ok(evidence
                .0
                .iter()
                .filter(|subject| {
                    subject_matches_type(subject, &requested_type)
                        && (!in_water || subject.semantics.aquatic)
                })
                .count())
        })?;
        globals.set(name, function.clone())?;
        installed.push((name, function));
    }
    Ok(installed)
}

fn subject_matches_type(subject: &PhotoEvidence, name: &str) -> bool {
    matches_type(
        subject.definition,
        subject.semantics.guest,
        subject.semantics.animal,
        name,
    ) || subject
        .semantics
        .animal_variant
        .is_some_and(|id| matches_type(id, false, false, name))
}

fn matches_type(definition: AssetId, guest: bool, animal: bool, name: &str) -> bool {
    definition == AssetId::from_key(name)
        || definition == AssetId::from_key(&name.to_ascii_lowercase())
        || (guest && name.eq_ignore_ascii_case("guest"))
        || (animal && name.eq_ignore_ascii_case("animal"))
}

fn convert_lua_photo_score_to_game_score(lua_score: Value) -> mlua::Result<i32> {
    let score = match lua_score {
        Value::Integer(score) => score as f64,
        Value::Number(score) if score.is_finite() => score,
        _ => {
            return Err(mlua::Error::runtime(
                "photo scorer must return a finite number",
            ))
        }
    };
    let milli = if score <= 5.0 {
        score * 1000.0
    } else if score <= 5000.0 {
        score
    } else {
        score / 100.0
    };
    Ok(milli.clamp(0.0, 5000.0).floor() as i32)
}
