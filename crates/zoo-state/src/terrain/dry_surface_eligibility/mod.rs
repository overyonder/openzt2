use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use crate::aquatic::aquatic_simulation_types::{Tank, TankGeometry};
use crate::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use crate::habitat::habitat_types::HabitatIndex;
use crate::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex};
use crate::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionQuery,
};
use crate::terrain::terrain_world_sampling::{sample_terrain, terrain_chunk_at};
use crate::topology::topology_graph_types::TopologyGrid;

/// Borrows the same resolved water membership used by rendering and physics.
#[derive(SystemParam)]
pub struct TerrainDrySurfaceEligibility<'w, 's> {
    index: Res<'w, TerrainIndex>,
    assets: Res<'w, Assets<TerrainAsset>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    membership: Query<'w, 's, &'static TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery<'w, 's>,
}

impl TerrainDrySurfaceEligibility<'_, '_> {
    /// Terrain water only; [`DrySurfaceEligibility`] also includes tank water.
    pub fn on_land(&self, world: Vec2) -> Option<bool> {
        let entity = terrain_chunk_at(&self.index, world)?;
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
    fn water_surface_height(&self, world: Vec2) -> Option<f32> {
        let entity = terrain_chunk_at(&self.index, world)?;
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

    fn ground_height(&self, world: Vec2) -> Option<f32> {
        let (chunk, edited) = self
            .chunks
            .get(terrain_chunk_at(&self.index, world)?)
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
    habitats: Res<'w, HabitatIndex>,
    topology: Res<'w, TopologyGrid>,
    tanks: Query<'w, 's, &'static TankGeometry, With<Tank>>,
}

impl DrySurfaceEligibility<'_, '_> {
    /// Terrain height under a world position.
    pub fn ground_height(&self, world: Vec2) -> Option<f32> {
        self.terrain.ground_height(world)
    }

    /// The water surface over a world position, tank water included, and
    /// `None` on dry ground.
    pub fn water_surface_height(&self, world: Vec2) -> Option<f32> {
        let tank = locate_habitat_at_world_position(&self.habitats, world, *self.topology)
            .and_then(|habitat| self.tanks.get(habitat).ok());
        match tank {
            Some(tank) => (self.terrain.ground_height(world)? < tank.water_height)
                .then_some(tank.water_height),
            None => self.terrain.water_surface_height(world),
        }
    }

    pub fn on_land(&self, world: Vec2) -> Option<bool> {
        let tank = locate_habitat_at_world_position(&self.habitats, world, *self.topology)
            .and_then(|habitat| self.tanks.get(habitat).ok());
        match tank {
            Some(tank) => Some(self.terrain.ground_height(world)? >= tank.water_height),
            None => self.terrain.on_land(world),
        }
    }

    /// Entity qualifiers compare the target's height with tank water, so an
    /// animal above the surface is not classified as submerged by the ground.
    pub fn on_land_at_position(&self, position: Vec3) -> Option<bool> {
        let world = position.xz();
        let tank = locate_habitat_at_world_position(&self.habitats, world, *self.topology)
            .and_then(|habitat| self.tanks.get(habitat).ok());
        match tank {
            Some(tank) => Some(position.y >= tank.water_height),
            None => self.terrain.on_land(world),
        }
    }
}
