//! Initializes container quantities from world definitions.

#[cfg(test)]
mod tests;

use bevy::prelude::*;
use openzt2_game_data::{world_definitions::world_objects::WorldObjectContainerContent, AssetId};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::container_quantity::{DrinkContainer, FoodContainer, AUTHORED_CONTAINER_CAPACITY_Q16};

#[derive(Component, Debug, PartialEq, Eq)]
pub(crate) struct ContainerQuantityResolved {
    pub(crate) definition_id: AssetId,
    pub(crate) definitions_revision: u64,
}

pub(super) fn hydrate_food_and_drink_container_quantities_for_new_world_objects(
    mut commands: Commands,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    objects: Query<(
        Entity,
        &DefinitionId,
        Option<&ContainerQuantityResolved>,
        Option<&FoodContainer>,
        Option<&DrinkContainer>,
    )>,
) {
    let definitions_revision = active_world_definitions.catalogue_revision();
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    for (entity, definition_id, resolved, food, drink) in &objects {
        // A replacement definition must not expose the previous object's
        // consumable state while its asset is still loading.
        if food.is_some_and(|container| container.food != definition_id.0)
            || drink.is_some_and(|container| container.drink != definition_id.0)
            || resolved.is_some_and(|state| state.definition_id != definition_id.0)
        {
            commands
                .entity(entity)
                .remove::<FoodContainer>()
                .remove::<DrinkContainer>()
                .remove::<ContainerQuantityResolved>();
        }
        let Some(world_definitions) = world_definitions.as_ref() else {
            continue;
        };
        if resolved.is_some_and(|state| {
            state.definitions_revision == definitions_revision
                && state.definition_id == definition_id.0
        }) {
            continue;
        }
        let Some(object_definition) = world_definitions.find_object(definition_id.0) else {
            // A registry record whose asset is not loaded yet is pending, not
            // absent. Leave the entity unmarked so the next hydration pass
            // retries it.
            if unresolved_definition_should_retry(
                world_definitions.has_object_record(definition_id.0),
                world_definitions.has_pending_assets(),
            ) {
                continue;
            }
            commands
                .entity(entity)
                .remove::<FoodContainer>()
                .remove::<DrinkContainer>()
                .insert(ContainerQuantityResolved {
                    definition_id: definition_id.0,
                    definitions_revision,
                });
            continue;
        };

        let mut entity_commands = commands.entity(entity);
        match object_definition.container_quantity {
            Some(authored_quantity) => {
                let existing_amount_q16 = match authored_quantity.content {
                    WorldObjectContainerContent::Food => food
                        .filter(|container| container.food == definition_id.0)
                        .map(|container| container.amount_q16),
                    WorldObjectContainerContent::Drink => drink
                        .filter(|container| container.drink == definition_id.0)
                        .map(|container| container.amount_q16),
                };
                let amount_q16 = preserve_live_container_amount(
                    existing_amount_q16,
                    authored_quantity.initial_q16,
                );
                match authored_quantity.content {
                    WorldObjectContainerContent::Food => {
                        entity_commands
                            .remove::<DrinkContainer>()
                            .insert(FoodContainer {
                                food: object_definition.id,
                                amount_q16,
                                capacity_q16: AUTHORED_CONTAINER_CAPACITY_Q16,
                            });
                    }
                    WorldObjectContainerContent::Drink => {
                        entity_commands
                            .remove::<FoodContainer>()
                            .insert(DrinkContainer {
                                drink: object_definition.id,
                                amount_q16,
                                capacity_q16: AUTHORED_CONTAINER_CAPACITY_Q16,
                            });
                    }
                }
            }
            None => {
                entity_commands
                    .remove::<FoodContainer>()
                    .remove::<DrinkContainer>();
            }
        }
        entity_commands.insert(ContainerQuantityResolved {
            definition_id: definition_id.0,
            definitions_revision,
        });
    }
}

fn unresolved_definition_should_retry(has_record: bool, has_pending_assets: bool) -> bool {
    has_record || has_pending_assets
}

fn preserve_live_container_amount(existing_amount_q16: Option<i32>, initial_q16: i32) -> i32 {
    existing_amount_q16
        .unwrap_or(initial_q16)
        .clamp(0, AUTHORED_CONTAINER_CAPACITY_Q16)
}
