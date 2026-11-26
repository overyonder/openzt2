//! Keeps the terrain inside each tank's walls at the tank floor, so the floor
//! is the ground seen through the glass and under the water.
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::terrain::terrain_chunk_identity_type::TerrainChunkId;
use openzt2_zoo_state::terrain::terrain_chunk_types::{
    EditedTerrainSamples, TerrainChunk, TerrainIndex,
};
use openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainEditDelta;
use openzt2_zoo_state::terrain::terrain_edit_types::{
    CommitTerrainEdit, TerrainEdit, TerrainEditAcknowledged,
};
use openzt2_zoo_state::terrain::terrain_sample_grid_queries::read_materialized_terrain_sample;

use super::aquatic_simulation_types::Tank;
use super::tank_geometry_projection::{
    TankExcavationBoundary, TankFloorLevelCm, TankFloorTerrainDisturbed,
};
use super::tank_presentation::inside_walls;

/// A terrain edit that moves tank insides to their floors. It stays out of
/// construction undo: the floor follows the tank geometry, whose edits undo.
#[derive(Component)]
pub(super) struct TankFloorExcavation;

#[derive(Message)]
pub(super) struct RestoreTankFloorTerrain(TankExcavationBoundary);

pub(super) fn queue_terrain_restoration_after_tank_removal(
    removal: On<Remove<Tank>>,
    mut commands: Commands,
    mut boundaries: Query<&mut TankExcavationBoundary>,
    mut restorations: MessageWriter<RestoreTankFloorTerrain>,
) {
    if let Ok(mut boundary) = boundaries.get_mut(removal.entity) {
        restorations.write(RestoreTankFloorTerrain(std::mem::take(&mut *boundary)));
    }
    commands
        .entity(removal.entity)
        .try_remove::<(TankExcavationBoundary, TankFloorLevelCm)>();
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
    terrain: Single<(Entity, &TerrainIndex)>,
    tanks: Query<(&WorldMember, &TankExcavationBoundary, &TankFloorLevelCm), With<Tank>>,
    changed: Query<
        Entity,
        (
            With<Tank>,
            Or<(Changed<TankExcavationBoundary>, Changed<TankFloorLevelCm>)>,
        ),
    >,
    mut restorations: MessageReader<RestoreTankFloorTerrain>,
    mut disturbed: MessageReader<TankFloorTerrainDisturbed>,
    chunks: Query<(
        &TerrainChunk,
        Option<&EditedTerrainSamples>,
        &TerrainChunkId,
    )>,
    assets: Res<Assets<TerrainAsset>>,
) {
    let (root, terrain_index) = terrain.into_inner();
    let restorations = restorations.read().collect::<Vec<_>>();
    let affected: EntityHashSet = changed
        .iter()
        .chain(disturbed.read().map(|event| event.0))
        .collect();
    let excavations: Vec<_> = tanks
        .iter_many(affected)
        .flatten()
        .filter_map(|(member, boundary, floor)| {
            (member.root == root).then_some((boundary, floor.0))
        })
        .collect();
    if restorations.is_empty() && excavations.is_empty() {
        return;
    }
    let floors = restorations
        .iter()
        .map(|restoration| (&restoration.0, restoration.0.fill_height_cm))
        .chain(excavations.iter().copied());
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
    if !deltas.is_empty() {
        let transaction = commands
            .spawn((
                WorldMember { root },
                TankFloorExcavation,
                TerrainEdit {
                    deltas,
                    water_regions: Vec::new(),
                },
            ))
            .id();
        commands.write_message(CommitTerrainEdit {
            transaction,
            application: EditApplication::InitialCommit,
        });
    }
}

pub(super) fn retire_applied_tank_floor_excavations(
    mut commands: Commands,
    excavations: Query<(), With<TankFloorExcavation>>,
    mut acknowledged: MessageReader<TerrainEditAcknowledged>,
) {
    for acknowledgement in acknowledged.read() {
        if excavations.contains(acknowledgement.transaction) {
            commands.entity(acknowledgement.transaction).despawn();
        }
    }
}
