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

#[derive(Component, Debug, Default)]
pub struct HabitatIndex {
    cells_by_coordinate: HashMap<IVec2, Entity>,
    partitioned_cells: HashMap<IVec2, [Option<Entity>; 4]>,
}

impl HabitatIndex {
    pub fn cell_at(&self, coordinate: IVec2) -> Option<Entity> {
        self.cells_by_coordinate.get(&coordinate).copied()
    }

    pub fn record_cell(&mut self, coordinate: IVec2, cell: Entity) {
        if self.cell_at(coordinate) != Some(cell) {
            self.cells_by_coordinate.insert(coordinate, cell);
        }
    }

    pub fn remove_cell(&mut self, coordinate: IVec2, cell: Entity) {
        if self.cell_at(coordinate) == Some(cell) {
            self.cells_by_coordinate.remove(&coordinate);
            self.partitioned_cells.remove(&coordinate);
        }
    }

    /// Each entry names the sector cell bearing the live habitat relationship.
    /// An empty sector of a partitioned tile is outside every habitat.
    pub fn set_cell_partition(&mut self, coordinate: IVec2, cells: Option<[Option<Entity>; 4]>) {
        if let Some(cells) = cells {
            self.partitioned_cells.insert(coordinate, cells);
        } else {
            self.partitioned_cells.remove(&coordinate);
        }
    }

    pub fn cell_in_sector(&self, coordinate: IVec2, sector: HabitatSector) -> Option<Entity> {
        match self.partitioned_cells.get(&coordinate) {
            Some(cells) => cells[sector as usize],
            None => self.cell_at(coordinate),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HabitatSector {
    North,
    East,
    South,
    West,
}

impl HabitatSector {
    pub fn from_centre_offset(offset_xz: Vec2) -> Self {
        let x = offset_xz.x;
        let y = -offset_xz.y;
        match (y >= x, y > -x) {
            (true, true) => Self::North,
            (false, true) => Self::East,
            (false, false) => Self::South,
            (true, false) => Self::West,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HabitatGridCell(pub IVec2);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = HabitatGridCells)]
pub struct HabitatCellOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = HabitatCellOf)]
pub struct HabitatGridCells(Vec<Entity>);

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
