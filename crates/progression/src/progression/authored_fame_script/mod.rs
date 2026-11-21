use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
};

use super::zoo_rating_input_types::{GuestRatingFacts, LivingAnimalPopulation};
use bevy::prelude::*;
use mlua::{Function, Lua, MultiValue, Scope, Table, Value};
use openzt2_game_assets::lua_script::{
    lua_script_archive_path_index::LuaScriptArchivePathIndex,
    lua_script_asset_loading::LuaScriptAsset,
    lua_script_module_execution::{
        execute_lua_script_module_once_per_virtual_machine,
        install_lua_script_dependency_loading_functions,
    },
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::{
    DefinitionId, WorldMember, WorldRoot,
};
use openzt2_zoo_state::progression::{
    fame_types::{Fame, FameAuthoredPresentation, FameGateAppearanceRequested},
    profile_scenario_variables::ProfileScenarioVariables,
};

const FAME_SCRIPT_PATH: &str = "scripts/ZooFame.lua";

#[derive(Component)]
#[require(FameAuthoredPresentation)]
pub(super) struct AuthoredFameScript {
    handle: Handle<LuaScriptAsset>,
    path: String,
    lua: Option<Lua>,
    modules: BTreeSet<bevy::asset::AssetId<LuaScriptAsset>>,
}

pub(super) fn resolve_winning_fame_script(
    mut commands: Commands,
    index: Res<LuaScriptArchivePathIndex>,
    mut asset_events: MessageReader<AssetEvent<LuaScriptAsset>>,
    mut roots: Query<(Entity, Option<&mut AuthoredFameScript>), With<WorldRoot>>,
) {
    let changed = asset_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let winner = index.resolve("", FAME_SCRIPT_PATH);
    for (root, existing) in &mut roots {
        match (winner, existing) {
            (Some((path, handle)), Some(mut current)) => {
                if current.handle != *handle || current.path != path {
                    current.path = path.to_owned();
                    current.handle = handle.clone();
                    current.lua = None;
                    current.modules.clear();
                } else if changed.contains(&handle.id()) || !changed.is_disjoint(&current.modules) {
                    current.lua = None;
                    current.modules.clear();
                }
            }
            (Some((path, handle)), None) => {
                commands.entity(root).insert(AuthoredFameScript {
                    handle: handle.clone(),
                    path: path.to_owned(),
                    lua: None,
                    modules: BTreeSet::new(),
                });
            }
            (None, Some(_)) => {
                commands
                    .entity(root)
                    .remove::<(AuthoredFameScript, FameAuthoredPresentation)>();
            }
            (None, None) => {}
        }
    }
}

#[derive(Default)]
pub(super) struct FameScriptOutput {
    pub(super) fame: Option<(u8, f32)>,
    pub(super) notices: Vec<(String, f64)>,
    pub(super) gate_changes: Vec<FameGateAppearanceRequested>,
    pub(super) presentation: FameAuthoredPresentation,
}

impl AuthoredFameScript {
    pub(super) fn evaluate(
        &mut self,
        index: &LuaScriptArchivePathIndex,
        assets: &Assets<LuaScriptAsset>,
        animals: &LivingAnimalPopulation,
        guests: &GuestRatingFacts,
        old: Fame,
        award_points: u32,
        scenario_points: u32,
        animals_released: u32,
        gates: &[(Entity, AssetId)],
        variables: Option<&ProfileScenarioVariables>,
    ) -> mlua::Result<FameScriptOutput> {
        if !assets.contains(self.handle.id()) {
            return Ok(FameScriptOutput::default());
        }
        let lua = self.lua.get_or_insert_with(Lua::new);
        let modules = RefCell::new(std::mem::take(&mut self.modules));
        let stack = RefCell::new(Vec::new());
        let output = RefCell::new(FameScriptOutput::default());
        let maximum = Cell::new(old.maximum_percent_reached);
        let result = lua.scope(|scope| {
            install_lua_script_dependency_loading_functions(
                lua, scope, assets, index, &modules, &stack,
            )?;
            install_fame_logging(lua)?;
            execute_lua_script_module_once_per_virtual_machine(
                lua,
                &self.path,
                &self.handle,
                assets,
                &modules,
                &stack,
            )?;
            stack.borrow_mut().push(self.path.clone());
            let globals = lua.globals();
            let status = if let Ok(fame_globals) = globals.get::<Table>("FameGlobals") {
                fame_globals.get::<Option<Table>>("status")?
            } else {
                None
            }
            .map_or_else(|| lua.create_table(), Ok)?;
            status.set("awardPoints", award_points)?;
            status.set("scenarioPoints", scenario_points)?;
            status.set("currentHalfStars", old.half_stars)?;
            status.set("fame", old.current_percent.unwrap_or_default())?;
            status.set(
                "ZT_GET_ANIMALS_RELEASED",
                lua.create_function(move |_, _: Table| Ok(animals_released))?,
            )?;
            let output_ref = &output;
            let maximum_ref = &maximum;
            status.set(
                "ZT_SET_FAME_HALFSTARS",
                scope.create_function(move |_, (_, result): (Table, Table)| {
                    let stars: u8 = result.get(1)?;
                    let percent: f32 = result.get(2)?;
                    if stars <= 10 && percent.is_finite() && (0.0..=100.0).contains(&percent) {
                        output_ref.borrow_mut().fame = Some((stars, percent));
                        maximum_ref.set(Some(
                            maximum_ref
                                .get()
                                .map_or(percent, |maximum| maximum.max(percent)),
                        ));
                    }
                    Ok(())
                })?,
            )?;
            status.set(
                "ZT_GET_MAX_FAME",
                scope.create_function(move |_, _: Table| {
                    Ok(maximum_ref.get().unwrap_or_default())
                })?,
            )?;
            install_fame_scene(
                lua, scope, &status, animals, guests, gates, variables, &output,
            )?;
            let calculate = globals.get::<Function>("adjustZooFame")?;
            calculate.call::<()>(status)
        });
        if result.is_ok() {
            self.modules = modules.into_inner();
        } else {
            // A failing include can leave globals from a half-loaded module.
            self.lua = None;
            self.modules.clear();
        }
        result.map(|()| output.into_inner())
    }
}

fn install_fame_logging(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let log = lua.create_function(|_, arguments: MultiValue| {
        for value in arguments {
            if let Value::String(message) = value {
                trace!(message = %message.to_string_lossy(), "fame script log");
            }
        }
        Ok(())
    })?;
    globals.set("BFLOG", log.clone())?;
    globals.set("LUALOG", log)?;
    globals.set("SYSNOTE", 1)?;
    globals.set("SYSERROR", 2)?;
    Ok(())
}

fn install_fame_scene<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    status: &Table,
    animals: &'env LivingAnimalPopulation,
    guests: &'env GuestRatingFacts,
    gates: &'env [(Entity, AssetId)],
    variables: Option<&'env ProfileScenarioVariables>,
    output: &'env RefCell<FameScriptOutput>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let animal_rows = lua.create_table()?;
    for (index, animal) in animals.observations.iter().enumerate() {
        let value = lua.create_table()?;
        let happiness = animal.happiness;
        value.set(
            "BFG_GET_ATTR_FLOAT",
            lua.create_function(move |_, (_, name): (Table, String)| {
                Ok((name == "f_needPointsGood").then_some(happiness))
            })?,
        )?;
        let species = animal.species.to_lowercase_hexadecimal_string();
        value.set(
            "BFG_GET_ATTR_STRING",
            lua.create_function(move |_, (_, name): (Table, String)| {
                Ok((name == "s_Species").then(|| species.clone()))
            })?,
        )?;
        let item = lua.create_table()?;
        item.set("value", value)?;
        animal_rows.set(index + 1, item)?;
    }
    let guest_rows = lua.create_table()?;
    for (index, guest) in guests.observations.iter().enumerate() {
        let value = lua.create_table()?;
        let (happiness, education) = (guest.happiness, guest.education);
        value.set(
            "BFG_GET_ATTR_FLOAT",
            lua.create_function(move |_, (_, name): (Table, String)| {
                Ok(match name.as_str() {
                    "happiness" => Some(happiness),
                    "f_Education" => Some(education),
                    _ => None,
                })
            })?,
        )?;
        let item = lua.create_table()?;
        item.set("value", value)?;
        guest_rows.set(index + 1, item)?;
    }
    let gate_rows = lua.create_table()?;
    for (index, &(gate, definition)) in gates.iter().enumerate() {
        let value = lua.create_table()?;
        value.set(
            "BFG_GET_BINDER_TYPE",
            lua.create_function(move |_, _: Table| {
                Ok([
                    "frontgate_df",
                    "frontgate_fame02",
                    "frontgate_fame03",
                    "frontgate_fame04",
                    "frontgate_fame05",
                ]
                .into_iter()
                .find(|key| AssetId::from_key(key) == definition))
            })?,
        )?;
        value.set(
            "BFG_ENTITY_MORPH_TO_NEW_ENTITY",
            scope.create_function(move |_, (_, args): (Table, Table)| {
                let binder: String = args.get(1)?;
                let definition = AssetId::from_key(&binder.to_ascii_lowercase());
                output
                    .borrow_mut()
                    .gate_changes
                    .push(FameGateAppearanceRequested { gate, definition });
                Ok(())
            })?,
        )?;
        let item = lua.create_table()?;
        item.set("value", value)?;
        gate_rows.set(index + 1, item)?;
    }
    let entities = lua.create_table()?;
    entities.set(
        "BFG_GET_ENTITIES",
        scope.create_function(move |_, (_, kind): (Table, String)| {
            Ok(kind
                .eq_ignore_ascii_case("entrance")
                .then(|| gate_rows.clone()))
        })?,
    )?;
    let ui = create_fame_ui_root(lua, scope, output)?;
    let status = status.clone();
    globals.set(
        "queryObject",
        scope.create_function(move |_, name: String| {
            Ok(match name.as_str() {
                "ZTStatus" => Some(status.clone()),
                "BFGManager" => Some(entities.clone()),
                "UIRoot" => Some(ui.clone()),
                _ => None,
            })
        })?,
    )?;
    globals.set(
        "findType",
        scope.create_function(move |lua, name: String| {
            if name.eq_ignore_ascii_case("animal") {
                Ok(animal_rows.clone())
            } else if name.eq_ignore_ascii_case("guest") {
                Ok(guest_rows.clone())
            } else {
                lua.create_table()
            }
        })?,
    )?;
    globals.set(
        "resolveTable",
        lua.create_function(|_, value: Value| Ok(value))?,
    )?;
    globals.set(
        "getSpeciesSeenData",
        scope.create_function(move |lua, _: Table| {
            let result = lua.create_table()?;
            for (species, seen) in &guests.species_seen {
                let record = lua.create_table()?;
                record.set("guestViews", seen.guests)?;
                record.set("viewEvents", seen.events)?;
                record.set("entertain", seen.entertainment)?;
                record.set("donate", seen.donation)?;
                result.set(species.to_lowercase_hexadecimal_string(), record)?;
            }
            Ok(result)
        })?,
    )?;
    globals.set(
        "displayZooMessage",
        scope.create_function(move |_, (key, _, timeout): (String, Value, Option<f64>)| {
            output
                .borrow_mut()
                .notices
                .push((key, timeout.unwrap_or_default()));
            Ok(())
        })?,
    )?;
    globals.set(
        "getglobalvar",
        scope.create_function(move |_, key: String| {
            Ok(variables
                .and_then(|variables| variables.get(AssetId::from_key(&key)))
                .map(str::to_owned))
        })?,
    )?;
    Ok(())
}

fn create_fame_ui_root<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    output: &'env RefCell<FameScriptOutput>,
) -> mlua::Result<Table> {
    let methods = lua.create_table()?;
    methods.set("__index", methods.clone())?;
    methods.set(
        "UI_GET_CHILD",
        lua.create_function(|lua, (parent, name): (Table, String)| {
            let child = lua.create_table()?;
            let path = parent.get::<String>("path")?;
            child.set("path", format!("{path}/{name}"))?;
            child.set_metatable(parent.metatable())?;
            Ok(child)
        })?,
    )?;
    methods.set(
        "UI_SET_SIZE",
        scope.create_function(move |_, (node, size): (Table, Table)| {
            let path = node.get::<String>("path")?;
            if let Some(name) = path
                .strip_prefix("/zoo fame tab layout/")
                .and_then(|path| path.strip_suffix("/bar"))
            {
                let width: f32 = size.get("x")?;
                if width.is_finite() {
                    output.borrow_mut().presentation.bars.insert(
                        AssetId::from_key(&name.to_ascii_lowercase()),
                        width.max(0.0),
                    );
                }
            }
            Ok(())
        })?,
    )?;
    methods.set(
        "UI_SET_VALUE",
        scope.create_function(move |_, (node, value): (Table, String)| {
            if node
                .get::<String>("path")?
                .eq_ignore_ascii_case("/Zoo Gate")
            {
                output.borrow_mut().presentation.gate_icon =
                    Some(AssetId::from_key(&value.to_ascii_lowercase()));
            }
            Ok(())
        })?,
    )?;
    let root = lua.create_table()?;
    root.set("path", "")?;
    root.set_metatable(Some(methods))?;
    Ok(root)
}

pub(super) fn entrance_gate_definitions(
    world: Entity,
    gates: &Query<
        (Entity, &WorldMember, &DefinitionId),
        With<openzt2_world_state::world_spawn::zoo_entrance_anchor_synchronization::ZooEntrance>,
    >,
) -> Vec<(Entity, AssetId)> {
    gates
        .iter()
        .filter(|(_, member, _)| member.root == world)
        .map(|(gate, _, definition)| (gate, definition.0))
        .collect()
}
