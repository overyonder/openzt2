//! Component tables project live entities and the canonical creations reserved by
//! the current invocation. Every reference retains its actual ECS identity.
use super::{
    scenario_lua_entity_commands::{PendingEntityOperations, ScenarioEntityCommand},
    scenario_lua_types::{ScenarioLuaLiveEntityQuery, ScenarioLuaQueryFacts},
};
use bevy::prelude::Entity;
use mlua::{Lua, Scope, Table, Value};
use openzt2_game_data::{
    species::{ConservationStatus, LifeStage, NeedKind, SpeciesFlags},
    AssetId,
};
use openzt2_zoo_state::animal_welfare::types::{SetAnimalNeed, MAX_NEED_Q16, Q16_ONE};

pub(super) const ENTITY_FIELD: &str = "__openzt2_entity";
pub(super) const PERSISTENT_ENTITY_FIELD: &str = "__openzt2_persistent_entity";
const NEEDS: [(&str, NeedKind); 11] = [
    ("hunger", NeedKind::Hunger),
    ("thirst", NeedKind::Thirst),
    ("rest", NeedKind::Rest),
    ("privacy", NeedKind::Privacy),
    ("social", NeedKind::Social),
    ("exercise", NeedKind::Exercise),
    ("stimulation", NeedKind::Stimulation),
    ("environment", NeedKind::Environment),
    ("health", NeedKind::Health),
    ("hygiene", NeedKind::Hygiene),
    ("bathroom", NeedKind::Bathroom),
];

#[derive(Clone, Copy)]
struct EntityFacts {
    entity: Entity,
    persistent_id: Option<u64>,
    definition: AssetId,
    species: Option<AssetId>,
    variant: Option<AssetId>,
    adult: bool,
    pregnant: bool,
    crated: bool,
    needs: [Option<i32>; 11],
    carried: bool,
    relocating: bool,
}

fn entity_facts(
    queries: ScenarioLuaQueryFacts<'_>,
    pending: &PendingEntityOperations,
) -> Vec<EntityFacts> {
    collect_entity_facts(queries.live_world_entities, pending)
}

fn collect_entity_facts(
    live_world_entities: &ScenarioLuaLiveEntityQuery<'_, '_>,
    pending: &PendingEntityOperations,
) -> Vec<EntityFacts> {
    let mut facts = live_world_entities
        .iter()
        .map(live_fact)
        .collect::<Vec<_>>();
    let pending = pending.borrow();
    for request in &pending.spawns {
        facts.push(EntityFacts {
            entity: request.entity,
            persistent_id: Some(request.persistent_id.0),
            definition: request.components.definition,
            species: Some(request.species),
            variant: Some(request.components.variant),
            adult: request.components.life_stage == LifeStage::Adult,
            pregnant: false,
            crated: request.crated,
            needs: NEEDS.map(|(_, kind)| {
                request
                    .initial_needs
                    .iter()
                    .rev()
                    .find(|(need, _)| *need == kind)
                    .map(|(_, value)| *value)
            }),
            carried: false,
            relocating: false,
        });
    }
    for command in &pending.commands {
        match command {
            ScenarioEntityCommand::Crate(entity) => {
                if let Some(fact) = facts.iter_mut().find(|v| v.entity == *entity) {
                    fact.crated = true;
                }
            }
            ScenarioEntityCommand::Need(edit) => {
                if let Some(fact) = facts.iter_mut().find(|v| v.entity == edit.animal) {
                    if let Some(index) = NEEDS.iter().position(|(_, kind)| *kind == edit.need) {
                        fact.needs[index] = Some(edit.value_q16);
                    }
                }
            }
            _ => {}
        }
    }
    facts.retain(|fact| {
        !pending.commands.iter().any(|command| {
        matches!(command, ScenarioEntityCommand::Remove(entity) if *entity == fact.entity)
    })
    });
    facts.sort_unstable_by_key(|v| v.entity);
    facts
}

fn live_fact(
    animal: super::scenario_lua_entity_facts::ScenarioLuaEntityFactsItem<'_, '_>,
) -> EntityFacts {
    EntityFacts {
        entity: animal.entity,
        persistent_id: animal.persistent_id.map(|id| id.0),
        definition: animal.definition.0,
        species: animal.species.map(|v| v.species),
        variant: animal.variant.map(|v| v.0),
        adult: animal.stage.is_some_and(|v| v.0 == LifeStage::Adult),
        pregnant: animal.pregnant,
        crated: animal.crated.0 || animal.crated.1,
        needs: NEEDS.map(|(name, _)| animal.need_wellness_q16(name)),
        carried: animal.carried,
        relocating: animal.relocating,
    }
}

fn entity_fact(
    queries: ScenarioLuaQueryFacts<'_>,
    pending: &PendingEntityOperations,
    entity: Entity,
) -> Option<EntityFacts> {
    if let Ok(animal) = queries.live_world_entities.get(entity) {
        let mut fact = live_fact(animal);
        for command in &pending.borrow().commands {
            match command {
                ScenarioEntityCommand::Remove(target) if *target == entity => return None,
                ScenarioEntityCommand::Crate(target) if *target == entity => fact.crated = true,
                ScenarioEntityCommand::Need(edit) if edit.animal == entity => {
                    if let Some(index) = NEEDS.iter().position(|(_, kind)| *kind == edit.need) {
                        fact.needs[index] = Some(edit.value_q16);
                    }
                }
                _ => {}
            }
        }
        return Some(fact);
    }
    let pending = pending.borrow();
    let request = pending
        .spawns
        .iter()
        .find(|request| request.entity == entity)?;
    Some(EntityFacts {
        entity,
        persistent_id: Some(request.persistent_id.0),
        definition: request.components.definition,
        species: Some(request.species),
        variant: Some(request.components.variant),
        adult: request.components.life_stage == LifeStage::Adult,
        pregnant: false,
        crated: request.crated,
        needs: NEEDS.map(|(_, kind)| {
            request
                .initial_needs
                .iter()
                .rev()
                .find(|(need, _)| *need == kind)
                .map(|(_, value)| *value)
        }),
        carried: false,
        relocating: false,
    })
}

fn matches_kind(fact: &EntityFacts, kind: &str, queries: ScenarioLuaQueryFacts<'_>) -> bool {
    matches_entity_type(fact, kind, queries.species, queries.world_definitions)
}

fn authored_attribute_id(attribute: &str) -> AssetId {
    AssetId::from_key(
        &openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(attribute),
    )
}

fn entity_string_attribute(
    queries: ScenarioLuaQueryFacts<'_>,
    fact: EntityFacts,
    attribute: &str,
) -> mlua::Result<String> {
    if let (Some(species), "s_Species" | "s_Product") = (queries.species, attribute) {
        let is_species = attribute == "s_Species";
        let name = fact
            .variant
            .and_then(|variant| species.find_variant_binding(variant))
            .and_then(|binding| {
                if is_species {
                    binding.authored_species_name_override.as_deref()
                } else {
                    binding.authored_product_name_override.as_deref()
                }
            })
            .or_else(|| {
                let species = species.find(fact.species?)?;
                Some(if is_species {
                    species.authored_species_name.as_str()
                } else {
                    species.authored_product_name.as_str()
                })
            });
        if let Some(name) = name {
            return Ok(name.to_owned());
        }
    }
    let key = authored_attribute_id(attribute);
    Ok(queries
        .world_definitions
        .and_then(|definitions| {
            for id in fact.variant.into_iter().chain([fact.definition]) {
                let Some(definition) = definitions.find_object(id) else {
                    continue;
                };
                let values = &definition.authored_runtime.string_behavior_properties;
                if let Ok(index) = values.binary_search_by_key(&key, |(attribute, _)| *attribute) {
                    return Some(values[index].1.clone());
                }
            }
            None
        })
        .unwrap_or_default())
}

fn entity_float_attribute(
    queries: ScenarioLuaQueryFacts<'_>,
    fact: EntityFacts,
    attribute: &str,
) -> mlua::Result<f64> {
    if attribute == "f_TrashLevel" {
        if let Some(value) = queries.waste.and_then(|waste| waste(fact.entity, None)) {
            return Ok(f64::from(value));
        }
    }
    if let Some(index) = NEEDS.iter().position(|(name, _)| *name == attribute) {
        if let Some(wellness) = fact.needs[index] {
            return Ok(
                f64::from(MAX_NEED_Q16 - wellness.clamp(0, MAX_NEED_Q16)) / f64::from(10 * Q16_ONE)
            );
        }
    }
    let key = authored_attribute_id(attribute);
    if let Some(value) = queries
        .authored_float_attributes
        .get(fact.entity)
        .ok()
        .and_then(|values| values.get(key))
    {
        return Ok(f64::from(value));
    }
    Ok(queries
        .world_definitions
        .and_then(|definitions| {
            fact.variant
                .into_iter()
                .chain([fact.definition])
                .find_map(|id| {
                    let values = &definitions
                        .find_object(id)?
                        .authored_runtime
                        .numeric_behavior_properties;
                    let index = values
                        .binary_search_by_key(&key, |(attribute, _)| *attribute)
                        .ok()?;
                    Some(f64::from(values[index].1))
                })
        })
        .unwrap_or_default())
}

fn entity_boolean_attribute(
    queries: ScenarioLuaQueryFacts<'_>,
    fact: EntityFacts,
    attribute: &str,
) -> mlua::Result<bool> {
    match attribute {
        "b_Pregnant" => Ok(fact.pregnant),
        "b_Adult" => Ok(fact.adult),
        "b_ExtinctAnimal" => Ok(fact
            .species
            .and_then(|species_id| queries.species?.find(species_id))
            .is_some_and(|species| {
                species.flags.contains(SpeciesFlags::EXTINCT)
                    || species.conservation == ConservationStatus::Extinct
            })),
        _ => {
            let key = authored_attribute_id(attribute);
            Ok(queries
                .world_definitions
                .and_then(|definitions| {
                    fact.variant
                        .into_iter()
                        .chain([fact.definition])
                        .find_map(|id| {
                            let values = &definitions
                                .find_object(id)?
                                .authored_runtime
                                .boolean_behavior_properties;
                            let index = values
                                .binary_search_by_key(&key, |(attribute, _)| *attribute)
                                .ok()?;
                            Some(values[index].1)
                        })
                })
                .unwrap_or_default())
        }
    }
}

fn same_entity(left: &Table, right: &Table) -> mlua::Result<bool> {
    if let (Some(left), Some(right)) = (
        left.get::<Option<u64>>(PERSISTENT_ENTITY_FIELD)?,
        right.get::<Option<u64>>(PERSISTENT_ENTITY_FIELD)?,
    ) {
        return Ok(left == right);
    }
    Ok(left.get::<u64>(ENTITY_FIELD)? == right.get::<u64>(ENTITY_FIELD)?)
}

/// Each query builds a new table, and scripts compare them with `==` as they
/// would the same object.
fn entity_component_metatable(lua: &Lua) -> mlua::Result<Table> {
    const KEY: &str = "__openzt2_entity_component_metatable";
    if let Some(metatable) = lua.named_registry_value::<Option<Table>>(KEY)? {
        return Ok(metatable);
    }
    let metatable = lua.create_table()?;
    metatable.set(
        "__eq",
        lua.create_function(|_, (left, right): (Table, Table)| same_entity(&left, &right))?,
    )?;
    lua.set_named_registry_value(KEY, &metatable)?;
    Ok(metatable)
}

fn reference_entity(
    reference: &Table,
    queries: ScenarioLuaQueryFacts<'_>,
    pending: &PendingEntityOperations,
) -> mlua::Result<Option<Entity>> {
    let raw = reference
        .get::<Option<u64>>(ENTITY_FIELD)?
        .and_then(Entity::try_from_bits);
    let persistent = reference.get::<Option<u64>>(PERSISTENT_ENTITY_FIELD)?;
    let entity = if let Some(id) = persistent {
        queries
            .world
            .and_then(|world| {
                queries.persistent_ids.get(
                    world,
                    openzt2_world_state::world_spawn::persistent_id_types::PersistentId(id),
                )
            })
            .or_else(|| {
                pending
                    .borrow()
                    .spawns
                    .iter()
                    .find(|request| request.persistent_id.0 == id)
                    .map(|request| request.entity)
            })
    } else {
        raw
    };
    Ok(entity.filter(|entity| {
        entity_fact(queries, pending, *entity).is_some()
            || queries.fence_edges.contains(*entity)
            || queries.path_tiles.contains(*entity)
    }))
}

pub(super) fn resolve_reference_entity(
    reference: &Table,
    queries: ScenarioLuaQueryFacts<'_>,
    pending: &PendingEntityOperations,
) -> mlua::Result<Entity> {
    reference_entity(reference, queries, pending)?
        .ok_or_else(|| mlua::Error::runtime("scenario entity is no longer present"))
}

fn matches_entity_type(
    fact: &EntityFacts,
    kind: &str,
    species: Option<openzt2_game_assets::species::species_asset_types::SpeciesView<'_>>,
    world_definitions: Option<openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>>,
) -> bool {
    let kind = openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(kind);
    let requested = AssetId::from_key(&kind);
    fact.definition == requested
        || fact.species == Some(requested)
        || fact.variant == Some(requested)
        || (kind == "animal" && fact.species.is_some())
        || fact
            .variant
            .and_then(|variant| species?.find_variant(variant))
            .is_some_and(|variant| {
                variant
                    .behavior_subject_type_identifiers
                    .contains(&requested)
            })
        || world_definitions
            .and_then(|v| v.find_object(fact.definition))
            .is_some_and(|v| v.behavior_type_identifiers.contains(&requested))
}

pub(super) fn create_scenario_lua_entity_manager<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    queries: ScenarioLuaQueryFacts<'env>,
    pending: &'env PendingEntityOperations,
) -> mlua::Result<Table> {
    lua.globals().set(
        "resolveTable",
        scope.create_function(move |lua, value: Value| match value {
            Value::Table(reference) => {
                if let Some(fact) = reference_entity(&reference, queries, pending)?
                    .and_then(|entity| entity_fact(queries, pending, entity))
                {
                    return create_entity_component(lua, scope, queries, pending, fact)
                        .map(Value::Table);
                }
                Ok(Value::Nil)
            }
            _ => Err(mlua::Error::runtime(
                "resolveTable requires an entity reference from BFG_GET_ENTITIES",
            )),
        })?,
    )?;
    lua.globals().set(
        "areCompsEqual",
        lua.create_function(|_, (left, right): (Table, Table)| same_entity(&left, &right))?,
    )?;
    lua.globals().set(
        "isKindOf",
        scope.create_function(move |_, (binder, kind): (Value, String)| {
            let binder = match binder {
                Value::Table(binder) => binder,
                Value::String(key) => {
                    let key = openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(&key.to_str()?);
                    let requested = AssetId::from_key(&openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(&kind));
                    return Ok(AssetId::from_key(&key) == requested
                        || queries
                            .world_definitions
                            .and_then(|definitions| definitions.find_object(AssetId::from_key(&key)))
                            .is_some_and(|object| {
                                object.behavior_type_identifiers.contains(&requested)
                            }));
                }
                _ => return Err(mlua::Error::runtime("isKindOf requires an entity or binder")),
            };
            Ok(reference_entity(&binder, queries, pending)?
                .and_then(|entity| entity_fact(queries, pending, entity))
                .is_some_and(|fact| matches_kind(&fact, &kind, queries)))
        })?,
    )?;
    let manager = lua.create_table()?;
    let get_entities = scope.create_function(
        move |lua, (_, kind, direct): (Table, String, Option<bool>)| {
            let table = lua.create_table()?;
            for (index, fact) in entity_facts(queries, pending)
                .into_iter()
                .filter(|v| {
                    // An object being carried has left the world, though it stays selected.
                    !v.relocating && !v.carried
                        && !(direct == Some(true) && v.crated)
                        && matches_kind(v, &kind, queries)
                })
                .enumerate()
            {
                let reference = lua.create_table()?;
                reference.set(
                    "value",
                    create_entity_component(lua, scope, queries, pending, fact)?,
                )?;
                table.set(index + 1, reference)?;
            }
            let fence_kind = AssetId::from_key(&openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key(&kind));
            let kind_name = kind.to_ascii_lowercase();
            for (entity, edge, is_gate) in queries.fence_edges.iter() {
                // Authored types place every gate under `gate` and every other run
                // under `fence`, beside its own definition. A gate keeps its host
                // fence's definition here but counts as the host's gate type, as
                // the Marine Mania tutorial's tank entrance step expects.
                let matched = if is_gate {
                    kind_name == "gate"
                        || queries
                            .world_definitions
                            .and_then(|definitions| definitions.find_fence(edge.definition))
                            .is_some_and(|host| host.gate == fence_kind)
                } else {
                    edge.definition == fence_kind || kind_name == "fence"
                };
                if !matched {
                    continue;
                }
                let component = lua.create_table()?;
                component.set(ENTITY_FIELD, entity.to_bits())?;
                let reference = lua.create_table()?;
                reference.set("value", component)?;
                table.raw_push(reference)?;
            }
            for (entity, tile) in queries.path_tiles.iter() {
                if kind_name != "path" && tile.definition != fence_kind {
                    continue;
                }
                let component = lua.create_table()?;
                component.set(ENTITY_FIELD, entity.to_bits())?;
                let reference = lua.create_table()?;
                reference.set("value", component)?;
                table.raw_push(reference)?;
            }
            Ok(table)
        },
    )?;
    manager.set("BFG_GET_ENTITIES", get_entities.clone())?;
    manager.set(
        "BFG_GET_ENTITIES_DIRECT",
        scope.create_function(move |_, (manager, kind): (Table, String)| {
            get_entities.call::<Table>((manager, kind, true))
        })?,
    )?;
    manager.set(
        "BFG_REMOVE_ENTITY",
        scope.create_function(move |_, (_, reference): (Table, Table)| {
            if let Some(entity) = reference_entity(&reference, queries, pending)? {
                pending
                    .borrow_mut()
                    .commands
                    .push(ScenarioEntityCommand::Remove(entity));
            }
            Ok(())
        })?,
    )?;
    manager.set(
        "BFG_GET_SELECTED_ENTITY",
        scope.create_function(move |lua, _: Table| {
            match queries
                .selected_entity
                .and_then(|entity| entity_fact(queries, pending, entity))
            {
                Some(fact) => Ok(Value::Table(create_entity_component(
                    lua, scope, queries, pending, fact,
                )?)),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    Ok(manager)
}

fn create_entity_component<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    queries: ScenarioLuaQueryFacts<'env>,
    pending: &'env PendingEntityOperations,
    fact: EntityFacts,
) -> mlua::Result<Table> {
    let entity = fact.entity;
    let component = lua.create_table()?;
    component.set_metatable(Some(entity_component_metatable(lua)?))?;
    component.set(ENTITY_FIELD, entity.to_bits())?;
    component.set(PERSISTENT_ENTITY_FIELD, fact.persistent_id)?;
    component.set("_this", component.clone())?;
    component.set(
        "BFG_FIND_CONTAINED_ENTITY",
        scope.create_function(move |lua, (_, kind): (Table, String)| {
            let crate_occupant = queries
                .contained_crated_animals
                .get(entity)
                .ok()
                .map(|crate_| crate_.occupant());
            let found = crate_occupant
                .into_iter()
                .chain((queries.contained_entities)(entity))
                .filter_map(|occupant| entity_fact(queries, pending, occupant))
                .find(|fact| matches_kind(fact, &kind, queries));
            match found {
                Some(fact) => {
                    create_entity_component(lua, scope, queries, pending, fact).map(Value::Table)
                }
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    component.set(
        "BFG_GET_BINDER_TYPE",
        // Scripts both compare this with a record key and pass it to `isKindOf`.
        scope.create_function(move |lua, entity: Table| {
            let key = queries
                .world_definitions
                .and_then(|definitions| definitions.object_record_key(fact.definition));
            Ok(match key {
                Some(key) => Value::String(lua.create_string(key)?),
                None => Value::Table(entity),
            })
        })?,
    )?;
    let current = move || {
        entity_fact(queries, pending, entity)
            .ok_or_else(|| mlua::Error::runtime("attribute requested for absent entity"))
    };
    component.set(
        "BFG_GET_ATTR_BOOLEAN",
        scope.create_function(move |_, (_, attribute): (Table, String)| {
            entity_boolean_attribute(queries, current()?, &attribute)
        })?,
    )?;
    component.set(
        "BFG_GET_ATTR_STRING",
        scope.create_function(move |_, (_, attribute): (Table, String)| {
            entity_string_attribute(queries, current()?, &attribute)
        })?,
    )?;
    component.set(
        "BFG_GET_ATTR_FLOAT",
        scope.create_function(move |_, (_, attribute): (Table, String)| {
            entity_float_attribute(queries, current()?, &attribute)
        })?,
    )?;
    component.set(
        "BFAI_GET_PRESSING_THRESHOLD",
        scope.create_function(move |_, (_, attribute): (Table, String)| {
            let kind = NEEDS
                .iter()
                .find(|(name, _)| *name == attribute)
                .map(|(_, kind)| *kind)
                .ok_or_else(|| mlua::Error::runtime("unsupported animal need"))?;
            let threshold = entity_fact(queries, pending, entity)
                .and_then(|v| queries.species?.find(v.species?))
                .and_then(|species| species.needs.iter().find(|need| need.kind == kind))
                .and_then(|need| need.pressing_wellness_threshold)
                .ok_or_else(|| {
                    mlua::Error::runtime("animal need has no authored pressing threshold")
                })?;
            Ok(f64::from(1000 - threshold) / 10.0)
        })?,
    )?;
    component.set(
        "BFG_SET_ATTR_FLOAT",
        scope.create_function(
            move |_, (_, attribute, value): (Table, mlua::Value, Option<f64>)| {
                // Scripts pass either a named attribute table or the name and value.
                if let (mlua::Value::String(name), Some(level)) = (&attribute, value) {
                    if name.to_str()?.as_ref() != "f_TrashLevel" {
                        return Err(mlua::Error::runtime("unsupported float attribute"));
                    }
                    queries
                        .waste
                        .and_then(|waste| waste(entity, Some(level as u16)))
                        .ok_or_else(|| mlua::Error::runtime("entity holds no trash"))?;
                    return Ok(());
                }
                let mlua::Value::Table(attribute) = attribute else {
                    return Err(mlua::Error::runtime("expected BFGFloatAttribute"));
                };
                if attribute.get::<String>("_type")? != "BFGFloatAttribute" {
                    return Err(mlua::Error::runtime("expected BFGFloatAttribute"));
                }
                let name = attribute.get::<String>("name")?;
                let need = NEEDS
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, kind)| *kind)
                    .ok_or_else(|| mlua::Error::runtime("unsupported animal need"))?;
                let distress = attribute.get::<f64>("value")?;
                if !distress.is_finite() || !(0.0..=100.0).contains(&distress) {
                    return Err(mlua::Error::runtime(
                        "animal distress must be between zero and one hundred",
                    ));
                }
                let value_q16 = ((1000.0 - distress * 10.0) * f64::from(Q16_ONE)).round() as i32;
                let mut pending = pending.borrow_mut();
                if let Some(spawn) = pending.spawns.iter_mut().find(|v| v.entity == entity) {
                    spawn.initial_needs.push((need, value_q16));
                } else {
                    pending
                        .commands
                        .push(ScenarioEntityCommand::Need(SetAnimalNeed {
                            animal: entity,
                            need,
                            value_q16,
                        }));
                }
                Ok(())
            },
        )?,
    )?;
    component.set(
        "BFG_SEND_SIGNAL_SHARED",
        scope.create_function(move |_, (_, signal): (Table, String)| {
            if signal != "ZT_CRATE_ENTITY" {
                super::scenario_lua_ui::warn_unhandled_scenario_message(&signal);
                return Ok(());
            }
            let mut pending = pending.borrow_mut();
            if let Some(spawn) = pending.spawns.iter_mut().find(|v| v.entity == entity) {
                spawn.crated = true;
            } else {
                pending.commands.push(ScenarioEntityCommand::Crate(entity));
            }
            Ok(())
        })?,
    )?;
    Ok(component)
}

pub(super) fn create_world_manager<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    queries: ScenarioLuaQueryFacts<'env>,
    pending: &'env PendingEntityOperations,
) -> mlua::Result<Table> {
    let manager = lua.create_table()?;
    manager.set(
        "ZT_GET_BIOME_COUNT",
        scope.create_function(move |_, (_, biome): (Table, String)| {
            Ok(queries.biome_cells.map_or(0, |cells| cells.count(&biome)))
        })?,
    )?;
    manager.set(
        "ZT_GET_REAL_TIME",
        scope.create_function(move |_, _: Table| Ok(queries.real_seconds))?,
    )?;
    for (method, read) in [
        (
            "BFG_GET_CAMERA_X",
            (|rig| rig.focus.x)
                as fn(
                    &openzt2_interface_state::camera::camera_runtime_state_types::OverheadRig,
                ) -> f32,
        ),
        ("BFG_GET_CAMERA_Y", |rig| rig.focus.y),
        ("BFG_GET_CAMERA_Z", |rig| rig.height_m),
        ("BFG_GET_CAMERA_ZOOM", |rig| rig.distance),
        ("BFG_GET_CAMERA_YAW", |rig| rig.yaw),
    ] {
        manager.set(
            method,
            scope.create_function(move |_, _: Table| Ok(queries.overhead_camera.map(read)))?,
        )?;
    }
    let attribute_source = move |request: &Table| -> mlua::Result<(EntityFacts, String)> {
        let binder: Table = request.get("key")?;
        let entity = resolve_reference_entity(&binder, queries, pending)?;
        let fact = entity_fact(queries, pending, entity)
            .ok_or_else(|| mlua::Error::runtime("attribute requested for absent entity"))?;
        Ok((fact, request.get("val")?))
    };
    manager.set(
        "BFG_GET_ATTR_STRING",
        scope.create_function(move |_, (_, request): (Table, Table)| {
            let (fact, key) = attribute_source(&request)?;
            entity_string_attribute(queries, fact, &key)
        })?,
    )?;
    manager.set(
        "BFG_GET_ATTR_FLOAT",
        scope.create_function(move |_, (_, request): (Table, Table)| {
            let (fact, key) = attribute_source(&request)?;
            entity_float_attribute(queries, fact, &key)
        })?,
    )?;
    manager.set(
        "BFG_GET_ATTR_BOOLEAN",
        scope.create_function(move |_, (_, request): (Table, Table)| {
            let (fact, key) = attribute_source(&request)?;
            entity_boolean_attribute(queries, fact, &key)
        })?,
    )?;
    manager.set(
        "ZT_NAME_ENTITY",
        scope.create_function(move |_, (_, component): (Table, Table)| {
            let entity = resolve_reference_entity(&component, queries, pending)?;
            if entity_fact(queries, pending, entity).is_none() {
                return Err(mlua::Error::runtime("cannot name an absent entity"));
            }
            pending
                .borrow_mut()
                .commands
                .push(super::scenario_lua_entity_commands::ScenarioEntityCommand::Name(entity));
            Ok(())
        })?,
    )?;
    Ok(manager)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{ecs::system::SystemState, prelude::World};
    use openzt2_world_state::world_spawn::{
        world_entity_crating::ScriptedCrating,
        world_membership_types::{DefinitionId, WorldMember},
    };
    use openzt2_zoo_state::{
        animal_health::types::{Dead, DeathCause},
        animal_lifecycle::types::{AnimalVariant, SpeciesHandle},
    };

    #[test]
    fn variant_queries_include_crates_and_exclude_dead_animals() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let species = AssetId::from_key("lion");
        let male = AssetId::from_key("lion_adult_m");
        let female = AssetId::from_key("lion_adult_f");
        let mut spawn = |variant| {
            world
                .spawn((
                    WorldMember { root },
                    DefinitionId(species),
                    SpeciesHandle { species },
                    AnimalVariant(variant),
                ))
                .id()
        };
        let living = spawn(male);
        let crated = spawn(male);
        let dead = spawn(male);
        spawn(female);
        world.entity_mut(crated).insert(ScriptedCrating);
        let death = Dead {
            cause: DeathCause::Authored(AssetId::from_key("oldage")),
            since_tick: 1,
        };
        world.entity_mut(dead).insert(death);
        let mut state = SystemState::<ScenarioLuaLiveEntityQuery>::new(&mut world);
        let pending = PendingEntityOperations::default();
        let mut counts = |world: &World| {
            let query = state.get(world).unwrap();
            let facts = collect_entity_facts(&query, &pending);
            let count = |kind, direct| {
                facts
                    .iter()
                    .filter(|fact| {
                        !(direct && fact.crated) && matches_entity_type(fact, kind, None, None)
                    })
                    .count()
            };
            [
                count("Lion_Adult_M", false),
                count("Lion_Adult_M", true),
                count("Lion", false),
                count("animal", false),
            ]
        };
        assert_eq!(counts(&world), [2, 1, 3, 3]);
        world.entity_mut(living).insert(death);
        assert_eq!(counts(&world), [1, 0, 2, 2]);
        world.entity_mut(crated).remove::<ScriptedCrating>();
        assert_eq!(counts(&world), [1, 1, 2, 2]);
    }
}
