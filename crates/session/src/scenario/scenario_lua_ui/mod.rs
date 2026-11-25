//! Scenario script operations enter the existing authored UI projection pipeline.

pub(super) mod presentation;

use bevy::prelude::*;
use mlua::{Lua, Scope, Table};
use openzt2_game_assets::lua_script::lua_script_archive_path_index::LuaScriptArchivePathIndex;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_data::ui_document::{
    document::UiDocument, node_layout::UiNodeRegionMetric, widget::UiWidgetRecord,
};
use openzt2_game_data::world_scenario::{ScenarioScriptBinding, ScenarioScriptPhase};
use openzt2_game_data::AssetId;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_zoo_state::scenario::scenario_session_types::{
    ActiveScenarioSession, ScenarioDocument,
};
use openzt2_zoo_state::scenario::timed_scenario_ui_actions::{
    RunTimedScenarioUiAction, TimedScenarioUiActionCompleted,
};
use std::cell::RefCell;

use super::{
    scenario_lua_query_fact_collection::ScenarioLuaQueryFactSources,
    scenario_lua_types::{ScenarioLuaContext, ScenarioLuaQueryFacts},
};

#[derive(Message, Debug, Clone)]
pub(super) enum ScenarioUiCommand {
    Tool(openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool),
    /// `ZT_SET_TANK_WALL_HEIGHT`: every tank's wall tops, in world units.
    TankWallHeight(f32),
    /// `ZT_DISPLAY_ZOO_MESSAGE`: localized text for the zoo message bar.
    ZooMessage {
        root: Option<Entity>,
        text: String,
        timeout_seconds: f64,
    },
    /// Selects the placement, fence or path tool for an authored type.
    PlacementObject(AssetId),
    Node {
        path: Vec<String>,
        operation: NodeOperation,
    },
    Enabled {
        name: String,
        enabled: bool,
    },
    Sound(String),
    UnlockDefinition(AssetId),
    ResearchDuration {
        definition: AssetId,
        seconds: f32,
    },
    NextScenario(Option<String>),
    Overhead,
    Select(Option<Entity>),
    TutorialRuleActive(bool),
    FencePlacementMode(openzt2_zoo_state::scenario::scenario_session_types::SetScenarioFencePlacementMode),
}

#[derive(Debug, Clone)]
pub(super) enum NodeOperation {
    /// Returns to the selection tool once earlier node calls have settled.
    SelectionMode,
    ScriptCompleted(Entity),
    Construction(presentation::ScenarioConstructionRequest),
    SelectSubject(Option<Entity>),
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

/// Marks a path step naming a document node by index, for unnamed children.
const INDEX_PATH_STEP: char = '\u{1}';
const REFERENCE_PATH_SEPARATOR: char = '\u{2}';

pub(super) fn register(app: &mut App) {
    use openzt2_game_assets::application_lifecycle::GamePhase;
    app.add_message::<ScenarioUiCommand>()
        .add_message::<RunTimedScenarioUiAction>()
        .add_message::<TimedScenarioUiActionCompleted>()
        .add_message::<openzt2_zoo_state::scenario::timed_scenario_ui_actions::ScenarioUiSelectionCompleted>()
        .add_message::<presentation::ScenarioConstructionRequest>()
        .register_required_components::<
            openzt2_world_state::world_spawn::world_membership_types::WorldRoot,
            openzt2_interface_state::information::catalogue_types::ScenarioDisabledCatalogueDefinitions,
        >()
        .add_systems(
            Update,
            (
                create_named_ui_lua_contexts_for_requested_scripts,
                run_authored_timed_scenario_scripts,
                presentation::apply_scenario_ui_commands,
                presentation::apply_scenario_tool_requests,
            )
                .chain()
                .in_set(openzt2_interface_state::ui::UiSet::DomainProjection),
        )
        .add_systems(
            Update,
            (
                presentation::fit_animation_heights_to_text,
                presentation::apply_scenario_tank_requests,
                presentation::apply_scenario_zoo_messages,
                presentation::apply_scenario_tutorial_status,
            ).in_set(openzt2_interface_state::ui::UiSet::DomainProjection),
        )
        .add_systems(Update, presentation::preload_scenario_ui_documents
            .in_set(openzt2_world_state::application_schedule::GameSet::Intent)
            .run_if(in_state(GamePhase::InGame)))
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
                commands.borrow_mut().push(ScenarioUiCommand::Node {
                    path: vec![name],
                    operation: NodeOperation::Visible(visible),
                });
                Ok(())
            })?,
        )?;
    }
    Ok(())
}

/// A message nothing handles does nothing, as for any other receiver, so the
/// rest of the script still runs. Each distinct one is reported once.
pub(super) fn warn_unhandled_scenario_message(message: &str) {
    thread_local! {
        static REPORTED: RefCell<std::collections::HashSet<String>> = RefCell::default();
    }
    if REPORTED.with_borrow_mut(|reported| reported.insert(message.to_owned())) {
        bevy::log::warn!(message, "scenario script sent a message nothing handles");
    }
}

fn authored_type_id(name: &str) -> AssetId {
    AssetId::from_key(&openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(name))
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
            commands.borrow_mut().push(ScenarioUiCommand::Node {
                path: Vec::new(),
                operation: NodeOperation::SelectionMode,
            });
            Ok(())
        }
        // The tool follows from the object or fence named next.
        "mode_placement" | "mode_fence_placement" => Ok(()),
        "mode_biome" => {
            commands.borrow_mut().push(ScenarioUiCommand::Tool(
                openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool::Biome,
            ));
            Ok(())
        }
        unhandled => {
            warn_unhandled_scenario_message(&format!("ZT_SETMODE {unhandled}"));
            Ok(())
        }
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
                            .then(|| authored_type_id(&value.to_string_lossy()));
                        if let Some(definition) = placement.borrow().0 {
                            commands
                                .borrow_mut()
                                .push(ScenarioUiCommand::PlacementObject(definition));
                        }
                    }
                    "ZT_SETPLACEMENTFENCE" => {
                        let mlua::Value::String(value) = value else {
                            return Err(mlua::Error::runtime("fence type must be a string"));
                        };
                        commands.borrow_mut().push(ScenarioUiCommand::PlacementObject(
                            authored_type_id(&value.to_string_lossy()),
                        ));
                    }
                    "ZT_SETFENCEPLACEMENTMODE" => {
                        use openzt2_zoo_state::scenario::scenario_session_types::SetScenarioFencePlacementMode as Mode;
                        if let mlua::Value::String(value) = value {
                            let mode = match value.to_str()?.as_ref() {
                                "rubber-band" => Some(Mode::RubberBand),
                                "rectangle" => Some(Mode::Rectangle),
                                "octagon" => Some(Mode::Octagon),
                                "gate" => Some(Mode::Gate),
                                _ => None,
                            };
                            if let Some(mode) = mode {
                                commands.borrow_mut().push(ScenarioUiCommand::FencePlacementMode(mode));
                            }
                        }
                    }
                    "ZT_IS_TYPE_ON_CURSOR" => {
                        let mlua::Value::String(value) = value else {
                            return Err(mlua::Error::runtime("cursor type must be a string"));
                        };
                        let wanted = authored_type_id(&value.to_string_lossy());
                        return Ok(mlua::Value::Boolean(queries.construction_tool.is_some_and(
                            |tool| {
                                matches!(
                                    tool,
                                    openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool::Place(definition)
                                    | openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool::Fence(definition)
                                    | openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool::Path(definition)
                                    if definition == wanted
                                )
                            },
                        )));
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
                        // Spawning directly leaves the player's tool alone.
                        commands.borrow_mut().retain(|command| {
                            !matches!(command, ScenarioUiCommand::PlacementObject(definition) if *definition == kind)
                        });
                        let request = spawn(
                            kind,
                            [point.get("x")?, point.get("y")?, point.get("z")?],
                            rotation,
                            &entities.borrow().spawns,
                        )?;
                        entities.borrow_mut().spawns.push(request);
                    }
                    unhandled => warn_unhandled_scenario_message(unhandled),
                }
                Ok(mlua::Value::Nil)
            },
        )?,
    )?;
    Ok(mode)
}

pub(super) fn create_ui_root<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    commands: &'env RefCell<Vec<ScenarioUiCommand>>,
    queries: ScenarioLuaQueryFacts<'env>,
) -> mlua::Result<Table> {
    install_node_methods(lua, scope, commands, queries)?;
    let root = lua.create_table()?;
    root.set(
        "UI_GET_CHILD",
        lua.create_function(|lua, (_, name): (Table, String)| create_node(lua, &[name]))?,
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
    queries.ui_roots.iter().find_map(|(root_entity, root)| {
        let document = queries
            .ui_documents
            .get(&root.document)?
            .canonical_ui_document();
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
            highlighted: interaction.is_some_and(|hovered| hovered.get()),
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
    node.set(
        "__path",
        lua.create_sequence_from(path.iter().map(String::as_str))?,
    )?;
    let metatable = match lua
        .globals()
        .get::<Option<Table>>("__openzt2_ui_node_metatable")?
    {
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
            lua.globals()
                .set("__openzt2_ui_node_metatable", metatable.clone())?;
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
    lua.globals()
        .set("__openzt2_ui_node_methods", node.clone())?;
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
            Ok(
                locate_ui_node(queries, &node_path(&node)?)
                    .is_some_and(|node| node.lineage_visible),
            )
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
    for (method, selected) in [("UI_IS_ALTERNATE", true), ("UI_IS_ACTIVATED", true)] {
        node.set(
            method,
            scope.create_function(move |_, node: Table| {
                Ok(
                    locate_ui_node(queries, &node_path(&node)?).is_some_and(|node| node.selected)
                        == selected,
                )
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
            rectangle_table(
                lua,
                [
                    authored.rect[0],
                    authored.rect[1],
                    node.size[0],
                    node.size[1],
                ],
            )
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
            let key = commands
                .borrow()
                .iter()
                .rev()
                .find_map(|command| match command {
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
        node.set(
            method,
            lua.create_function(|_, _: mlua::MultiValue| Ok(()))?,
        )?;
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
            push(
                node_path(&node)?,
                NodeOperation::Position(read_point(&point)?),
            );
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
            push(
                node_path(&node)?,
                NodeOperation::IconKey(AssetId::from_key(&key)),
            );
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
            let fit_text = commands
                .borrow()
                .iter()
                .rev()
                .find_map(|command| match command {
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

#[derive(Component)]
struct NamedScenarioUiLuaContext(AssetId);

#[derive(Component)]
#[relationship(relationship_target = NamedScenarioUiLuaContexts)]
struct NamedScenarioUiLuaContextOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = NamedScenarioUiLuaContextOf, linked_spawn)]
struct NamedScenarioUiLuaContexts(Vec<Entity>);

#[derive(bevy::ecs::system::SystemParam)]
struct ScenarioUiLuaContextOwners<'w, 's> {
    documents: Query<'w, 's, (&'static UiDocumentRoot, &'static ChildOf)>,
    worlds: Query<
        'w,
        's,
        Entity,
        With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>,
    >,
    owned: Query<'w, 's, &'static NamedScenarioUiLuaContexts>,
    names: Query<'w, 's, &'static NamedScenarioUiLuaContext>,
}

impl ScenarioUiLuaContextOwners<'_, '_> {
    fn owner(&self, document: Entity) -> Option<(&UiDocumentRoot, Entity)> {
        let (root, parent) = self.documents.get(document).ok()?;
        Some((
            root,
            self.worlds.single().unwrap_or_else(|_| parent.parent()),
        ))
    }

    fn find(&self, owner: Entity, name: AssetId) -> Option<Entity> {
        use bevy::ecs::relationship::RelationshipTarget;
        self.owned.get(owner).ok()?.iter().find(|entity| {
            self.names
                .get(*entity)
                .is_ok_and(|context| context.0 == name)
        })
    }
}

fn create_named_ui_lua_contexts_for_requested_scripts(
    mut commands: Commands,
    mut requests: MessageReader<RunTimedScenarioUiAction>,
    owners: ScenarioUiLuaContextOwners,
) {
    let mut created = std::collections::HashSet::new();
    for request in requests.read() {
        let mut words = request.command.split_ascii_whitespace();
        let (Some(name), Some(_), Some(_)) = (words.next(), words.next(), words.next()) else {
            continue;
        };
        if name.eq_ignore_ascii_case("scenario") {
            continue;
        }
        let Some((_, owner)) = owners.owner(request.document_root) else {
            continue;
        };
        let name = AssetId::from_virtual_path(name);
        if owners.find(owner, name).is_none() && created.insert((owner, name)) {
            commands.spawn((
                NamedScenarioUiLuaContext(name),
                NamedScenarioUiLuaContextOf(owner),
                ScenarioLuaContext::default(),
            ));
        }
    }
}

fn run_authored_timed_scenario_scripts(
    mut requests: MessageReader<RunTimedScenarioUiAction>,
    owners: ScenarioUiLuaContextOwners,
    active_scenarios: Query<(Entity, &ActiveScenarioSession, &ScenarioDocument)>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    mut runtime: Query<&mut ScenarioLuaContext>,
    (fact_sources, direct): (
        ScenarioLuaQueryFactSources,
        super::scenario_lua_entity_commands::ScenarioLuaDirectAccess,
    ),
    mut outputs: super::scenario_lua_command_publication::ScenarioLuaOutputs,
    mut completed: MessageWriter<TimedScenarioUiActionCompleted>,
) {
    let contained = |entity| fact_sources.contained_entities(entity);
    let direct = std::cell::RefCell::new(direct);
    let waste = |entity, value| direct.borrow_mut().waste(entity, value);
    let profile = |key, value: Option<Option<&str>>| direct.borrow_mut().profile(key, value);
    let mut queries = fact_sources.borrow_scenario_lua_query_facts(&contained);
    queries.waste = Some(&waste);
    queries.profile = Some(&profile);
    for request in requests.read() {
        let delivered = (|| {
            let Some((root, view_owner)) = owners.owner(request.document_root) else {
                return false;
            };
            let mut words = request.command.split_ascii_whitespace();
            let (Some(context), Some(path), Some(entry)) =
                (words.next(), words.next(), words.next())
            else {
                warn!(command = %request.command, "unsupported timed scenario script command");
                return false;
            };
            let referring_path = root
                .document
                .path()
                .map(|path| path.path().to_string_lossy());
            let Some((script_path, script_handle)) =
                script_index.resolve(referring_path.as_deref().unwrap_or(""), path)
            else {
                warn!(%path, "timed UI script does not resolve");
                return false;
            };
            let (owner, scenario, document_id) = if context.eq_ignore_ascii_case("scenario") {
                let Ok((owner, active, document)) = active_scenarios.single() else {
                    return false;
                };
                (owner, active.definition, Some(document.0.id()))
            } else {
                let context = AssetId::from_virtual_path(context);
                let Some(owner) = owners.find(view_owner, context) else {
                    return false;
                };
                (owner, context, None)
            };
            let Ok(mut context) = runtime.get_mut(owner) else {
                return false;
            };
            let arguments = words.collect::<Vec<_>>();
            match context.execute(
                document_id,
                script_path,
                script_handle,
                &scripts,
                &script_index,
                &ScenarioScriptBinding {
                    scenario,
                    objective: AssetId::default(),
                    script: script_path.to_owned(),
                    entry: entry.to_owned(),
                    phase: ScenarioScriptPhase::AlwaysRun,
                },
                queries,
                Some(&arguments),
            ) {
                Ok(result) => {
                    if let Some(failure) = &result.failure {
                        error!(%failure, %entry, "timed scenario script failed");
                    }
                    outputs.publish(queries.world.unwrap_or(request.document_root), result);
                }
                Err(failure) => error!(%failure, %entry, "timed scenario script failed"),
            }
            true
        })();
        if delivered {
            outputs.write_timed_boundary(ScenarioUiCommand::Node {
                path: Vec::new(),
                operation: NodeOperation::ScriptCompleted(request.action),
            });
        } else {
            completed.write(TimedScenarioUiActionCompleted(request.action));
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScenarioUiSavedState {
    pub(crate) enabled: std::collections::BTreeMap<String, bool>,
    #[serde(default)]
    pub(crate) shown: std::collections::BTreeMap<String, bool>,
    pub(crate) continuation: Option<AssetId>,
}

impl ScenarioUiSavedState {
    fn capture_override(&mut self, node: &presentation::ScenarioUiNodeOverride) {
        if let Some(enabled) = node.enabled {
            self.enabled.insert(node.name.clone(), enabled);
        }
        if let Some(shown) = node.shown {
            self.shown.insert(node.name.clone(), shown);
        }
    }

    fn spawn_overrides(mut self, commands: &mut Commands, owner: Entity) {
        for (name, enabled) in self.enabled {
            let shown = self.shown.remove(&name);
            commands.spawn((
                presentation::ScenarioUiNodeOverrideOf(owner),
                presentation::ScenarioUiNodeOverride {
                    name,
                    enabled: Some(enabled),
                    shown,
                },
            ));
        }
        for (name, shown) in self.shown {
            commands.spawn((
                presentation::ScenarioUiNodeOverrideOf(owner),
                presentation::ScenarioUiNodeOverride {
                    name,
                    enabled: None,
                    shown: Some(shown),
                },
            ));
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ScenarioUiSnapshotCapture<'w, 's> {
    pending: Query<
        'w,
        's,
        (
            Option<&'static presentation::ScenarioUiState>,
            Option<&'static presentation::ScenarioUiNodeOverrides>,
        ),
        With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>,
    >,
    overrides: Query<'w, 's, &'static presentation::ScenarioUiNodeOverride>,
}

impl ScenarioUiSnapshotCapture<'_, '_> {
    pub(crate) fn capture(&self) -> ScenarioUiSavedState {
        use bevy::ecs::relationship::RelationshipTarget;
        let mut saved = ScenarioUiSavedState::default();
        if let Ok((pending, overrides)) = self.pending.single() {
            saved.continuation = pending.and_then(|state| state.continuation);
            if let Some(overrides) = overrides {
                for node in self.overrides.iter_many(overrides.iter()).flatten() {
                    saved.capture_override(node);
                }
            }
        }
        saved
    }
}

pub(crate) fn restore_saved_scenario_ui(
    commands: &mut Commands,
    root: Entity,
    saved: ScenarioUiSavedState,
) {
    let mut state = presentation::ScenarioUiState::default();
    state.continuation = saved.continuation;
    commands
        .entity(root)
        .despawn_related::<presentation::ScenarioUiNodeOverrides>()
        .insert(state);
    saved.spawn_overrides(commands, root);
}
