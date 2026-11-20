//! Keeps the terrain inside each tank's walls at the tank floor, so the floor
//! is the ground seen through the glass and under the water.
use openzt2_zoo_state::topology::{
    habitat_boundary_queries::HabitatBoundaryQueries, topology_graph_types::FenceSegment,
};

use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::aquatic::tank_wall_ground_sampling::TankWallGroundSampling;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::terrain::terrain_chunk_identity_type::TerrainChunkId;
use openzt2_zoo_state::terrain::terrain_chunk_types::{
    EditedTerrainSamples, TerrainChunk, TerrainIndex,
};
use openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainEditDelta;
use openzt2_zoo_state::terrain::terrain_edit_types::{
    CommitTerrainEdit, TerrainChanged, TerrainEdit, TerrainEditAcknowledged,
};
use openzt2_zoo_state::terrain::terrain_sample_grid_queries::read_materialized_terrain_sample;
use openzt2_zoo_state::topology::topology_graph_types::{TopologyGrid, TopologyNode};

use super::aquatic_simulation_types::{Tank, TankGeometry};
use super::tank_presentation::inside_walls;

/// A terrain edit that moves tank insides to their floors. It stays out of
/// construction undo: the floor follows the tank geometry, whose edits undo.
#[derive(Component)]
pub(super) struct TankFloorExcavation;

/// The excavation's footprint and fill height survive demolition of its walls.
#[derive(Component, Default, PartialEq)]
pub(super) struct TankFloorTerrainFootprint {
    walls: Vec<[Vec2; 2]>,
    bounds: Rect,
    floor_height_cm: i16,
    fill_height_cm: i16,
}

#[derive(Message)]
pub(super) struct RestoreTankFloorTerrain(TankFloorTerrainFootprint);

pub(super) fn queue_terrain_restoration_after_tank_removal(
    removal: On<Remove<Tank>>,
    mut commands: Commands,
    mut footprints: Query<&mut TankFloorTerrainFootprint>,
    mut restorations: MessageWriter<RestoreTankFloorTerrain>,
) {
    if let Ok(mut footprint) = footprints.get_mut(removal.entity) {
        restorations.write(RestoreTankFloorTerrain(std::mem::take(&mut *footprint)));
        commands
            .entity(removal.entity)
            .try_remove::<TankFloorTerrainFootprint>();
    }
}

/// Whether `point` lies inside the walls and more than `margin` from each.
/// Samples within one terrain spacing of a wall keep the outside ground:
/// walls and samples lie on different grids, so the terrain triangles crossing
/// a wall would otherwise poke up through the floor beside the glass.
pub(super) fn inside_walls_beyond(point: Vec2, walls: &[[Vec2; 2]], margin: f32) -> bool {
    inside_walls(point, walls)
        && walls.iter().all(|[a, b]| {
            let along = (point - *a).dot(*b - *a) / (*b - *a).length_squared().max(f32::EPSILON);
            point.distance(*a + (*b - *a) * along.clamp(0.0, 1.0)) > margin
        })
}

/// Moves the terrain inside tanks whose geometry changed, including new and
/// restored tanks, to their floor height. Saved terrain already matches, so
/// restoring a tank changes nothing. A removed tank is filled back to the mean
/// ground under its walls.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn excavate_changed_tank_floors(
    mut commands: Commands,
    grid: Res<TopologyGrid>,
    terrain_index: Res<TerrainIndex>,
    tanks: Query<(Entity, &TankGeometry, Ref<HabitatRegion>), With<Tank>>,
    mut restorations: MessageReader<RestoreTankFloorTerrain>,
    mut changes: MessageReader<TerrainChanged>,
    (ground, boundaries): (TankWallGroundSampling, HabitatBoundaryQueries),
    fences: Query<FenceSegment>,
    nodes: Query<&TopologyNode>,
    chunks: Query<(
        &TerrainChunk,
        Option<&EditedTerrainSamples>,
        &TerrainChunkId,
    )>,
    assets: Res<Assets<TerrainAsset>>,
    mut footprints: Query<&mut TankFloorTerrainFootprint>,
) {
    let restorations = restorations.read().collect::<Vec<_>>();
    let height_edits: Vec<_> = changes
        .read()
        .filter(|event| event.ground_height_changed)
        .filter_map(|event| {
            let (chunk, ..) = chunks.get(event.chunk).ok()?;
            let min = chunk.origin + event.min.as_vec2() * chunk.spacing_m;
            let max = chunk.origin + event.max.as_vec2() * chunk.spacing_m;
            Some(Rect::from_corners(min, max).inflate(chunk.spacing_m))
        })
        .collect();
    let excavations = tanks
        .iter()
        .filter_map(|(tank, geometry, region)| {
            let floor_height_cm = (geometry.floor_height * 100.0).round() as i16;
            if !region.is_changed()
                && !grid.is_changed()
                && footprints.get(tank).is_ok_and(|footprint| {
                    footprint.floor_height_cm == floor_height_cm
                        && !height_edits
                            .iter()
                            .any(|edit| !edit.intersect(footprint.bounds).is_empty())
                })
            {
                return None;
            }
            let mut wall_ground_sum = 0.0;
            let walls = boundaries
                .fences(tank)
                .map(|fence| {
                    let edge = fences.get(fence).ok()?;
                    let a = nodes.get(edge.a()).ok()?.cell;
                    let b = nodes.get(edge.b()).ok()?.cell;
                    wall_ground_sum += ground.ground_height_at_topology_cell(a)?;
                    Some([a, b].map(|cell| grid.cell_translation(cell).xz()))
                })
                .collect::<Option<Vec<_>>>()?;
            let mean_ground = wall_ground_sum / walls.len().max(1) as f32;
            let bounds = walls
                .iter()
                .flatten()
                .copied()
                .map(|point| Rect::from_corners(point, point))
                .reduce(|bounds, point| bounds.union(point))?;
            let footprint = TankFloorTerrainFootprint {
                walls,
                bounds,
                floor_height_cm,
                fill_height_cm: (mean_ground * 100.0).round() as i16,
            };
            Some((tank, footprint))
        })
        .collect::<Vec<_>>();
    if restorations.is_empty() && excavations.is_empty() {
        return;
    }
    let floors = restorations
        .iter()
        .map(|restoration| (&restoration.0, restoration.0.fill_height_cm))
        .chain(
            excavations
                .iter()
                .map(|(_, footprint)| (footprint, footprint.floor_height_cm)),
        );
    let mut affected_chunks = EntityHashSet::default();
    if terrain_index.chunk_span_m.is_finite() && terrain_index.chunk_span_m > 0.0 {
        for (footprint, _) in floors.clone() {
            let minimum = ((footprint.bounds.min - terrain_index.origin)
                / terrain_index.chunk_span_m)
                .floor()
                .as_ivec2()
                - IVec2::ONE;
            let maximum = ((footprint.bounds.max - terrain_index.origin)
                / terrain_index.chunk_span_m)
                .floor()
                .as_ivec2();
            for z in minimum.y..=maximum.y {
                for x in minimum.x..=maximum.x {
                    if let Some(entity) = terrain_index.chunks.get(&IVec2::new(x, z)) {
                        affected_chunks.insert(*entity);
                    }
                }
            }
        }
    }
    let mut affected_chunks: Vec<_> = chunks
        .iter_many(affected_chunks)
        .filter_map(Result::ok)
        .collect();
    affected_chunks.sort_unstable_by_key(|(_, _, id)| id.0);
    let mut deltas = Vec::new();
    for (chunk, edited, id) in affected_chunks {
        let Some(asset) = assets.get(&chunk.asset) else {
            continue;
        };
        if chunk.spacing_m <= 0.0 {
            continue;
        }
        // One delta per chunk for every changed tank, so tanks sharing a
        // chunk cannot overwrite each other's floors.
        let mut targets = Vec::new();
        for (footprint, height_cm) in floors.clone() {
            let Rect {
                min: minimum,
                max: maximum,
            } = footprint.bounds;
            let last = f32::from(chunk.side.saturating_sub(1));
            let chunk_max = chunk.origin + Vec2::splat(last * chunk.spacing_m);
            if maximum.cmplt(chunk.origin).any() || minimum.cmpgt(chunk_max).any() {
                continue;
            }
            let first = ((minimum - chunk.origin) / chunk.spacing_m)
                .floor()
                .clamp(Vec2::ZERO, Vec2::splat(last));
            let end = ((maximum - chunk.origin) / chunk.spacing_m)
                .ceil()
                .clamp(Vec2::ZERO, Vec2::splat(last));
            for z in first.y as u32..=end.y as u32 {
                for x in first.x as u32..=end.x as u32 {
                    let point = chunk.origin + UVec2::new(x, z).as_vec2() * chunk.spacing_m;
                    if inside_walls_beyond(point, &footprint.walls, chunk.spacing_m) {
                        targets.push((UVec2::new(x, z), height_cm));
                    }
                }
            }
        }
        let (Some(minimum), Some(maximum)) = (
            targets.iter().map(|(sample, _)| *sample).reduce(UVec2::min),
            targets.iter().map(|(sample, _)| *sample).reduce(UVec2::max),
        ) else {
            continue;
        };
        targets.sort_unstable_by_key(|(sample, _)| (sample.y, sample.x));
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut changed = false;
        for z in minimum.y..=maximum.y {
            for x in minimum.x..=maximum.x {
                let Some(sample) =
                    read_materialized_terrain_sample(chunk, asset, edited, x as usize, z as usize)
                else {
                    continue;
                };
                before.push(sample);
                let mut floor = sample;
                if let Ok(index) =
                    targets.binary_search_by_key(&(z, x), |(sample, _)| (sample.y, sample.x))
                {
                    floor.height_cm = targets[index].1;
                    changed |= floor != sample;
                }
                after.push(floor);
            }
        }
        if changed && before.len() == ((maximum - minimum + UVec2::ONE).element_product()) as usize
        {
            deltas.push(TerrainEditDelta {
                chunk: *id,
                min: minimum,
                max: maximum,
                before: before.into_boxed_slice(),
                after: after.into_boxed_slice(),
            });
        }
    }
    for (tank, footprint) in excavations {
        if let Ok(mut current) = footprints.get_mut(tank) {
            current.set_if_neq(footprint);
        } else {
            commands.entity(tank).try_insert(footprint);
        }
    }
    if !deltas.is_empty() {
        commands.spawn((
            TankFloorExcavation,
            TerrainEdit {
                deltas,
                water_regions: Vec::new(),
            },
        ));
    }
}

/// Commits new floor excavations and drops them once the terrain answers.
pub(super) fn commit_tank_floor_excavations(
    mut commands: Commands,
    added: Query<Entity, Added<TankFloorExcavation>>,
    excavations: Query<(), With<TankFloorExcavation>>,
    mut acknowledged: MessageReader<TerrainEditAcknowledged>,
    mut commits: MessageWriter<CommitTerrainEdit>,
) {
    for acknowledgement in acknowledged.read() {
        if excavations.contains(acknowledgement.transaction) {
            commands.entity(acknowledgement.transaction).despawn();
        }
    }
    for excavation in &added {
        commits.write(CommitTerrainEdit {
            transaction: excavation,
            application: EditApplication::InitialCommit,
        });
    }
}
