//! The `BFGManager` entity queries that shipped scenario helpers make through
//! `findType`, `getSelectedEntity` and `resolveTable`.
//!
//! Each live entity is a component table. Only facts the live world owns are
//! answered; any other type, attribute or selection containment is an error.

use mlua::{Lua, Scope, Table, Value};

use super::scenario_lua_types::ScenarioLuaQueryFacts;

/// Field identifying a component table created by this adapter.
const ENTITY_FIELD: &str = "__openzt2_entity";
/// Field holding whether this animal is pregnant.
const PREGNANT_FIELD: &str = "__openzt2_pregnant";

/// Creates the `BFGManager` object and installs the global `resolveTable`.
pub(super) fn create_scenario_lua_entity_manager<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    queries: ScenarioLuaQueryFacts<'env>,
) -> mlua::Result<Table> {
    lua.globals().set(
        "resolveTable",
        lua.create_function(|_, value: Value| match value {
            Value::Table(entity) if entity.contains_key(ENTITY_FIELD)? => Ok(entity),
            _ => Err(mlua::Error::runtime(
                "resolveTable requires an entity reference from BFG_GET_ENTITIES",
            )),
        })?,
    )?;
    let read_entity_boolean_attribute = lua.create_function(
        |_, (entity, attribute): (Table, String)| match attribute.as_str() {
            "b_Pregnant" => entity.get::<bool>(PREGNANT_FIELD),
            "b_MarineAnimal" => Err(mlua::Error::runtime(
                "BFG_GET_ATTR_BOOLEAN b_MarineAnimal: the species catalogue does not record \
                 which species are marine",
            )),
            _ => Err(mlua::Error::runtime(format!(
                "BFG_GET_ATTR_BOOLEAN does not expose {attribute}"
            ))),
        },
    )?;
    let manager = lua.create_table()?;
    manager.set(
        "BFG_GET_ENTITIES",
        scope.create_function(move |lua, (_, entity_type): (Table, String)| {
            if entity_type != "animal" {
                return Err(mlua::Error::runtime(format!(
                    "BFG_GET_ENTITIES does not expose {entity_type}"
                )));
            }
            let mut animals = queries.live_world_animals.iter().collect::<Vec<_>>();
            animals.sort_unstable_by_key(|(entity, _)| *entity);
            let entities = lua.create_table()?;
            for (index, (entity, pregnant)) in animals.into_iter().enumerate() {
                let component = lua.create_table()?;
                component.set(ENTITY_FIELD, entity.to_bits())?;
                component.set(PREGNANT_FIELD, pregnant)?;
                component.set(
                    "BFG_GET_ATTR_BOOLEAN",
                    read_entity_boolean_attribute.clone(),
                )?;
                let reference = lua.create_table()?;
                reference.set("value", component)?;
                entities.set(index + 1, reference)?;
            }
            Ok(entities)
        })?,
    )?;
    manager.set(
        "BFG_GET_SELECTED_ENTITY",
        scope.create_function(move |_, _: Table| match queries.selected_entity {
            None => Ok(Value::Nil),
            // `findType` adds an animal held by the selection, such as one in
            // a crate. Which live entities contain an animal is not modelled.
            Some(_) => Err(mlua::Error::runtime(
                "BFG_GET_SELECTED_ENTITY cannot describe the selection's contained entities",
            )),
        })?,
    )?;
    Ok(manager)
}
