use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
};

use bevy::{ecs::{entity_disabling::Disabled, query::Allow, relationship::RelationshipTarget, system::SystemParam}, prelude::*};
use mlua::{Function, Lua, MultiValue, Scope, Table, Value};
use openzt2_game_assets::lua_script::{
    lua_script_asset_loading::LuaScriptAsset,
    lua_script_module_execution::{
        execute_lua_script_module_once_per_virtual_machine,
        install_lua_script_dependency_loading_functions,
    },
    lua_script_sources::LuaScriptSources,
};
use openzt2_game_assets::asset_source::ArchiveOverlayChanged;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::{
    DefinitionId, WorldMembers, WorldRoot,
};
use openzt2_zoo_state::progression::{
    fame_types::{Fame, FameAuthoredPresentation, FameGateAppearanceRequested, GuestSpeciesFameViews, ZooSpeciesFameViews},
    profile_scenario_variables::ProfileScenarioVariables,
};

use openzt2_world_state::world_spawn::zoo_entrance_anchor_synchronization::ZooEntrance;
use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::{Animal, SpeciesHandle},
    animal_welfare::types::NeedPoints,
    guests::guest_simulation_types::{Guest, GuestEducation, GuestSatisfaction},
    progression::fame_types::PendingFameGatePrefab,
};

#[derive(SystemParam)]
pub(super) struct FameEntityFacts<'w, 's> {
    animals: Query<
        'w,
        's,
        (&'static SpeciesHandle, Option<&'static NeedPoints>),
        (With<Animal>, Without<Dead>),
    >,
    guests: Query<
        'w,
        's,
        (
            Option<&'static GuestSatisfaction>,
            Option<&'static GuestEducation>,
        ),
        With<Guest>,
    >,
    guest_views: Query<'w, 's, &'static GuestSpeciesFameViews, With<Guest>>,
    gates: Query<
        'w,
        's,
        (
            Entity,
            &'static DefinitionId,
            Option<&'static PendingFameGatePrefab>,
        ),
        (With<ZooEntrance>, Allow<Disabled>),
    >,
}

const FAME_SCRIPT_PATH: &str = "scripts/ZooFame.lua";

#[derive(Component)]
#[require(FameAuthoredPresentation)]
pub(super) struct AuthoredFameScript {
    handle: Handle<LuaScriptAsset>,
    path: String,
    lua: Option<Lua>,
    modules: BTreeSet<AssetId>,
}

pub(super) fn resolve_winning_fame_script(
    mut commands: Commands,
    sources: Res<LuaScriptSources>,
    server: Res<AssetServer>,
    mut asset_events: MessageReader<AssetEvent<LuaScriptAsset>>,
    mut overlay: MessageReader<ArchiveOverlayChanged>,
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
    let changed_paths = changed
        .iter()
        .filter_map(|&id| server.get_path(id))
        .map(|path| AssetId::from_virtual_path(&path.path().to_string_lossy()))
        .chain(
            overlay
                .read()
                .flat_map(|event| &event.changes)
                .map(|entry| {
                    AssetId::from_virtual_path(&entry.normalized_asset_path.to_string_lossy())
                }),
        )
        .collect::<BTreeSet<_>>();
    let winner = sources.load("", FAME_SCRIPT_PATH);
    for (root, existing) in &mut roots {
        match (winner.as_ref(), existing) {
            (Some((path, handle)), Some(mut current)) => {
                if current.handle != *handle || current.path != *path {
                    current.path.clone_from(path);
                    current.handle = handle.clone();
                    current.lua = None;
                    current.modules.clear();
                } else if changed.contains(&handle.id())
                    || !changed_paths.is_disjoint(&current.modules)
                {
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
    pub(super) effects: Vec<FameScriptEffect>,
    pub(super) presentation: FameAuthoredPresentation,
}

pub(super) enum FameScriptEffect {
    Fame(u8, f32),
    Notice(String, f64),
    Gate(FameGateAppearanceRequested),
}

impl AuthoredFameScript {
    pub(super) fn evaluate(
        &mut self,
        sources: &LuaScriptSources,
        assets: &Assets<LuaScriptAsset>,
        views: Option<&ZooSpeciesFameViews>,
        old: Fame,
        award_points: u32,
        scenario_points: u32,
        animals_released: u32,
        entities: &FameEntityFacts,
        members: Option<&WorldMembers>,
        definitions: WorldDefinitionsView<'_>,
        variables: Option<&ProfileScenarioVariables>,
        award_globals: Option<&super::AwardGlobalVariables>,
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
                lua, scope, assets, sources, &modules, &stack,
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
                        output_ref
                            .borrow_mut()
                            .effects
                            .push(FameScriptEffect::Fame(stars, percent));
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
                lua,
                scope,
                &status,
                views,
                entities,
                members,
                definitions,
                variables,
                award_globals,
                &output,
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
    views: Option<&'env ZooSpeciesFameViews>,
    facts: &'env FameEntityFacts<'_, '_>,
    members: Option<&'env WorldMembers>,
    definitions: WorldDefinitionsView<'env>,
    variables: Option<&'env ProfileScenarioVariables>,
    award_globals: Option<&'env super::AwardGlobalVariables>,
    output: &'env RefCell<FameScriptOutput>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let methods = lua.create_table()?;
    methods.set("__index", methods.clone())?;
    methods.set(
        "BFG_GET_ATTR_FLOAT",
        scope.create_function(move |_, (row, name): (Table, String)| {
            let entity = Entity::try_from_bits(row.get::<i64>("entity")? as u64);
            Ok(entity.and_then(|entity| match name.as_str() {
                "f_needPointsGood" => facts
                    .animals
                    .get(entity)
                    .ok()
                    .map(|(_, need)| need.map_or(0.0, |need| need.good)),
                "happiness" => facts.guests.get(entity).ok().map(|(value, _)| {
                    value.map_or(0.0, |value| value.source_q16() as f32 / 65_536.0)
                }),
                "f_Education" => facts
                    .guests
                    .get(entity)
                    .ok()
                    .map(|(_, value)| value.map_or(0.0, |value| f32::from(value.0) / 10.0)),
                _ => None,
            }))
        })?,
    )?;
    methods.set(
        "BFG_GET_ATTR_STRING",
        scope.create_function(move |_, (row, name): (Table, String)| {
            let entity = Entity::try_from_bits(row.get::<i64>("entity")? as u64);
            Ok(entity.filter(|_| name == "s_Species").and_then(|entity| {
                facts
                    .animals
                    .get(entity)
                    .ok()
                    .map(|(species, _)| species.species.to_lowercase_hexadecimal_string())
            }))
        })?,
    )?;
    let gate_methods = lua.create_table()?;
    gate_methods.set("__index", gate_methods.clone())?;
    gate_methods.set(
        "BFG_GET_BINDER_TYPE",
        lua.create_function(|_, row: Table| row.get::<Option<String>>("binder"))?,
    )?;
    gate_methods.set(
        "BFG_ENTITY_MORPH_TO_NEW_ENTITY",
        scope.create_function(move |_, (row, args): (Table, Table)| {
            let Some(gate) = Entity::try_from_bits(row.get::<i64>("entity")? as u64) else {
                return Ok(());
            };
            let requested: String = args.get(1)?;
            let definition = AssetId::from_key(&requested.to_ascii_lowercase());
            row.set("binder", requested)?;
            output
                .borrow_mut()
                .effects
                .push(FameScriptEffect::Gate(FameGateAppearanceRequested {
                    gate,
                    definition,
                }));
            Ok(())
        })?,
    )?;
    let entities = lua.create_table()?;
    entities.set(
        "BFG_GET_ENTITIES",
        scope.create_function(move |lua, (_, kind): (Table, String)| {
            if !kind.eq_ignore_ascii_case("entrance") {
                return Ok(None);
            }
            let rows = lua.create_table()?;
            for (index, (gate, definition, pending)) in facts
                .gates
                .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
                .flatten()
                .enumerate()
            {
                let binder = definitions
                    .object_binder_type(pending.map_or(definition.0, |pending| pending.definition));
                append_fame_entity(lua, &rows, &gate_methods, index, gate, binder)?;
            }
            Ok(Some(rows))
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
            let rows = lua.create_table()?;
            let animal = name.eq_ignore_ascii_case("animal");
            let guest = name.eq_ignore_ascii_case("guest");
            for (index, entity) in members
                .into_iter()
                .flat_map(RelationshipTarget::iter)
                .filter(|&entity| {
                    animal && facts.animals.contains(entity)
                        || guest && facts.guests.contains(entity)
                })
                .enumerate()
            {
                append_fame_entity(lua, &rows, &methods, index, entity, None)?;
            }
            Ok(rows)
        })?,
    )?;
    globals.set(
        "resolveTable",
        lua.create_function(|_, value: Value| Ok(value))?,
    )?;
    let view_row_methods = lua.create_table()?;
    view_row_methods.set(
        "__index",
        scope.create_function(move |_, (row, key): (Table, String)| {
            if key != "guestViews" {
                return Ok(None);
            }
            let id = row.raw_get::<mlua::String>("_species")?;
            let Ok(bytes) = <[u8; 16]>::try_from(id.as_bytes().as_ref()) else {
                return Ok(None);
            };
            let species = AssetId(bytes);
            let count = facts
                .guest_views
                .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
                .flatten()
                .filter(|view| view.0.get(&species).is_some_and(|view| view.events != 0))
                .count();
            row.raw_set("guestViews", count)?;
            Ok(Some(count))
        })?,
    )?;
    globals.set(
        "getSpeciesSeenData",
        scope.create_function(move |lua, _: Table| {
            let result = lua.create_table()?;
            for (species, seen) in views.into_iter().flat_map(|views| views.0.iter()) {
                let record = lua.create_table()?;
                record.set("_species", lua.create_string(species.0)?)?;
                record.set_metatable(Some(view_row_methods.clone()))?;
                record.set("viewEvents", seen.events)?;
                record.set("entertain", seen.entertainment)?;
                record.set("donate", seen.donation)?;
                result.set(species.to_lowercase_hexadecimal_string(), record)?;
            }
            let count = views.map_or(0, |views| views.0.len());
            let collection = lua.create_table()?;
            collection.set("__len", lua.create_function(move |_, _: Table| Ok(count))?)?;
            result.set_metatable(Some(collection))?;
            Ok(result)
        })?,
    )?;
    globals.set(
        "displayZooMessage",
        scope.create_function(move |_, (key, _, timeout): (String, Value, Option<f64>)| {
            output
                .borrow_mut()
                .effects
                .push(FameScriptEffect::Notice(key, timeout.unwrap_or_default()));
            Ok(())
        })?,
    )?;
    globals.set(
        "getglobalvar",
        scope.create_function(move |_, key: String| {
            let key = AssetId::from_key(&key);
            Ok(award_globals
                .and_then(|globals| globals.0.get(&key).map(String::as_str))
                .or_else(|| variables.and_then(|variables| variables.get(key)))
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

fn append_fame_entity(
    lua: &Lua,
    rows: &Table,
    methods: &Table,
    index: usize,
    entity: Entity,
    binder: Option<&str>,
) -> mlua::Result<()> {
    let value = lua.create_table()?;
    value.set("entity", entity.to_bits() as i64)?;
    value.set("binder", binder)?;
    value.set_metatable(Some(methods.clone()))?;
    let item = lua.create_table()?;
    item.set("value", value)?;
    rows.set(index + 1, item)
}

#[cfg(test)]
mod tests;
