use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use crate::aquatic::aquatic_simulation_types::{Tank, TankGeometry};
use crate::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use crate::habitat::habitat_types::{HabitatCellOf, HabitatIndex};
use crate::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex};
use crate::terrain::terrain_sample_grid_queries::{
    calculate_row_major_terrain_sample_index, read_authored_terrain_sample,
};
use crate::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery,
};
use crate::terrain::terrain_world_sampling::{sample_terrain, terrain_chunk_at};
use crate::topology::topology_graph_types::TopologyGrid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainGroundKind {
    Dirt,
    Cover,
    Shore,
    Water,
    Cliff,
}

impl TerrainGroundKind {
    pub fn dictionary_key(self) -> openzt2_game_data::AssetId {
        openzt2_game_data::AssetId::from_key(match self {
            Self::Dirt => "dirt",
            Self::Cover => "cover",
            Self::Shore => "shore",
            Self::Water => "water",
            Self::Cliff => "cliff",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainGroundContext {
    pub ground: TerrainGroundKind,
    pub underwater: bool,
}

/// Borrows the same resolved water membership used by rendering and physics.
#[derive(SystemParam)]
pub struct TerrainDrySurfaceEligibility<'w, 's> {
    indexes: Query<'w, 's, &'static TerrainIndex>,
    assets: Res<'w, Assets<TerrainAsset>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    membership: Query<'w, 's, &'static TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery<'w, 's>,
}

impl TerrainDrySurfaceEligibility<'_, '_> {
    /// Terrain water only; [`DrySurfaceEligibility`] also includes tank water.
    pub fn on_land(&self, root: Entity, world: Vec2) -> Option<bool> {
        let entity = terrain_chunk_at(self.indexes.get(root).ok()?, world)?;
        let (chunk, edited) = self.chunks.get(entity).ok()?;
        let asset = self.assets.get(&chunk.asset)?;
        let membership = self
            .membership
            .iter()
            .find(|membership| membership.terrain() == chunk.asset.id())?;
        let Some(owner) = membership.owner_at_world_xz(world, asset)? else {
            return Some(true);
        };
        let surface = sample_terrain(chunk, asset, edited, world)?;
        Some(surface.height_m >= self.regions.get(owner)?.surface_height(asset)?)
    }

    /// The terrain water surface over a world position, if water covers it.
    fn water_surface_height(&self, root: Entity, world: Vec2) -> Option<f32> {
        let entity = terrain_chunk_at(self.indexes.get(root).ok()?, world)?;
        let (chunk, edited) = self.chunks.get(entity).ok()?;
        let asset = self.assets.get(&chunk.asset)?;
        let membership = self
            .membership
            .iter()
            .find(|membership| membership.terrain() == chunk.asset.id())?;
        let owner = membership.owner_at_world_xz(world, asset)??;
        let surface = self.regions.get(owner)?.surface_height(asset)?;
        (sample_terrain(chunk, asset, edited, world)?.height_m < surface).then_some(surface)
    }

    fn ground_height(&self, root: Entity, world: Vec2) -> Option<f32> {
        let (chunk, edited) = self
            .chunks
            .get(terrain_chunk_at(self.indexes.get(root).ok()?, world)?)
            .ok()?;
        Some(sample_terrain(chunk, self.assets.get(&chunk.asset)?, edited, world)?.height_m)
    }
}

/// Whether a world position is dry ground, as the `onLand` and `inWater`
/// qualifiers ask. A tank's water is not terrain water: ground inside a tank
/// is wet below that tank's water surface, and dry on a floor raised to or
/// above it.
#[derive(SystemParam)]
pub struct DrySurfaceEligibility<'w, 's> {
    terrain: TerrainDrySurfaceEligibility<'w, 's>,
    roots: Query<'w, 's, (&'static HabitatIndex, &'static TopologyGrid)>,
    cell_owners: Query<'w, 's, &'static HabitatCellOf>,
    tanks: Query<'w, 's, &'static TankGeometry, With<Tank>>,
}

impl DrySurfaceEligibility<'_, '_> {
    pub fn dictionary_ground(&self, root: Entity, position: Vec3) -> Option<TerrainGroundContext> {
        let world = position.xz();
        let entity = terrain_chunk_at(self.terrain.indexes.get(root).ok()?, world)?;
        let (chunk, edited) = self.terrain.chunks.get(entity).ok()?;
        let asset = self.terrain.assets.get(&chunk.asset)?;
        let surface = sample_terrain(chunk, asset, edited, world)?;
        let cell = ((world - chunk.origin) / chunk.spacing_m)
            .round()
            .as_uvec2();
        let local =
            calculate_row_major_terrain_sample_index(chunk.side, cell.x as usize, cell.y as usize)?;
        let sample = read_authored_terrain_sample(chunk, asset, cell.x as usize, cell.y as usize)?;
        let cover = edited
            .and_then(|edits| edits.ground_cover.get(local))
            .map_or(sample.has_ground_cover, |cover| *cover != 0);
        let membership = self
            .terrain
            .membership
            .iter()
            .find(|membership| membership.terrain() == chunk.asset.id())?;
        let water = membership
            .owner_at_world_xz(world, asset)?
            .and_then(|owner| self.terrain.regions.get(owner)?.surface_height(asset));
        let [axis_slope, diagonal_slope] = asset
            .canonical_terrain_grid()
            .presentation
            .cliff_rise_per_metre;
        let rise = surface.normal.xz().abs();
        let exceeds =
            |gradient: f32, authored: f32| authored > 0.0 && gradient > authored * surface.normal.y;
        let ground = if exceeds(rise.max_element(), axis_slope)
            || exceeds(
                (rise.x + rise.y) * std::f32::consts::FRAC_1_SQRT_2,
                diagonal_slope,
            ) {
            TerrainGroundKind::Cliff
        } else if let Some(water) = water {
            let height = edited
                .and_then(|edits| edits.heights_cm.get(local))
                .map_or_else(
                    || {
                        asset.canonical_terrain_grid().units.base_height_metres
                            + sample.height_relative_to_base_metres
                    },
                    |height| f32::from(*height) * 0.01,
                );
            if height < water {
                TerrainGroundKind::Water
            } else {
                TerrainGroundKind::Shore
            }
        } else if cover {
            TerrainGroundKind::Cover
        } else {
            TerrainGroundKind::Dirt
        };
        Some(TerrainGroundContext {
            ground,
            underwater: water.is_some_and(|water| position.y < water),
        })
    }

    /// Terrain height under a world position.
    pub fn ground_height(&self, root: Entity, world: Vec2) -> Option<f32> {
        self.terrain.ground_height(root, world)
    }

    /// The water surface over a world position, tank water included, and
    /// `None` on dry ground.
    pub fn water_surface_height(&self, root: Entity, world: Vec2) -> Option<f32> {
        let tank = self
            .roots
            .get(root)
            .ok()
            .and_then(|(habitats, topology)| {
                locate_habitat_at_world_position(habitats, &self.cell_owners, world, *topology)
            })
            .and_then(|habitat| self.tanks.get(habitat).ok());
        match tank {
            Some(tank) => (self.terrain.ground_height(root, world)? < tank.water_height)
                .then_some(tank.water_height),
            None => self.terrain.water_surface_height(root, world),
        }
    }

    pub fn on_land(&self, root: Entity, world: Vec2) -> Option<bool> {
        let tank = self
            .roots
            .get(root)
            .ok()
            .and_then(|(habitats, topology)| {
                locate_habitat_at_world_position(habitats, &self.cell_owners, world, *topology)
            })
            .and_then(|habitat| self.tanks.get(habitat).ok());
        match tank {
            Some(tank) => Some(self.terrain.ground_height(root, world)? >= tank.water_height),
            None => self.terrain.on_land(root, world),
        }
    }

    /// Entity qualifiers compare the target's height with tank water, so an
    /// animal above the surface is not classified as submerged by the ground.
    pub fn on_land_at_position(&self, root: Entity, position: Vec3) -> Option<bool> {
        let world = position.xz();
        let tank = self
            .roots
            .get(root)
            .ok()
            .and_then(|(habitats, topology)| {
                locate_habitat_at_world_position(habitats, &self.cell_owners, world, *topology)
            })
            .and_then(|habitat| self.tanks.get(habitat).ok());
        match tank {
            Some(tank) => Some(position.y >= tank.water_height),
            None => self.terrain.on_land(root, world),
        }
    }
}
