use super::placement_occupancy_index::{discard_occupied_cells, register_occupied_cells};
use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};
use openzt2_game_data::world_definitions::object_placement::EntrancePurpose;
use openzt2_game_data::AssetId;

/// Stable definition identity attached to an object governed by placement.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedObjectDefinitionReference {
    pub definition: AssetId,
}

/// Authoritative occupied grid origin and authored orientation for a placed object.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(IndexedObjectPlacementCells)]
#[component(on_remove = discard_footprint_projection)]
pub struct PlacedObjectFootprintOccupancy {
    pub origin: IVec2,
    pub eighth_turns: u8,
}

/// Cells currently published to the spatial placement index.
#[derive(Component, Debug, Default)]
#[component(immutable, on_insert = register_occupied_cells, on_discard = discard_occupied_cells)]
pub struct IndexedObjectPlacementCells(pub Box<[IVec2]>);

fn discard_footprint_projection(mut world: DeferredWorld, context: HookContext) {
    world.commands().queue(move |world: &mut World| {
        if let Ok(mut entity) = world.get_entity_mut(context.entity) {
            if !entity.contains::<PlacedObjectFootprintOccupancy>() {
                entity.remove::<IndexedObjectPlacementCells>();
            }
        }
    });
}

/// Opt-in for objects whose physics motion changes their occupied placement cells.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhysicsMovedPlacedObjectFootprint;

/// One authored object entrance projected into the Bevy world.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PlacedObjectAuthoredEntrance {
    pub local_position: Vec3,
    pub local_forward: Vec3,
    pub purpose: EntrancePurpose,
}
