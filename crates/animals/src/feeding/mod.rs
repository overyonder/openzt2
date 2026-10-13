//! Food and drink container quantities and consumption.

use openzt2_zoo_state::feeding::ContainerQuantityHydration;

use openzt2_zoo_state::feeding::container_quantity;
mod container_quantity_hydration;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;

pub struct FeedingPlugin;

impl Plugin for FeedingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            container_quantity_hydration::hydrate_food_and_drink_container_quantities_for_new_world_objects
                .in_set(FixedGameSet::Think)
                .in_set(ContainerQuantityHydration)
                .run_if(in_state(GamePhase::InGame)),
        );
    }
}
