use crate::aquatic::aquatic_simulation_types::{Tank, TankGeometry};
use crate::construction::construction_interaction_types::PlacementFailure;
use crate::habitat::habitat_types::{Habitat, HabitatRegion};
use crate::shows::show_stage_types::ShowStage;
use crate::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex};
use crate::terrain::terrain_world_sampling::{
    sample_terrain, sample_terrain_triangle_height_bounds, terrain_chunk_at,
};
use crate::topology::topology_graph_types::TopologyGrid;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::world_definitions::object_placement::PlacementSurfacePolicy;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

#[derive(SystemParam)]
/// Whether an actor or object may stand at a site, by its authored land, water,
/// tank and show tank placement rules and its depth limits.
pub struct PlacementSiteQueries<'w, 's> {
    terrain_index: Res<'w, TerrainIndex>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    terrain_chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    habitats: Query<'w, 's, &'static WorldMember, With<Habitat>>,
    roots: Query<'w, 's, Entity, With<WorldRoot>>,
    tanks: Query<'w, 's, (&'static TankGeometry, &'static HabitatRegion), With<Tank>>,
    stages: Query<'w, 's, (&'static Transform, &'static WorldMember), With<ShowStage>>,
    grid: Res<'w, TopologyGrid>,
}

impl PlacementSiteQueries<'_, '_> {
    pub fn validate(
        &self,
        policy: PlacementSurfacePolicy,
        world: Entity,
        habitat: Option<Entity>,
        position: Vec3,
    ) -> Result<(), PlacementFailure> {
        if !position.is_finite()
            || !self.roots.contains(world)
            || habitat.is_some_and(|habitat| {
                self.habitats
                    .get(habitat)
                    .map_or(true, |member| member.root != world)
            })
        {
            return Err(PlacementFailure::OutsideMap);
        }
        let chunk = terrain_chunk_at(&self.terrain_index, position.xz())
            .and_then(|entity| self.terrain_chunks.get(entity).ok())
            .ok_or(PlacementFailure::OutsideMap)?;
        let asset = self
            .terrain_assets
            .get(&chunk.0.asset)
            .ok_or(PlacementFailure::OutsideMap)?;
        let terrain = sample_terrain(chunk.0, asset, chunk.1, position.xz())
            .ok_or(PlacementFailure::OutsideMap)?;
        let depth_bounds = |water_height: f32| {
            sample_terrain_triangle_height_bounds(chunk.0, asset, chunk.1, position.xz())
                .map(|[lowest, highest]| {
                    [
                        (water_height - highest).max(0.0),
                        (water_height - lowest).max(0.0),
                    ]
                })
                .ok_or(PlacementFailure::OutsideMap)
        };
        let tank = habitat.and_then(|habitat| self.tanks.get(habitat).ok());
        if tank.is_some_and(|(geometry, _)| !geometry.is_valid()) {
            return Err(PlacementFailure::InvalidHabitat);
        }
        let submerged_tank = tank.filter(|(geometry, _)| geometry.water_height >= terrain.height_m);
        let (allowed, depth) = if let Some((geometry, region)) = submerged_tank {
            let show_tank = self.stages.iter().any(|(stage, member)| {
                if member.root != world {
                    return false;
                }
                let cell = ((stage.translation.xz() - self.grid.origin) / self.grid.spacing_m)
                    .floor()
                    .as_ivec2();
                region
                    .topology_cells
                    .binary_search_by_key(&(cell.x, cell.y), |cell| (cell.x, cell.y))
                    .is_ok()
            });
            (
                if show_tank {
                    policy.show_tank
                } else {
                    policy.tank
                },
                Some(if policy.use_actual_depth_in_water_collision {
                    depth_bounds(geometry.water_height)?
                } else {
                    [geometry.depth(); 2]
                }),
            )
        } else if let Some(water) = terrain
            .water_height_m
            .filter(|height| *height >= terrain.height_m)
        {
            (policy.water, Some(depth_bounds(water)?))
        } else {
            (policy.land, None)
        };
        if !allowed {
            return Err(PlacementFailure::InvalidHabitat);
        }
        if let Some([minimum_depth, maximum_depth]) = depth {
            let maximum = if policy.maximum_depth_metres == -1.0 {
                1000.0
            } else {
                policy.maximum_depth_metres
            };
            if policy.minimum_depth_metres > 0.0
                && minimum_depth < policy.minimum_depth_metres - 0.00001
                || maximum > 0.0 && maximum_depth > maximum
            {
                return Err(PlacementFailure::InvalidHabitat);
            }
        }
        Ok(())
    }
}
