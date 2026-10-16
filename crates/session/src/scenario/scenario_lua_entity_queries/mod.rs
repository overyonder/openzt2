//! Component tables project live entities and the canonical creations reserved by
//! the current invocation. Every reference retains its actual ECS identity.
use super::{
    scenario_lua_entity_commands::{PendingEntityOperations, ScenarioEntityCommand},
    scenario_lua_types::{ScenarioLuaLiveEntityQuery, ScenarioLuaQueryFacts},
};
use bevy::prelude::Entity;
use mlua::{Lua, Scope, Table, Value};
use openzt2_game_data::{
    species::{LifeStage, NeedKind},
    AssetId,
};
use openzt2_zoo_state::animal_welfare::types::{SetAnimalNeed, MAX_NEED_Q16, Q16_ONE};

const ENTITY_FIELD: &str = "__openzt2_entity";
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

struct EntityFacts {
    entity: Entity,
    definition: AssetId,
    species: Option<AssetId>,
    variant: Option<AssetId>,
    adult: bool,
    pregnant: bool,
    crated: bool,
    needs: [Option<i32>; 11],
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
        .map(|animal| EntityFacts {
            entity: animal.entity,
            definition: animal.definition.0,
            species: animal.species.map(|v| v.species),
            variant: animal.variant.map(|v| v.0),
            adult: animal.stage.is_some_and(|v| v.0 == LifeStage::Adult),
            pregnant: animal.pregnant,
            crated: animal.crated,
            needs: NEEDS.map(|(name, _)| animal.need_wellness_q16(name)),
        })
        .collect::<Vec<_>>();
    let pending = pending.borrow();
    for request in &pending.spawns {
        facts.push(EntityFacts {
            entity: request.entity,
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
    facts.sort_unstable_by_key(|v| v.entity);
    facts
}

fn matches_kind(fact: &EntityFacts, kind: &str, queries: ScenarioLuaQueryFacts<'_>) -> bool {
    matches_entity_type(fact, kind, queries.species, queries.world_definitions)
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
        lua.create_function(|_, value: Value| match value {
            Value::Table(entity) if entity.contains_key(ENTITY_FIELD)? => Ok(entity),
            _ => Err(mlua::Error::runtime(
                "resolveTable requires an entity reference from BFG_GET_ENTITIES",
            )),
        })?,
    )?;
    lua.globals().set(
        "areCompsEqual",
        lua.create_function(|_, (left, right): (Table, Table)| {
            Ok(left.get::<u64>(ENTITY_FIELD)? == right.get::<u64>(ENTITY_FIELD)?)
        })?,
    )?;
    lua.globals().set(
        "isKindOf",
        scope.create_function(move |_, (binder, kind): (Table, String)| {
            let entity = Entity::from_bits(binder.get::<u64>(ENTITY_FIELD)?);
            Ok(entity_facts(queries, pending)
                .iter()
                .find(|v| v.entity == entity)
                .is_some_and(|v| matches_kind(v, &kind, queries)))
        })?,
    )?;
    let manager = lua.create_table()?;
    let get_entities = scope.create_function(
        move |lua, (_, kind, direct): (Table, String, Option<bool>)| {
            let table = lua.create_table()?;
            for (index, fact) in entity_facts(queries, pending)
                .into_iter()
                .filter(|v| !(direct == Some(true) && v.crated) && matches_kind(v, &kind, queries))
                .enumerate()
            {
                let reference = lua.create_table()?;
                reference.set(
                    "value",
                    create_entity_component(lua, scope, queries, pending, fact)?,
                )?;
                table.set(index + 1, reference)?;
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
        "BFG_GET_SELECTED_ENTITY",
        scope.create_function(move |lua, _: Table| {
            match entity_facts(queries, pending)
                .into_iter()
                .find(|v| Some(v.entity) == queries.selected_entity)
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
    component.set(ENTITY_FIELD, entity.to_bits())?;
    component.set("_this", component.clone())?;
    component.set(
        "BFG_FIND_CONTAINED_ENTITY",
        lua.create_function(|_, _: mlua::MultiValue| Ok(Value::Nil))?,
    )?;
    component.set(
        "BFG_GET_BINDER_TYPE",
        lua.create_function(|_, entity: Table| Ok(entity))?,
    )?;
    component.set(
        "BFG_GET_ATTR_BOOLEAN",
        lua.create_function(
            move |_, (_, attribute): (Table, String)| match attribute.as_str() {
                "b_Pregnant" => Ok(fact.pregnant),
                "b_Adult" => Ok(fact.adult),
                _ => Err(mlua::Error::runtime(format!(
                    "BFG_GET_ATTR_BOOLEAN does not expose {attribute}"
                ))),
            },
        )?,
    )?;
    component.set(
        "BFG_GET_ATTR_FLOAT",
        scope.create_function(move |_, (_, attribute): (Table, String)| {
            let index = NEEDS
                .iter()
                .position(|(name, _)| *name == attribute)
                .ok_or_else(|| mlua::Error::runtime("unsupported animal float attribute"))?;
            let wellness = entity_facts(queries, pending)
                .iter()
                .find(|v| v.entity == entity)
                .and_then(|v| v.needs[index])
                .ok_or_else(|| mlua::Error::runtime("animal need has not initialized"))?;
            Ok(f64::from(MAX_NEED_Q16 - wellness.clamp(0, MAX_NEED_Q16)) / f64::from(10 * Q16_ONE))
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
            let threshold = entity_facts(queries, pending)
                .iter()
                .find(|v| v.entity == entity)
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
        scope.create_function(move |_, (_, attribute): (Table, Table)| {
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
        })?,
    )?;
    component.set(
        "BFG_SEND_SIGNAL_SHARED",
        scope.create_function(move |_, (_, signal): (Table, String)| {
            if signal != "ZT_CRATE_ENTITY" {
                return Err(mlua::Error::runtime(format!(
                    "unsupported entity signal {signal}"
                )));
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
        "BFG_GET_ATTR_STRING",
        scope.create_function(move |_, (_, attribute): (Table, Table)| {
            if attribute.get::<String>("val")? != "s_Species" {
                return Err(mlua::Error::runtime("unsupported world string attribute"));
            }
            let binder: Table = attribute.get("key")?;
            let entity = Entity::from_bits(binder.get::<u64>(ENTITY_FIELD)?);
            let facts = entity_facts(queries, pending);
            let fact = facts
                .iter()
                .find(|fact| fact.entity == entity)
                .ok_or_else(|| mlua::Error::runtime("species requested for absent entity"))?;
            let species = queries
                .species
                .ok_or_else(|| mlua::Error::runtime("species catalogue unavailable"))?;
            if let Some(name) = fact
                .variant
                .and_then(|variant| species.find_variant_binding(variant))
                .and_then(|binding| binding.authored_species_name_override.as_deref())
            {
                return Ok(name.to_owned());
            }
            fact.species
                .and_then(|id| species.find(id))
                .map(|species| species.authored_species_name.clone())
                .ok_or_else(|| mlua::Error::runtime("entity has no authored species"))
        })?,
    )?;
    manager.set(
        "ZT_NAME_ENTITY",
        scope.create_function(move |_, (_, component): (Table, Table)| {
            let entity = Entity::from_bits(component.get::<u64>(ENTITY_FIELD)?);
            if !entity_facts(queries, pending)
                .iter()
                .any(|v| v.entity == entity)
            {
                return Err(mlua::Error::runtime("cannot name an absent entity"));
            }
            // Scripted creations already carry their canonical generated Name.
            if !pending.borrow().spawns.iter().any(|v| v.entity == entity) {
                pending
                    .borrow_mut()
                    .commands
                    .push(ScenarioEntityCommand::Name(entity));
            }
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
        world_entity_crating::WorldEntityIsCrated,
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
        world.entity_mut(crated).insert(WorldEntityIsCrated);
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
        world.entity_mut(crated).remove::<WorldEntityIsCrated>();
        assert_eq!(counts(&world), [1, 1, 2, 2]);
    }
}
