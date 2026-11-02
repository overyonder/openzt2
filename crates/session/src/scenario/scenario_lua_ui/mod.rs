//! Scenario script operations enter the existing authored UI projection pipeline.

mod presentation;

use bevy::prelude::*;
use mlua::{Lua, Scope, Table};
use openzt2_game_assets::lua_script::lua_script_archive_path_index::LuaScriptArchivePathIndex;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_game_data::ui_document::{
    document::UiDocument, node_layout::UiNodeRegionMetric, widget::UiWidgetRecord,
};
use openzt2_game_data::AssetId;
use openzt2_zoo_state::scenario::scenario_session_types::SelectedScenarioDocument;
use std::cell::RefCell;

use super::{
    scenario_lua_query_fact_collection::ScenarioLuaQueryFactSources,
    scenario_lua_types::{ScenarioLuaQueryFacts, ScenarioLuaVirtualMachineContexts},
    scenario_lua_virtual_machine::execute_loaded_scenario_lua_script_entry,
};

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
    Shown {
        name: String,
        visible: bool,
    },
    Sound(String),
    UnlockDefinition(AssetId),
    ResearchDuration {
        definition: AssetId,
        seconds: f32,
    },
    NextScenario(Option<String>),
    Overhead,
    SelectionMode,
    Select(Option<Entity>),
}

#[derive(Debug, Clone)]
pub(super) enum NodeOperation {
    Visible(bool),
    Selected(bool),
    Press,
    Localized(AssetId),
    Text(String),
    RefreshGoals,
    Modal(bool),
    WindAnimation,
    Position([i32; 2]),
    Size([i32; 2]),
    IconKey(AssetId),
    AnimationStart([f32; 4]),
    AnimationEnd {
        rectangle: [f32; 4],
        /// The text whose height the script added to `rectangle`.
        fit_text: Option<Vec<String>>,
    },
    /// A script read this text's size. See `UI_GET_STRING_SIZE`.
    MeasureText,
}

/// `ZTUITimedEvents "ring timer"` runs `updateTutorialRing` every 20 ms while it
/// is active, paused or not.
#[derive(Resource, Default)]
pub(super) struct TutorialRingTimer {
    active: bool,
    elapsed_seconds: f32,
}

const TUTORIAL_RING_INTERVAL_SECONDS: f32 = 0.02;
/// Marks a path step naming a document node by index, for unnamed children.
const INDEX_PATH_STEP: char = '\u{1}';
const REFERENCE_PATH_SEPARATOR: char = '\u{2}';

#[derive(Resource, Default)]
pub(super) struct ScenarioContinuation {
    pub(super) scenario: Option<AssetId>,
}

pub(super) fn register(app: &mut App) {
    use openzt2_game_assets::application_lifecycle::GamePhase;
    app.add_message::<ScenarioUiCommand>()
        .init_resource::<ScenarioContinuation>()
        .init_resource::<presentation::PendingScenarioUi>()
        .init_resource::<TutorialRingTimer>()
        .init_resource::<openzt2_interface_state::information::catalogue_types::ScenarioDisabledCatalogueDefinitions>()
        .add_systems(
            Update,
            (
                presentation::apply_scenario_ui_commands,
                presentation::fit_animation_heights_to_text,
                // After applying, so an update can't follow the script's own
                // `UI_HIDE` and turn the finished ring back on.
                run_tutorial_ring_timer,
            )
                .chain()
                .in_set(openzt2_interface_state::ui::UiSet::DomainProjection)
                .run_if(in_state(GamePhase::InGame)),
        )
        .add_systems(
            OnExit(GamePhase::InGame),
            |mut continuation: ResMut<ScenarioContinuation>, mut ring: ResMut<TutorialRingTimer>| {
                continuation.scenario = None;
                *ring = default();
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
    for (method, visible) in [("UI_SHOW", true), ("UI_HIDE", false)] {
        manager.set(
            method,
            scope.create_function(move |_, (_, name): (Table, String)| {
                let command = if presentation::role_for(std::slice::from_ref(&name)).is_some() {
                    ScenarioUiCommand::Node {
                        path: vec![name],
                        operation: NodeOperation::Visible(visible),
                    }
                } else {
                    ScenarioUiCommand::Shown { name, visible }
                };
                commands.borrow_mut().push(command);
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
    queries: ScenarioLuaQueryFacts<'env>,
    entities: &'env super::scenario_lua_entity_commands::PendingEntityOperations,
) -> mlua::Result<Table> {
    let mode = lua.create_table()?;
    let placement = RefCell::new((None, 0.0_f32));
    let set_mode = move |value: &str| match value {
        "mode_overhead" => {
            commands.borrow_mut().push(ScenarioUiCommand::Overhead);
            Ok(())
        }
        "mode_selection" => {
            commands.borrow_mut().push(ScenarioUiCommand::SelectionMode);
            Ok(())
        }
        "mode_placement" => Ok(()),
        _ => Err(mlua::Error::runtime("unsupported scenario placement mode")),
    };
    mode.set(
        "ZT_SETMODE",
        scope.create_function(move |_, (_, value): (Table, String)| set_mode(&value))?,
    )?;
    mode.set(
        "sendMessage",
        scope.create_function(
            move |_, (_, message, value): (Table, String, mlua::Value)| {
                match message.as_str() {
                    "ZT_SETMODE" => {
                        let mlua::Value::String(value) = value else {
                            return Err(mlua::Error::runtime("mode must be a string"));
                        };
                        set_mode(value.to_str()?.as_ref())?;
                    }
                    // Starting Super Staff releases the mouse for walking, which
                    // this game's first-person controls don't capture to begin with.
                    "UI_RBUTTONDOWN" => {}
                    "ZT_SET_SELECTED_ENTITY" => {
                        let entity = match value {
                            mlua::Value::Table(reference) => {
                                Some(super::scenario_lua_entity_queries::resolve_reference_entity(&reference, queries, entities)?)
                            }
                            _ => None,
                        };
                        commands.borrow_mut().push(ScenarioUiCommand::Select(entity));
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
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    root: Table,
    commands: &'env RefCell<Vec<ScenarioUiCommand>>,
    queries: ScenarioLuaQueryFacts<'env>,
) -> mlua::Result<Table> {
    install_node_methods(lua, scope, commands, queries)?;
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
            create_node(lua, &[name])
        })?,
    )?;
    root.set(
        "UI_PLAY_SOUND",
        scope.create_function(move |_, (_, name): (Table, String)| {
            commands.borrow_mut().push(ScenarioUiCommand::Sound(name));
            Ok(())
        })?,
    )?;
    Ok(root)
}

/// `UI_GET_CHILDREN` entries hold a reference that `resolveTable` turns back
/// into a node, alongside the entity references it already resolves.
pub(super) fn install_ui_reference_resolution<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
) -> mlua::Result<()> {
    let entities: mlua::Function = lua.globals().get("resolveTable")?;
    lua.globals().set(
        "resolveTable",
        scope.create_function(move |lua, value: mlua::Value| {
            if let mlua::Value::String(reference) = &value {
                let path: Vec<String> = reference
                    .to_str()?
                    .split(REFERENCE_PATH_SEPARATOR)
                    .map(str::to_owned)
                    .collect();
                return create_node(lua, &path).map(mlua::Value::Table);
            }
            entities.call::<mlua::Value>(value)
        })?,
    )
}

/// A node as its script reads it back, in authored canvas units.
struct LocatedUiNode<'a> {
    document: &'a UiDocument,
    index: usize,
    lineage_visible: bool,
    selected: bool,
    enabled: bool,
    highlighted: bool,
    size: [f32; 2],
    screen_position: [f32; 2],
}

fn locate_ui_node<'a>(
    queries: ScenarioLuaQueryFacts<'a>,
    path: &[String],
) -> Option<LocatedUiNode<'a>> {
    let role = presentation::role_for(path);
    queries.ui_roots.iter().find_map(|(root_entity, root)| {
        let document = queries
            .ui_documents
            .get(&root.document)?
            .canonical_ui_document();
        if role.is_some_and(|role| role != document.role) {
            return None;
        }
        let id = presentation::resolve_node(document, path)?;
        let (node, _, inherited, computed, transform, selected, enabled, interaction) = queries
            .ui_nodes
            .iter()
            .find(|(node, owner, ..)| owner.0 == root_entity && node.id == id)?;
        let authored = &document.nodes[node.index as usize];
        let computed_size = computed.size() * computed.inverse_scale_factor();
        let center = transform.translation / queries.ui_canvas_scale;
        Some(LocatedUiNode {
            document,
            index: node.index as usize,
            lineage_visible: inherited.get(),
            selected: selected.is_some_and(|selected| selected.0),
            enabled: enabled.is_none_or(|enabled| enabled.0),
            highlighted: interaction.is_some_and(|interaction| *interaction != Interaction::None),
            size: if computed_size.min_element() > 0.0 {
                computed_size.to_array()
            } else {
                [authored.rect[2], authored.rect[3]]
            },
            screen_position: (center - computed_size * 0.5).to_array(),
        })
    })
}

/// The visibility this call has already asked for. Native UI calls act at once;
/// these apply a frame later.
fn pending_visibility(commands: &[ScenarioUiCommand], path: &[String]) -> Option<bool> {
    commands.iter().rev().find_map(|command| match command {
        ScenarioUiCommand::Node {
            path: target,
            operation: NodeOperation::Visible(visible),
        } if target == path => Some(*visible),
        _ => None,
    })
}

fn rectangle_table(lua: &Lua, [x, y, w, h]: [f32; 4]) -> mlua::Result<Table> {
    let rectangle = lua.create_table()?;
    rectangle.set("x", x)?;
    rectangle.set("y", y)?;
    rectangle.set("w", w)?;
    rectangle.set("h", h)?;
    Ok(rectangle)
}

fn read_rectangle(rectangle: &Table) -> mlua::Result<[f32; 4]> {
    Ok([
        rectangle.get("x")?,
        rectangle.get("y")?,
        rectangle.get("w")?,
        rectangle.get("h")?,
    ])
}

fn read_point(point: &Table) -> mlua::Result<[i32; 2]> {
    Ok([
        point.get::<f32>("x")?.round() as i32,
        point.get::<f32>("y")?.round() as i32,
    ])
}

/// Scripts keep nodes in globals across calls, such as `chaseComponent`, and
/// everything scoped dies with its call. A node is therefore a plain table holding
/// its path, and finds this call's methods through the shared metatable.
fn create_node(lua: &Lua, path: &[String]) -> mlua::Result<Table> {
    let node = lua.create_table()?;
    node.set("__path", lua.create_sequence_from(path.iter().map(String::as_str))?)?;
    let metatable = match lua.globals().get::<Option<Table>>("__openzt2_ui_node_metatable")? {
        Some(metatable) => metatable,
        None => {
            let metatable = lua.create_table()?;
            metatable.set(
                "__index",
                lua.create_function(|lua, (_, method): (Table, mlua::Value)| {
                    lua.globals()
                        .get::<Table>("__openzt2_ui_node_methods")?
                        .get::<mlua::Value>(method)
                })?,
            )?;
            lua.globals().set("__openzt2_ui_node_metatable", metatable.clone())?;
            metatable
        }
    };
    node.set_metatable(Some(metatable))?;
    Ok(node)
}

fn node_path(node: &Table) -> mlua::Result<Vec<String>> {
    node.raw_get::<Table>("__path")?.sequence_values().collect()
}

fn install_node_methods<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    commands: &'env RefCell<Vec<ScenarioUiCommand>>,
    queries: ScenarioLuaQueryFacts<'env>,
) -> mlua::Result<()> {
    let node = lua.create_table()?;
    lua.globals().set("__openzt2_ui_node_methods", node.clone())?;
    let push = move |path: Vec<String>, operation: NodeOperation| {
        commands
            .borrow_mut()
            .push(ScenarioUiCommand::Node { path, operation });
    };
    node.set(
        "UI_GET_CHILD",
        lua.create_function(|lua, (node, name): (Table, String)| {
            let mut path = node_path(&node)?;
            path.push(name);
            create_node(lua, &path)
        })?,
    )?;
    node.set(
        "UI_GET_PARENT",
        scope.create_function(move |lua, node: Table| {
            let mut path = node_path(&node)?;
            let Some(located) = locate_ui_node(queries, &path) else {
                return Ok(None);
            };
            let parent = located.document.nodes[located.index].parent as usize;
            if parent >= located.document.nodes.len() || parent == located.index {
                return Ok(None);
            }
            path.push(format!("{INDEX_PATH_STEP}{parent}"));
            create_node(lua, &path).map(Some)
        })?,
    )?;
    node.set(
        "UI_GET_CHILDREN",
        scope.create_function(move |lua, node: Table| {
            let children = lua.create_table()?;
            let path = node_path(&node)?;
            let Some(located) = locate_ui_node(queries, &path) else {
                return Ok(children);
            };
            let members = located
                .document
                .nodes
                .iter()
                .enumerate()
                .filter(|(index, node)| {
                    node.parent as usize == located.index && *index != located.index
                });
            for (position, (index, _)) in members.enumerate() {
                let entry = lua.create_table()?;
                entry.set(
                    "value",
                    format!(
                        "{}{REFERENCE_PATH_SEPARATOR}{INDEX_PATH_STEP}{index}",
                        path.join(&REFERENCE_PATH_SEPARATOR.to_string())
                    ),
                )?;
                children.set(position + 1, entry)?;
            }
            Ok(children)
        })?,
    )?;
    node.set(
        "UI_IS_HIDDEN",
        scope.create_function(move |_, node: Table| {
            let path = node_path(&node)?;
            if let Some(visible) = pending_visibility(&commands.borrow(), &path) {
                return Ok(!visible);
            }
            Ok(!locate_ui_node(queries, &path).is_some_and(|node| node.lineage_visible))
        })?,
    )?;
    node.set(
        "UI_GET_LINEAGE_VISIBLE",
        scope.create_function(move |_, node: Table| {
            Ok(locate_ui_node(queries, &node_path(&node)?).is_some_and(|node| node.lineage_visible))
        })?,
    )?;
    node.set(
        "UI_IS_DISABLED",
        scope.create_function(move |_, node: Table| {
            Ok(locate_ui_node(queries, &node_path(&node)?).is_some_and(|node| !node.enabled))
        })?,
    )?;
    node.set(
        "UI_IS_HIGHLIGHTED",
        scope.create_function(move |_, node: Table| {
            Ok(locate_ui_node(queries, &node_path(&node)?).is_some_and(|node| node.highlighted))
        })?,
    )?;
    // Hidden is a state of its own, so a hidden control isn't normal.
    node.set(
        "UI_IS_NORMAL",
        scope.create_function(move |_, node: Table| {
            Ok(locate_ui_node(queries, &node_path(&node)?)
                .is_some_and(|node| node.lineage_visible && !node.selected))
        })?,
    )?;
    for (method, selected) in [
        ("UI_IS_ALTERNATE", true),
        ("UI_IS_ACTIVATED", true),
    ] {
        node.set(
            method,
            scope.create_function(move |_, node: Table| {
                Ok(locate_ui_node(queries, &node_path(&node)?).is_some_and(|node| node.selected)
                    == selected)
            })?,
        )?;
    }
    node.set(
        "UI_GET_SRC_RECT",
        scope.create_function(move |lua, node: Table| {
            let Some(node) = locate_ui_node(queries, &node_path(&node)?) else {
                return Ok(None);
            };
            let authored = &node.document.nodes[node.index];
            rectangle_table(lua, [authored.rect[0], authored.rect[1], node.size[0], node.size[1]])
                .map(Some)
        })?,
    )?;
    node.set(
        "UI_GET_SCREEN_RECT",
        scope.create_function(move |lua, node: Table| {
            let Some(node) = locate_ui_node(queries, &node_path(&node)?) else {
                return Ok(None);
            };
            rectangle_table(
                lua,
                [
                    node.screen_position[0],
                    node.screen_position[1],
                    node.size[0],
                    node.size[1],
                ],
            )
            .map(Some)
        })?,
    )?;
    node.set(
        "UI_GET_IMAGE_SRC_RECT",
        scope.create_function(move |lua, node: Table| {
            let image_path = node_path(&node)?;
            let key = commands.borrow().iter().rev().find_map(|command| match command {
                ScenarioUiCommand::Node {
                    path,
                    operation: NodeOperation::IconKey(key),
                } if *path == image_path => Some(*key),
                _ => None,
            });
            let located = locate_ui_node(queries, &image_path);
            let entry = located.as_ref().and_then(|node| {
                let UiWidgetRecord::MultiIcon { entries } = &node.document.nodes[node.index].widget
                else {
                    return None;
                };
                entries.iter().find(|entry| Some(entry.key) == key)
            });
            let metric = |index: usize| match entry.map(|entry| &entry.source_rect.metrics[index]) {
                Some(UiNodeRegionMetric::Pixels(value)) => *value,
                _ => 1.0,
            };
            rectangle_table(lua, [metric(0), metric(1), metric(2), metric(3)])
        })?,
    )?;
    node.set(
        "UI_GET_STRING_SIZE",
        scope.create_function(move |lua, node: Table| {
            // The script adds this height to its panel's end rectangle, and only
            // layout knows it, a frame on. Report none and let
            // `fit_animation_heights_to_text` add the measured height.
            let path = node_path(&node)?;
            let width = locate_ui_node(queries, &path).map_or(0.0, |node| node.size[0]);
            push(path, NodeOperation::MeasureText);
            rectangle_table(lua, [0.0, 0.0, width, 0.0])
        })?,
    )?;
    for (method, operation) in [
        ("UI_SHOW", NodeOperation::Visible(true)),
        ("UI_HIDE", NodeOperation::Visible(false)),
        ("UI_ACTIVATE_ON", NodeOperation::Selected(true)),
        ("UI_ACTIVATE_OFF", NodeOperation::Selected(false)),
        ("UI_REPRESS", NodeOperation::Press),
        ("UI_ACTIVATE", NodeOperation::Press),
        ("UI_WIND_ANIMATION", NodeOperation::WindAnimation),
        ("ZT_AUTOPOPULATE_LIST", NodeOperation::RefreshGoals),
    ] {
        node.set(
            method,
            scope.create_function(move |_, node: Table| {
                push(node_path(&node)?, operation.clone());
                Ok(())
            })?,
        )?;
    }
    for (method, enabled) in [("UI_ENABLE", true), ("UI_DISABLE", false)] {
        node.set(
            method,
            scope.create_function(move |_, node: Table| {
                if let Some(name) = node_path(&node)?.pop() {
                    commands
                        .borrow_mut()
                        .push(ScenarioUiCommand::Enabled { name, enabled });
                }
                Ok(())
            })?,
        )?;
    }
    // The tutorial document sits above the others and these raise only within it.
    for method in ["UI_RAISE", "UI_MOVE_TO_FRONT"] {
        node.set(method, lua.create_function(|_, _: mlua::MultiValue| Ok(()))?)?;
    }
    node.set(
        "UI_SET_MODAL",
        scope.create_function(move |_, (node, modal): (Table, bool)| {
            push(node_path(&node)?, NodeOperation::Modal(modal));
            Ok(())
        })?,
    )?;
    node.set(
        "UI_SET_POS",
        scope.create_function(move |_, (node, point): (Table, Table)| {
            push(node_path(&node)?, NodeOperation::Position(read_point(&point)?));
            Ok(())
        })?,
    )?;
    node.set(
        "UI_SET_SIZE",
        scope.create_function(move |_, (node, point): (Table, Table)| {
            push(node_path(&node)?, NodeOperation::Size(read_point(&point)?));
            Ok(())
        })?,
    )?;
    node.set(
        "UI_SET_VALUE",
        scope.create_function(move |_, (node, key): (Table, String)| {
            push(node_path(&node)?, NodeOperation::IconKey(AssetId::from_key(&key)));
            Ok(())
        })?,
    )?;
    node.set(
        "UI_SET_ANIM_RECT_START",
        scope.create_function(move |_, (node, rectangle): (Table, Table)| {
            push(
                node_path(&node)?,
                NodeOperation::AnimationStart(read_rectangle(&rectangle)?),
            );
            Ok(())
        })?,
    )?;
    node.set(
        "UI_SET_ANIM_RECT_END",
        scope.create_function(move |_, (node, rectangle): (Table, Table)| {
            let fit_text = commands.borrow().iter().rev().find_map(|command| match command {
                ScenarioUiCommand::Node {
                    path,
                    operation: NodeOperation::MeasureText,
                } => Some(path.clone()),
                _ => None,
            });
            push(
                node_path(&node)?,
                NodeOperation::AnimationEnd {
                    rectangle: read_rectangle(&rectangle)?,
                    fit_text,
                },
            );
            Ok(())
        })?,
    )?;
    node.set("UI_SET_LOCID", scope.create_function(move |_, (node, key): (Table, String)| {
        let key = openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(&key);
        push(node_path(&node)?, NodeOperation::Localized(AssetId::from_key(&key)));
        Ok(())
    })?)?;
    node.set(
        "UI_SET_TEXT",
        scope.create_function(move |_, (node, text): (Table, String)| {
            push(node_path(&node)?, NodeOperation::Text(text));
            Ok(())
        })?,
    )?;
    Ok(())
}

/// Drives `updateTutorialRing` in the VM of the tutorial that started the ring.
#[allow(clippy::too_many_arguments)]
fn run_tutorial_ring_timer(
    mut timer: ResMut<TutorialRingTimer>,
    time: Res<Time<Real>>,
    selected: Option<Res<SelectedScenarioDocument>>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    mut runtime: NonSendMut<ScenarioLuaVirtualMachineContexts>,
    fact_sources: ScenarioLuaQueryFactSources,
    mut ui: MessageWriter<ScenarioUiCommand>,
) {
    if !timer.active {
        return;
    }
    timer.elapsed_seconds += time.delta_secs();
    if timer.elapsed_seconds < TUTORIAL_RING_INTERVAL_SECONDS {
        return;
    }
    timer.elapsed_seconds = 0.0;
    let Some(selected) = selected else {
        return;
    };
    let Some(asset) = scenario_assets.get(&selected.0) else {
        return;
    };
    let Some(binding) = asset
        .document
        .scenario_script_bindings
        .iter()
        .find(|binding| binding.script.to_ascii_lowercase().contains("tutorial"))
    else {
        return;
    };
    match execute_loaded_scenario_lua_script_entry(
        &mut runtime,
        &selected,
        &scenario_assets,
        &scripts,
        &script_index,
        binding,
        "updateTutorialRing",
        fact_sources.borrow_scenario_lua_query_facts(),
    ) {
        Ok(result) => {
            if let Some(failure) = result.failure {
                error!(%failure, "tutorial ring update failed");
            }
            ui.write_batch(result.ui);
        }
        Err(failure) => error!(%failure, "tutorial ring update failed"),
    }
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScenarioUiSavedState {
    enabled: std::collections::BTreeMap<String, bool>,
    #[serde(default)]
    shown: std::collections::BTreeMap<String, bool>,
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
            shown: self
                .pending
                .as_ref()
                .map(|pending| pending.shown.clone())
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
    pending.shown = saved.shown;
    commands.insert_resource(pending);
    commands.insert_resource(ScenarioContinuation {
        scenario: saved.continuation,
    });
}
