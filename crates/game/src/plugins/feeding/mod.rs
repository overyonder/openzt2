//! Food and drink container quantities and consumption.

pub(crate) mod container_quantity;
mod container_quantity_hydration;

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::FixedGameSet;

pub(crate) struct FeedingPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ContainerQuantityHydration;

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
