//! Projects winning container definitions onto live quantities.

#[cfg(test)]
mod tests;

use bevy::prelude::*;
use openzt2_game_data::world_definitions::world_objects::WorldObjectContainerContent;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use super::container_quantity::{DrinkContainer, FoodContainer, AUTHORED_CONTAINER_CAPACITY_Q16};

pub(super) fn hydrate_food_and_drink_container_quantities_for_new_world_objects(
    mut commands: Commands,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut events: Option<MessageReader<AssetEvent<WorldDefinitionAsset>>>,
    mut objects: Query<(
        Entity,
        &DefinitionId,
        Option<&mut FoodContainer>,
        Option<&mut DrinkContainer>,
    )>,
    changed: Query<Entity, Changed<DefinitionId>>,
    mut removed: RemovedComponents<DefinitionId>,
) {
    let asset_changes = events.as_mut().is_some_and(|events| {
        let changes = events.read().any(|event| {
            matches!(
                event,
                AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
            )
        });
        events.clear();
        changes
    });
    let all = definitions.is_changed() || asset_changes;
    let mut entities: Vec<_> = if all {
        objects.iter().map(|(entity, ..)| entity).collect()
    } else {
        changed.iter().collect()
    };
    entities.extend(removed.read());
    entities.sort_unstable();
    entities.dedup();
    let active = definitions.get(&assets);
    for entity in entities {
        let Ok((_, definition, food, drink)) = objects.get_mut(entity) else {
            commands
                .entity(entity)
                .try_remove::<(FoodContainer, DrinkContainer)>();
            continue;
        };
        let object = active.and_then(|active| active.find_object(definition.0));
        if object.is_none() && (!definitions.is_complete() || active.is_none()) {
            if food.is_some_and(|container| container.food != definition.0) {
                commands.entity(entity).remove::<FoodContainer>();
            }
            if drink.is_some_and(|container| container.drink != definition.0) {
                commands.entity(entity).remove::<DrinkContainer>();
            }
            continue;
        }
        let quantity = object.and_then(|object| object.container_quantity);
        match quantity.map(|quantity| (quantity.content, quantity.initial_q16)) {
            Some((WorldObjectContainerContent::Food, initial)) => {
                if drink.is_some() {
                    commands.entity(entity).remove::<DrinkContainer>();
                }
                let amount_q16 = food
                    .as_deref()
                    .filter(|container| container.food == definition.0)
                    .map_or(initial, |container| container.amount_q16)
                    .clamp(0, AUTHORED_CONTAINER_CAPACITY_Q16);
                let next = FoodContainer {
                    food: definition.0,
                    amount_q16,
                    capacity_q16: AUTHORED_CONTAINER_CAPACITY_Q16,
                };
                if let Some(mut food) = food {
                    food.set_if_neq(next);
                } else {
                    commands.entity(entity).insert(next);
                }
            }
            Some((WorldObjectContainerContent::Drink, initial)) => {
                if food.is_some() {
                    commands.entity(entity).remove::<FoodContainer>();
                }
                let amount_q16 = drink
                    .as_deref()
                    .filter(|container| container.drink == definition.0)
                    .map_or(initial, |container| container.amount_q16)
                    .clamp(0, AUTHORED_CONTAINER_CAPACITY_Q16);
                let next = DrinkContainer {
                    drink: definition.0,
                    amount_q16,
                    capacity_q16: AUTHORED_CONTAINER_CAPACITY_Q16,
                };
                if let Some(mut drink) = drink {
                    drink.set_if_neq(next);
                } else {
                    commands.entity(entity).insert(next);
                }
            }
            None => {
                if food.is_some() || drink.is_some() {
                    commands
                        .entity(entity)
                        .remove::<(FoodContainer, DrinkContainer)>();
                }
            }
        }
    }
}
