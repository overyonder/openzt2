use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::HabitatIndex,
    },
    locomotion::locomotion_types::{ActiveTerrainDerivedNavigationGraph, NavAgent, NavFlags},
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_water_presentation_types::{NaturalWaterRegion, NaturalWaterRegionCells},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
    },
    topology::topology_graph_types::TopologyGrid,
};

use openzt2_zoo_state::animal_behavior::behavior_natural_water_target_types::{
    NaturalWaterTargetKind, SelectedNaturalWaterTarget,
};

#[derive(SystemParam)]
pub(super) struct NaturalWaterTargets<'w, 's> {
    regions: Query<
        'w,
        's,
        (Entity, &'static NaturalWaterRegionCells, &'static ChildOf),
        With<NaturalWaterRegion>,
    >,
    worlds: Query<'w, 's, &'static WorldMember>,
    graph: Option<Res<'w, ActiveTerrainDerivedNavigationGraph>>,
    pub agents: Query<'w, 's, &'static NavAgent>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    assets: Res<'w, Assets<TerrainAsset>>,
    index: Res<'w, TerrainIndex>,
    habitats: Res<'w, HabitatIndex>,
    grid: Res<'w, TopologyGrid>,
}

impl NaturalWaterTargets<'_, '_> {
    pub(super) fn collect_water(
        &self,
        world: Entity,
        position: Vec3,
        radius: f32,
        output: &mut Vec<SelectedNaturalWaterTarget>,
    ) {
        output.clear();
        for (region, cells, parent) in &self.regions {
            if !self
                .worlds
                .get(parent.parent())
                .is_ok_and(|member| member.root == world)
            {
                continue;
            }
            for &point in &cells.centres {
                if point.xz().distance_squared(position.xz()) >= radius * radius
                    || !self.same_habitat(point, position)
                {
                    continue;
                }
                let Some((chunk, edited)) = terrain_chunk_at(&self.index, point.xz())
                    .and_then(|entity| self.chunks.get(entity).ok())
                else {
                    continue;
                };
                let Some(sample) = self
                    .assets
                    .get(&chunk.asset)
                    .and_then(|asset| sample_terrain(chunk, asset, edited, point.xz()))
                else {
                    continue;
                };
                let Some(height) = sample.water_height_m else {
                    continue;
                };
                let water = Vec3::new(point.x, height, point.z);
                output.push(SelectedNaturalWaterTarget {
                    region,
                    water,
                    approach: water,
                    kind: NaturalWaterTargetKind::Surface {
                        depth: height - sample.height_m,
                    },
                });
            }
        }
    }
    pub(super) fn same_habitat(&self, first: Vec3, second: Vec3) -> bool {
        let first = locate_habitat_at_world_position(&self.habitats, first.xz(), *self.grid);
        first.is_some()
            && first == locate_habitat_at_world_position(&self.habitats, second.xz(), *self.grid)
    }

    pub(super) fn water_height(&self, point: Vec2) -> Option<f32> {
        let (chunk, edited) = self
            .chunks
            .get(terrain_chunk_at(&self.index, point)?)
            .ok()?;
        sample_terrain(chunk, self.assets.get(&chunk.asset)?, edited, point)?.water_height_m
    }

    pub(super) fn valid(&self, target: &SelectedNaturalWaterTarget) -> bool {
        self.regions.contains(target.region)
            && self
                .water_height(target.water.xz())
                .is_some_and(|height| (height - target.water.y).abs() < 0.01)
            && match target.kind {
                NaturalWaterTargetKind::Shore => true,
                NaturalWaterTargetKind::Surface { depth } => self
                    .water_depth(target.water.xz())
                    .is_some_and(|current| (depth - current).abs() < 0.01),
            }
    }

    pub(super) fn water_depth(&self, point: Vec2) -> Option<f32> {
        let (chunk, edited) = self
            .chunks
            .get(terrain_chunk_at(&self.index, point)?)
            .ok()?;
        let sample = sample_terrain(chunk, self.assets.get(&chunk.asset)?, edited, point)?;
        Some(sample.water_height_m? - sample.height_m)
    }

    pub(super) fn docking_endpoint(
        &self,
        actor: Entity,
        point: Vec3,
        shore: &SelectedNaturalWaterTarget,
    ) -> bool {
        let Some((chunk, edited)) = terrain_chunk_at(&self.index, point.xz())
            .and_then(|entity| self.chunks.get(entity).ok())
        else {
            return false;
        };
        let Some(sample) = self
            .assets
            .get(&chunk.asset)
            .and_then(|asset| sample_terrain(chunk, asset, edited, point.xz()))
        else {
            return false;
        };
        self.dry_endpoint(actor, point)
            && self.same_habitat(point, shore.approach)
            && self
                .agents
                .get(actor)
                .is_ok_and(|agent| (sample.height_m - point.y).abs() <= agent.radius_m.max(0.05))
    }

    pub(super) fn dry_endpoint(&self, actor: Entity, point: Vec3) -> bool {
        let Ok(agent) = self.agents.get(actor) else {
            return false;
        };
        let Some(graph) = self.graph.as_ref() else {
            return false;
        };
        let graph = graph.terrain_derived_navigation_graph();
        let size = f32::from(graph.terrain_cell_size_centimetres) * 0.01;
        if !point.is_finite() || point.x < 0.0 || point.z > 0.0 || size <= 0.0 {
            return false;
        }
        let Some((chunk, edited)) = terrain_chunk_at(&self.index, point.xz())
            .and_then(|entity| self.chunks.get(entity).ok())
        else {
            return false;
        };
        let Some(sample) = self
            .assets
            .get(&chunk.asset)
            .and_then(|asset| sample_terrain(chunk, asset, edited, point.xz()))
        else {
            return false;
        };
        graph
            .calculate_navigation_node_for_terrain_cell([
                (point.x / size).floor() as u32,
                (-point.z / size).floor() as u32,
            ])
            .and_then(|index| graph.find_navigation_node(index))
            .is_some_and(|node| {
                node.eligibility_flags.0 & agent.capabilities.0 & NavFlags::ELIGIBILITY_BITS != 0
                    && !node.eligibility_flags.contains(NavFlags::WATER)
                    && !node
                        .eligibility_flags
                        .contains(NavFlags::DISABLED_BY_DEFAULT)
                    && f32::from(node.clearance_centimetres) >= agent.radius_m * 100.0
            })
            && sample.water_height_m.is_none()
    }

    pub(super) fn collect(
        &self,
        actor: Entity,
        world: Entity,
        position: Vec3,
        radius: f32,
        output: &mut Vec<SelectedNaturalWaterTarget>,
    ) {
        output.clear();
        let habitat_at =
            |point: Vec3| locate_habitat_at_world_position(&self.habitats, point.xz(), *self.grid);
        let Some(habitat) = habitat_at(position) else {
            return;
        };
        for (region, cells, parent) in &self.regions {
            if !cells.cell_size_m.is_finite() || cells.cell_size_m <= 0.0 {
                continue;
            }
            if !self
                .worlds
                .get(parent.parent())
                .is_ok_and(|owner| owner.root == world)
            {
                continue;
            }
            for &wet in &cells.centres {
                if position.xz().distance_squared(wet.xz()) > (radius + cells.cell_size_m).powi(2)
                    || self.water_height(wet.xz()).is_none()
                {
                    continue;
                }
                for direction in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
                    let mut dry = wet + direction * cells.cell_size_m;
                    if !self.dry_endpoint(actor, dry) || habitat_at(dry) != Some(habitat) {
                        continue;
                    }
                    let Some(chunk_entity) = terrain_chunk_at(&self.index, dry.xz()) else {
                        continue;
                    };
                    let Ok((chunk, edited)) = self.chunks.get(chunk_entity) else {
                        continue;
                    };
                    let Some(sample) = self
                        .assets
                        .get(&chunk.asset)
                        .and_then(|asset| sample_terrain(chunk, asset, edited, dry.xz()))
                    else {
                        continue;
                    };
                    dry.y = sample.height_m;
                    let mut wet_edge = wet;
                    let mut dry_edge = dry;
                    // Resolve the boundary to better than one terrain centimetre.
                    while wet_edge.xz().distance_squared(dry_edge.xz()) > 0.0001 {
                        let midpoint = (wet_edge + dry_edge) * 0.5;
                        if self.water_height(midpoint.xz()).is_some() {
                            wet_edge = midpoint;
                        } else {
                            dry_edge = midpoint;
                        }
                    }
                    let Some(height) = self.water_height(wet_edge.xz()) else {
                        continue;
                    };
                    wet_edge.y = height;
                    if position.xz().distance_squared(wet_edge.xz()) >= radius * radius
                        || habitat_at(wet_edge) != Some(habitat)
                    {
                        continue;
                    }
                    output.push(SelectedNaturalWaterTarget {
                        region,
                        water: wet_edge,
                        approach: dry,
                        kind: NaturalWaterTargetKind::Shore,
                    });
                }
            }
        }
    }
}
