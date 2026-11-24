use crate::aquatic::aquatic_simulation_types::{Tank, TankGeometry};
use crate::construction::construction_interaction_types::PlacementFailure;
use crate::habitat::habitat_types::Habitat;
use crate::shows::show_stage_types::HabitatShowStages;
use crate::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex};
use crate::terrain::terrain_world_sampling::{sample_terrain_surface, terrain_chunk_at};
use bevy::ecs::{relationship::RelationshipTarget, system::SystemParam};
use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::world_definitions::object_placement::PlacementSurfacePolicy;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

#[derive(SystemParam)]
/// Whether an actor or object may stand at a site, by its authored land, water,
/// tank and show tank placement rules and its depth limits.
pub struct PlacementSiteQueries<'w, 's> {
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    terrain_chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    habitats: Query<'w, 's, &'static WorldMember, With<Habitat>>,
    roots: Query<'w, 's, &'static TerrainIndex, With<WorldRoot>>,
    tanks: Query<'w, 's, (&'static TankGeometry, Option<&'static HabitatShowStages>), With<Tank>>,
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
            || habitat.is_some_and(|habitat| {
                self.habitats
                    .get(habitat)
                    .map_or(true, |member| member.root != world)
            })
        {
            return Err(PlacementFailure::OutsideMap);
        }
        let terrain_index = self
            .roots
            .get(world)
            .map_err(|_| PlacementFailure::OutsideMap)?;
        let chunk = terrain_chunk_at(terrain_index, position.xz())
            .and_then(|entity| self.terrain_chunks.get(entity).ok())
            .ok_or(PlacementFailure::OutsideMap)?;
        let asset = self
            .terrain_assets
            .get(&chunk.0.asset)
            .ok_or(PlacementFailure::OutsideMap)?;
        let surface = sample_terrain_surface(chunk.0, asset, chunk.1, position.xz())
            .ok_or(PlacementFailure::OutsideMap)?;
        let terrain = surface.geometry;
        let tank = habitat.and_then(|habitat| self.tanks.get(habitat).ok());
        if tank.is_some_and(|(geometry, _)| !geometry.is_valid()) {
            return Err(PlacementFailure::InvalidHabitat);
        }
        let submerged_tank = tank.filter(|(geometry, _)| geometry.water_height >= terrain.height_m);
        let (allowed, depth) = if let Some((geometry, stages)) = submerged_tank {
            let show_tank = stages.is_some_and(|stages| !stages.is_empty());
            (
                if show_tank {
                    policy.show_tank
                } else {
                    policy.tank
                },
                Some(if policy.use_actual_depth_in_water_collision {
                    surface.depth_bounds(geometry.water_height)
                } else {
                    [geometry.depth(); 2]
                }),
            )
        } else if let Some(water) = terrain
            .water_height_m
            .filter(|height| *height >= terrain.height_m)
        {
            (policy.water, Some(surface.depth_bounds(water)))
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
