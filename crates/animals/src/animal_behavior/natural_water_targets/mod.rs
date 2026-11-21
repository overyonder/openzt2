use bevy::{
    ecs::system::SystemParam,
    math::I64Vec2,
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
};
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    animal_behavior::behavior_natural_water_target_types::{
        NaturalWaterTargetKind, SelectedNaturalWaterTarget,
    },
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::{HabitatIndex, HabitatSummary},
    },
    locomotion::locomotion_types::{ActiveTerrainDerivedNavigationGraph, NavAgent, NavFlags},
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_water_presentation_types::{NaturalWaterRegion, NaturalWaterRegionCells},
        terrain_world_sampling::{sample_terrain, terrain_chunk_at},
        terrain_world_sampling_types::TerrainPoint,
    },
    topology::topology_graph_types::TopologyGrid,
};

#[derive(Component, Default)]
struct ChunkWaterTargets {
    sites: Vec<WaterTargetSite>,
    surfaces: Vec<ProjectedWaterTarget>,
    shores: Vec<ProjectedWaterTarget>,
}

struct WaterTargetSite {
    order: usize,
    region: Entity,
    world: Entity,
    centre: Vec3,
    cell_size: f32,
}

struct ProjectedWaterTarget {
    order: usize,
    world: Entity,
    target: SelectedNaturalWaterTarget,
}

#[derive(Resource, Default, PartialEq)]
struct WaterTargetSearchMargin(f32);

#[derive(SystemParam)]
struct WaterTerrain<'w, 's> {
    index: Res<'w, TerrainIndex>,
    assets: Res<'w, Assets<TerrainAsset>>,
    chunks: Query<
        'w,
        's,
        (
            Ref<'static, TerrainChunk>,
            Option<Ref<'static, EditedTerrainSamples>>,
        ),
    >,
}

impl WaterTerrain<'_, '_> {
    fn sample(&self, point: Vec2) -> Option<TerrainPoint> {
        let (chunk, edited) = self
            .chunks
            .get(terrain_chunk_at(&self.index, point)?)
            .ok()?;
        sample_terrain(
            &chunk,
            self.assets.get(&chunk.asset)?,
            edited.as_deref(),
            point,
        )
    }

    fn changed_at(&self, point: Vec2) -> bool {
        self.index.is_changed()
            || self.assets.is_changed()
            || terrain_chunk_at(&self.index, point)
                .and_then(|chunk| self.chunks.get(chunk).ok())
                .is_none_or(|(chunk, edited)| {
                    chunk.is_changed() || edited.is_some_and(|edits| edits.is_changed())
                })
    }
}

pub(super) fn register_water_target_projection(app: &mut App) {
    app.init_resource::<WaterTargetSearchMargin>().add_systems(
        FixedUpdate,
        project_chunk_water_targets
            .in_set(FixedGameSet::Clock)
            .run_if(in_state(GamePhase::InGame)),
    );
}

fn project_chunk_water_targets(
    terrain: WaterTerrain,
    regions: Query<(Entity, &NaturalWaterRegionCells, &ChildOf), With<NaturalWaterRegion>>,
    worlds: Query<&WorldMember>,
    changed_regions: Query<
        (),
        (
            With<NaturalWaterRegion>,
            Or<(
                Added<NaturalWaterRegion>,
                Changed<NaturalWaterRegionCells>,
                Changed<ChildOf>,
            )>,
        ),
    >,
    changed_terrain: Query<
        Entity,
        (
            With<TerrainChunk>,
            Or<(Changed<TerrainChunk>, Changed<EditedTerrainSamples>)>,
        ),
    >,
    mut removed_regions: RemovedComponents<NaturalWaterRegion>,
    mut removed_region_cells: RemovedComponents<NaturalWaterRegionCells>,
    mut removed_edits: RemovedComponents<EditedTerrainSamples>,
    mut projected: Query<(Entity, &mut ChunkWaterTargets)>,
    mut margin: ResMut<WaterTargetSearchMargin>,
    mut commands: Commands,
) {
    let regions_removed = removed_regions.read().count() + removed_region_cells.read().count() != 0;
    if terrain.index.is_changed()
        || terrain.assets.is_changed()
        || regions_removed
        || !changed_regions.is_empty()
    {
        removed_edits.clear();
        let mut targets_by_chunk = HashMap::<Entity, ChunkWaterTargets>::default();
        let mut maximum_cell_size = 0.0_f32;
        let mut order = 0;
        for (region, cells, parent) in &regions {
            let Ok(world) = worlds.get(parent.parent()) else {
                continue;
            };
            let cell_size = if cells.cell_size_m.is_finite() {
                cells.cell_size_m.max(0.0)
            } else {
                0.0
            };
            maximum_cell_size = maximum_cell_size.max(cell_size);
            for &centre in &cells.centres {
                let Some(chunk) = terrain_chunk_at(&terrain.index, centre.xz()) else {
                    continue;
                };
                targets_by_chunk
                    .entry(chunk)
                    .or_default()
                    .sites
                    .push(WaterTargetSite {
                        order,
                        region,
                        world: world.root,
                        centre,
                        cell_size,
                    });
                // Reserve one surface and four ordered shore positions per site.
                order += 5;
            }
        }
        margin.set_if_neq(WaterTargetSearchMargin(maximum_cell_size));
        for targets in targets_by_chunk.values_mut() {
            project_water_sites(&terrain, targets);
        }
        for (chunk, mut existing) in &mut projected {
            if let Some(next) = targets_by_chunk.remove(&chunk) {
                *existing = next;
            } else {
                commands.entity(chunk).remove::<ChunkWaterTargets>();
            }
        }
        for (chunk, targets) in targets_by_chunk {
            commands.entity(chunk).insert(targets);
        }
        return;
    }

    let terrain_edits: HashSet<Entity> =
        changed_terrain.iter().chain(removed_edits.read()).collect();
    if terrain_edits.is_empty() || terrain.index.chunk_span_m <= 0.0 {
        return;
    }
    let chunks = &terrain.index.chunks;
    let reach = (margin.0 / terrain.index.chunk_span_m).ceil() as i64;
    let mut affected = HashSet::<Entity>::new();
    for (&coordinate, _) in chunks
        .iter()
        .filter(|(_, chunk)| terrain_edits.contains(*chunk))
    {
        if reach.saturating_mul(reach).saturating_mul(4) > chunks.len() as i64 {
            affected.extend(chunks.iter().filter_map(|(&other, &chunk)| {
                let distance = (other.as_i64vec2() - coordinate.as_i64vec2()).abs();
                (distance.max_element() <= reach).then_some(chunk)
            }));
        } else {
            for x in -reach..=reach {
                for z in -reach..=reach {
                    let cell = coordinate.as_i64vec2() + I64Vec2::new(x, z);
                    if let Some(&chunk) = chunks.get(&cell.as_ivec2()) {
                        affected.insert(chunk);
                    }
                }
            }
        }
    }
    for chunk in affected {
        if let Ok((_, mut targets)) = projected.get_mut(chunk) {
            project_water_sites(&terrain, &mut targets);
        }
    }
}

fn project_water_sites(terrain: &WaterTerrain<'_, '_>, targets: &mut ChunkWaterTargets) {
    targets.surfaces.clear();
    targets.shores.clear();
    for site in &targets.sites {
        let wet = site.centre;
        let Some(sample) = terrain.sample(wet.xz()) else {
            continue;
        };
        let Some(height) = sample.water_height_m else {
            continue;
        };
        let water = wet.with_y(height);
        targets.surfaces.push(ProjectedWaterTarget {
            order: site.order,
            world: site.world,
            target: SelectedNaturalWaterTarget {
                region: site.region,
                water,
                approach: water,
                kind: NaturalWaterTargetKind::Surface {
                    depth: height - sample.height_m,
                },
            },
        });
        if site.cell_size == 0.0 {
            continue;
        }
        for (direction_index, direction) in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z]
            .into_iter()
            .enumerate()
        {
            let mut dry = wet + direction * site.cell_size;
            let Some(dry_sample) = terrain.sample(dry.xz()) else {
                continue;
            };
            if dry_sample.water_height_m.is_some() {
                continue;
            }
            dry.y = dry_sample.height_m;
            let mut wet_edge = water;
            let mut dry_edge = dry;
            // Resolve the boundary to one terrain centimetre.
            while wet_edge.xz().distance_squared(dry_edge.xz()) > 0.0001 {
                let midpoint = (wet_edge + dry_edge) * 0.5;
                if midpoint.xz() == wet_edge.xz() || midpoint.xz() == dry_edge.xz() {
                    break;
                }
                if let Some(height) = terrain
                    .sample(midpoint.xz())
                    .and_then(|sample| sample.water_height_m)
                {
                    wet_edge = midpoint.with_y(height);
                } else {
                    dry_edge = midpoint;
                }
            }
            targets.shores.push(ProjectedWaterTarget {
                order: site.order + direction_index + 1,
                world: site.world,
                target: SelectedNaturalWaterTarget {
                    region: site.region,
                    water: wet_edge,
                    approach: dry,
                    kind: NaturalWaterTargetKind::Shore,
                },
            });
        }
    }
}

#[derive(SystemParam)]
pub(super) struct NaturalWaterTargets<'w, 's> {
    terrain: WaterTerrain<'w, 's>,
    regions: Query<'w, 's, Ref<'static, NaturalWaterRegionCells>, With<NaturalWaterRegion>>,
    chunk_targets: Query<'w, 's, Ref<'static, ChunkWaterTargets>>,
    search_margin: Res<'w, WaterTargetSearchMargin>,
    graph: Option<Res<'w, ActiveTerrainDerivedNavigationGraph>>,
    pub(super) agents: Query<'w, 's, Ref<'static, NavAgent>>,
    habitats: Res<'w, HabitatIndex>,
    habitat_summaries: Query<'w, 's, Ref<'static, HabitatSummary>>,
    grid: Res<'w, TopologyGrid>,
}

impl NaturalWaterTargets<'_, '_> {
    pub(super) fn collect_water(
        &self,
        world: Entity,
        position: Vec3,
        radius: f32,
        escaped: bool,
        output: &mut Vec<SelectedNaturalWaterTarget>,
    ) {
        self.collect_targets(world, position, radius, escaped, None, output);
    }

    pub(super) fn collect(
        &self,
        actor: Entity,
        world: Entity,
        position: Vec3,
        radius: f32,
        escaped: bool,
        output: &mut Vec<SelectedNaturalWaterTarget>,
    ) {
        self.collect_targets(world, position, radius, escaped, Some(actor), output);
    }

    fn collect_targets(
        &self,
        world: Entity,
        position: Vec3,
        radius: f32,
        escaped: bool,
        shore_actor: Option<Entity>,
        output: &mut Vec<SelectedNaturalWaterTarget>,
    ) {
        output.clear();
        let habitat = self.habitat_at(position);
        let index = &self.terrain.index;
        if (!escaped && habitat.is_none()) || index.chunk_span_m <= 0.0 || !radius.is_finite() {
            return;
        }
        let extent = radius + shore_actor.map_or(0.0, |_| self.search_margin.0);
        let center = position.xz();
        let bounds = [-extent, extent].map(|offset| {
            ((center + Vec2::splat(offset) - index.origin) / index.chunk_span_m)
                .floor()
                .as_ivec2()
        });
        let mut candidates = Vec::new();
        let mut append = |chunk: Entity| {
            let Ok(projected) = self.chunk_targets.get(chunk) else {
                return;
            };
            let source = if shore_actor.is_some() {
                &projected.shores
            } else {
                &projected.surfaces
            };
            for entry in source {
                let candidate = entry.target;
                if entry.world != world
                    || candidate.water.xz().distance_squared(center) >= radius * radius
                    || !self.reaches_from(candidate.water, habitat, escaped)
                    || shore_actor.is_some_and(|actor| {
                        !self.navigation_supports_dry_endpoint(actor, candidate.approach)
                            || !self.reaches_from(candidate.approach, habitat, escaped)
                    })
                {
                    continue;
                }
                candidates.push((entry.order, candidate));
            }
        };
        let [min, max] = bounds;
        let width = (i64::from(max.x) - i64::from(min.x) + 1).max(0);
        let height = (i64::from(max.y) - i64::from(min.y) + 1).max(0);
        if width.saturating_mul(height) > index.chunks.len() as i64 {
            for (&coordinate, &chunk) in &index.chunks {
                if coordinate.cmpge(min).all() && coordinate.cmple(max).all() {
                    append(chunk);
                }
            }
        } else {
            for x in min.x..=max.x {
                for z in min.y..=max.y {
                    if let Some(&chunk) = index.chunks.get(&IVec2::new(x, z)) {
                        append(chunk);
                    }
                }
            }
        }
        candidates.sort_unstable_by_key(|(order, _)| *order);
        output.extend(candidates.into_iter().map(|(_, candidate)| candidate));
    }

    fn habitat_at(&self, position: Vec3) -> Option<Entity> {
        locate_habitat_at_world_position(&self.habitats, position.xz(), *self.grid)
    }

    fn reaches_from(&self, point: Vec3, origin: Option<Entity>, escaped: bool) -> bool {
        let destination = self.habitat_at(point);
        if escaped {
            destination.is_none_or(|habitat| {
                self.habitat_summaries
                    .get(habitat)
                    .is_ok_and(|summary| summary.boundary_is_breached)
            })
        } else {
            origin.is_some() && origin == destination
        }
    }

    pub(super) fn valid(&self, target: &SelectedNaturalWaterTarget) -> bool {
        if !self.regions.contains(target.region) {
            return false;
        }
        let Some(sample) = self.terrain.sample(target.water.xz()) else {
            return false;
        };
        sample.water_height_m.is_some_and(|height| {
            (height - target.water.y).abs() < 0.01
                && match target.kind {
                    NaturalWaterTargetKind::Shore => true,
                    NaturalWaterTargetKind::Surface { depth } => {
                        (depth - (height - sample.height_m)).abs() < 0.01
                    }
                }
        })
    }

    pub(super) fn water_inputs_changed(&self, target: &SelectedNaturalWaterTarget) -> bool {
        self.regions
            .get(target.region)
            .map_or(true, |region| region.is_changed())
            || self.terrain.changed_at(target.water.xz())
            || terrain_chunk_at(&self.terrain.index, target.water.xz())
                .and_then(|chunk| self.chunk_targets.get(chunk).ok())
                .is_none_or(|targets| targets.is_changed())
    }

    pub(super) fn docking_inputs_changed(
        &self,
        actor: Entity,
        position: Vec3,
        target: &SelectedNaturalWaterTarget,
        escaped: bool,
    ) -> bool {
        self.terrain.changed_at(position.xz())
            || self.graph.as_ref().is_none_or(|graph| graph.is_changed())
            || self
                .agents
                .get(actor)
                .map_or(true, |agent| agent.is_changed())
            || self.habitats.is_changed()
            || self.grid.is_changed()
            || escaped
                && self.habitat_at(position).is_some_and(|habitat| {
                    self.habitat_summaries
                        .get(habitat)
                        .map_or(true, |summary| summary.is_changed())
                })
            || self.water_inputs_changed(target)
    }

    pub(super) fn docking_endpoint(
        &self,
        actor: Entity,
        point: Vec3,
        shore: &SelectedNaturalWaterTarget,
        escaped: bool,
    ) -> bool {
        self.terrain.sample(point.xz()).is_some_and(|sample| {
            sample.water_height_m.is_none()
                && self.agents.get(actor).is_ok_and(|agent| {
                    (sample.height_m - point.y).abs() <= agent.radius_m.max(0.05)
                })
                && self.navigation_supports_dry_endpoint(actor, point)
                && self.reaches_from(point, self.habitat_at(shore.approach), escaped)
        })
    }

    fn navigation_supports_dry_endpoint(&self, actor: Entity, point: Vec3) -> bool {
        let (Ok(agent), Some(graph)) = (self.agents.get(actor), self.graph.as_ref()) else {
            return false;
        };
        let graph = graph.terrain_derived_navigation_graph();
        let size = f32::from(graph.terrain_cell_size_centimetres) * 0.01;
        if !point.is_finite() || point.x < 0.0 || point.z > 0.0 || size <= 0.0 {
            return false;
        }
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
    }
}
