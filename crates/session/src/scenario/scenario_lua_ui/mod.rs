//! Scenario script operations enter the existing authored UI projection pipeline.

mod presentation;

use bevy::prelude::*;
use mlua::{Lua, Scope, Table};
use openzt2_game_data::AssetId;
use std::cell::RefCell;

#[derive(Message, Debug, Clone)]
pub(super) enum ScenarioUiCommand {
    Node {
        path: Vec<String>,
        operation: NodeOperation,
    },
    Enabled {
        name: String,
        enabled: bool,
    },
    NextScenario(Option<String>),
    Overhead,
}

#[derive(Debug, Clone)]
pub(super) enum NodeOperation {
    Visible(bool),
    Selected(bool),
    Press,
    Localized(AssetId),
    Text(String),
    RefreshGoals,
}

#[derive(Resource, Default)]
pub(super) struct ScenarioContinuation {
    pub(super) scenario: Option<AssetId>,
}

pub(super) fn register(app: &mut App) {
    use openzt2_game_assets::application_lifecycle::GamePhase;
    app.add_message::<ScenarioUiCommand>()
        .init_resource::<ScenarioContinuation>()
        .init_resource::<presentation::PendingScenarioUi>()
        .add_systems(
            Update,
            presentation::apply_scenario_ui_commands
                .in_set(openzt2_interface_state::ui::UiSet::DomainProjection)
                .run_if(in_state(GamePhase::InGame)),
        )
        .add_systems(
            OnExit(GamePhase::InGame),
            |mut continuation: ResMut<ScenarioContinuation>| {
                continuation.scenario = None;
            },
        )
        .add_systems(OnExit(GamePhase::InGame), presentation::clear_scenario_ui);
}

pub(super) fn install_manager_commands<'scope, 'env: 'scope>(
    scope: &'scope Scope<'scope, 'env>,
    manager: &Table,
    commands: &'env RefCell<Vec<ScenarioUiCommand>>,
) -> mlua::Result<()> {
    for (method, enabled) in [("UI_DISABLE", false), ("UI_ENABLE", true)] {
        manager.set(
            method,
            scope.create_function(move |_, (_, name): (Table, String)| {
                commands
                    .borrow_mut()
                    .push(ScenarioUiCommand::Enabled { name, enabled });
                Ok(())
            })?,
        )?;
    }
    Ok(())
}

pub(super) fn create_mode_adapter<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    commands: &'env RefCell<Vec<ScenarioUiCommand>>,
    queries: super::scenario_lua_types::ScenarioLuaQueryFacts<'env>,
    entities: &'env super::scenario_lua_entity_commands::PendingEntityOperations,
) -> mlua::Result<Table> {
    let mode = lua.create_table()?;
    let placement = RefCell::new((None, 0.0_f32));
    mode.set(
        "sendMessage",
        scope.create_function(
            move |_, (_, message, value): (Table, String, mlua::Value)| {
                match message.as_str() {
                    "ZT_SETMODE" => {
                        let mlua::Value::String(value) = value else {
                            return Err(mlua::Error::runtime("mode must be a string"));
                        };
                        match value.to_str()?.as_ref() {
                            "mode_overhead" => {
                                commands.borrow_mut().push(ScenarioUiCommand::Overhead)
                            }
                            "mode_placement" | "mode_selection" => {}
                            _ => {
                                return Err(mlua::Error::runtime(
                                    "unsupported scenario placement mode",
                                ))
                            }
                        }
                    }
                    "ZT_SETPLACEMENTOBJECT" => {
                        let mlua::Value::String(value) = value else {
                            return Err(mlua::Error::runtime("placement type must be a string"));
                        };
                        placement.borrow_mut().0 = (value.to_str()?.as_ref() != "-")
                            .then(|| AssetId::from_key(&openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(&value.to_string_lossy())));
                    }
                    "ZT_OBJECT_ROTATE" => {
                        let mlua::Value::String(value) = value else {
                            return Err(mlua::Error::runtime(
                                "placement rotation must be a string",
                            ));
                        };
                        placement.borrow_mut().1 = value
                            .to_str()?
                            .parse::<f32>()
                            .map_err(mlua::Error::external)?;
                    }
                    "ZT_PLACEOBJECTSPIRAL" => {
                        let mlua::Value::Table(point) = value else {
                            return Err(mlua::Error::runtime("placement position must be a table"));
                        };
                        let (kind, rotation) = *placement.borrow();
                        let kind = kind.ok_or_else(|| {
                            mlua::Error::runtime("no animal placement type selected")
                        })?;
                        let spawn = queries
                            .spawn_animal
                            .ok_or_else(|| mlua::Error::runtime("animal placement unavailable"))?;
                        let request = spawn(
                            kind,
                            [point.get("x")?, point.get("y")?, point.get("z")?],
                            rotation,
                            &entities.borrow().spawns,
                        )?;
                        entities.borrow_mut().spawns.push(request);
                    }
                    _ => {
                        return Err(mlua::Error::runtime(format!(
                            "unmapped scenario mode message {message}"
                        )))
                    }
                }
                Ok(())
            },
        )?,
    )?;
    Ok(mode)
}

pub(super) fn extend_ui_root<'scope, 'env: 'scope>(
    _lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    root: Table,
    commands: &'env RefCell<Vec<ScenarioUiCommand>>,
) -> mlua::Result<Table> {
    let existing: mlua::Function = root.get("UI_GET_CHILD")?;
    root.set(
        "UI_GET_CHILD",
        scope.create_function(move |lua, (root, name): (Table, String)| {
            if matches!(
                name.as_str(),
                "challenge layout"
                    | "instant challenge layout"
                    | "photo challenges layout"
                    | "challenge text"
            ) {
                return existing.call::<Table>((root, name));
            }
            create_node(lua, scope, vec![name], commands)
        })?,
    )?;
    Ok(root)
}

fn create_node<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    path: Vec<String>,
    commands: &'env RefCell<Vec<ScenarioUiCommand>>,
) -> mlua::Result<Table> {
    let node = lua.create_table()?;
    let child_path = path.clone();
    node.set(
        "UI_GET_CHILD",
        scope.create_function(move |lua, (_, name): (Table, String)| {
            let mut path = child_path.clone();
            path.push(name);
            create_node(lua, scope, path, commands)
        })?,
    )?;
    for (method, operation) in [
        ("UI_SHOW", NodeOperation::Visible(true)),
        ("UI_HIDE", NodeOperation::Visible(false)),
        ("UI_ACTIVATE_ON", NodeOperation::Selected(true)),
        ("UI_ACTIVATE_OFF", NodeOperation::Selected(false)),
        ("UI_REPRESS", NodeOperation::Press),
        ("ZT_AUTOPOPULATE_LIST", NodeOperation::RefreshGoals),
    ] {
        let path = path.clone();
        node.set(
            method,
            scope.create_function(move |_, _: mlua::MultiValue| {
                commands.borrow_mut().push(ScenarioUiCommand::Node {
                    path: path.clone(),
                    operation: operation.clone(),
                });
                Ok(())
            })?,
        )?;
    }
    let localized_path = path.clone();
    node.set("UI_SET_LOCID", scope.create_function(move |_, (_, key): (Table, String)| {
        let key = openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(&key);
        commands.borrow_mut().push(ScenarioUiCommand::Node { path: localized_path.clone(), operation: NodeOperation::Localized(AssetId::from_key(&key)) });
        Ok(())
    })?)?;
    node.set(
        "UI_SET_TEXT",
        scope.create_function(move |_, (_, text): (Table, String)| {
            commands.borrow_mut().push(ScenarioUiCommand::Node {
                path: path.clone(),
                operation: NodeOperation::Text(text),
            });
            Ok(())
        })?,
    )?;
    Ok(node)
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScenarioUiSavedState {
    enabled: std::collections::BTreeMap<String, bool>,
    continuation: Option<AssetId>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ScenarioUiSnapshotCapture<'w> {
    pending: Option<Res<'w, presentation::PendingScenarioUi>>,
    continuation: Option<Res<'w, ScenarioContinuation>>,
}

impl ScenarioUiSnapshotCapture<'_> {
    pub(crate) fn capture(&self) -> ScenarioUiSavedState {
        ScenarioUiSavedState {
            enabled: self
                .pending
                .as_ref()
                .map(|pending| pending.enabled.clone())
                .unwrap_or_default(),
            continuation: self
                .continuation
                .as_ref()
                .and_then(|continuation| continuation.scenario),
        }
    }
}

pub(crate) fn restore_saved_scenario_ui(commands: &mut Commands, saved: ScenarioUiSavedState) {
    let mut pending = presentation::PendingScenarioUi::default();
    pending.enabled = saved.enabled;
    commands.insert_resource(pending);
    commands.insert_resource(ScenarioContinuation {
        scenario: saved.continuation,
    });
}
