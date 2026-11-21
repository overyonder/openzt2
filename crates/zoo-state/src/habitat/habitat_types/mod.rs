use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Habitat;

#[derive(Component, Debug, Clone, PartialEq)]
pub struct HabitatRegion {
    pub topology_cells: Box<[IVec2]>,
    pub area_square_metres: f32,
}

#[derive(Component)]
#[relationship(relationship_target = HabitatFenceIncidences)]
pub struct HabitatBoundaryFence(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = HabitatBoundaryFence, linked_spawn)]
pub struct HabitatFenceIncidences(Vec<Entity>);

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HabitatTopologyPublication;

#[derive(Component, Debug, Clone, PartialEq)]
pub struct HabitatSummary {
    pub land_area_square_metres: f32,
    pub water_area_square_metres: f32,
    pub biome_areas_square_metres: Box<[(AssetId, f32)]>,
    pub boundary_is_breached: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HabitatClimate {
    pub biome_identifier: AssetId,
    pub temperature_range_celsius: [i16; 2],
    pub humidity_range_permille: [u16; 2],
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = HabitatMembers)]
pub struct HabitatMember {
    #[relationship]
    pub habitat_entity: Entity,
}

#[derive(Component)]
#[relationship_target(relationship = HabitatMember)]
pub struct HabitatMembers(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[require(Containment)]
pub struct HabitatLocatable;

#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Containment {
    pub is_contained: bool,
}

#[derive(Resource, Debug, Default)]
pub struct HabitatIndex {
    habitats_by_topology_cell: HashMap<IVec2, Entity>,
}

impl HabitatIndex {
    pub fn habitat_for_topology_cell(&self, topology_cell: IVec2) -> Option<Entity> {
        self.habitats_by_topology_cell.get(&topology_cell).copied()
    }

    pub fn record_habitat_for_topology_cell(&mut self, topology_cell: IVec2, habitat: Entity) {
        self.habitats_by_topology_cell
            .insert(topology_cell, habitat);
    }

    pub fn remove_habitat_for_topology_cell_if_it_matches(
        &mut self,
        topology_cell: IVec2,
        habitat: Entity,
    ) {
        if self.habitat_for_topology_cell(topology_cell) == Some(habitat) {
            self.habitats_by_topology_cell.remove(&topology_cell);
        }
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HabitatChanged {
    pub habitat_entity: Entity,
    pub changed_topology_bounds: IRect,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainmentChanged {
    pub affected_entity: Entity,
    pub habitat_entity: Option<Entity>,
    pub is_contained: bool,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RebuildHabitatRegionsWithinTopologyBounds(pub IRect);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshTerrainDerivedHabitatSummariesWithinTopologyBounds(pub IRect);
