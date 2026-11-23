use std::collections::{BTreeSet, HashMap, HashSet};

pub(super) mod authored_membership;
pub(super) mod edit_retention;

use bevy::math::DVec3;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::{TerrainChunk, TerrainIndex};
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainWaterChanged;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::NaturalWaterRegion;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::NaturalWaterRegionCells;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionSource;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionsProjected;
use openzt2_zoo_state::terrain::terrain_water_view_state::{
    TerrainWaterRegionFade, TerrainWaterRegionVisibility,
};
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain_water;

/// Projects each authored DAT water-region row into one focused ECS entity.
pub(super) fn project_natural_water_regions(
    mut commands: Commands,
    terrain_assets: Res<Assets<TerrainAsset>>,
    chunks: Query<(Entity, &TerrainChunk), Without<TerrainWaterRegionsProjected>>,
) {
    for (entity, chunk) in &chunks {
        let Some(asset) = terrain_assets.get(&chunk.asset) else {
            continue;
        };
        let cells_per_chunk = u32::from(chunk.side.saturating_sub(1));
        if cells_per_chunk == 0 {
            continue;
        }
        for (source_region_index, region) in asset
            .canonical_terrain_grid()
            .water_regions
            .iter()
            .enumerate()
        {
            let Some(&first_cell) = region.row_major_cell_indices.first() else {
                continue;
            };
            let x = first_cell % asset.canonical_terrain_grid().width;
            let z = first_cell / asset.canonical_terrain_grid().width;
            let owner = UVec2::new(x / cells_per_chunk, z / cells_per_chunk).min(UVec2::new(
                asset
                    .canonical_terrain_grid()
                    .sector_columns
                    .saturating_sub(1),
                asset.canonical_terrain_grid().sector_rows.saturating_sub(1),
            ));
            if owner.as_ivec2() != chunk.source_coord {
                continue;
            }
            let mut emitter = Vec3::ZERO;
            for cell in &region.row_major_cell_indices {
                let x = *cell % asset.canonical_terrain_grid().width;
                let z = *cell / asset.canonical_terrain_grid().width;
                emitter += Vec3::new(
                    x as f32 * asset.canonical_terrain_grid().units.cell_size_metres,
                    asset.canonical_terrain_grid().units.base_height_metres
                        + region.height_relative_to_base_metres,
                    -(z as f32) * asset.canonical_terrain_grid().units.cell_size_metres,
                );
            }
            emitter /= region.row_major_cell_indices.len() as f32;
            let local = emitter - Vec3::new(chunk.origin.x, 0.0, chunk.origin.y);
            commands.spawn((
                TerrainWaterRegionSource::new(chunk.asset.clone(), source_region_index),
                TerrainWaterRegionFade::default(),
                TerrainWaterRegionVisibility::default(),
                NaturalWaterRegionCells {
                    cell_size_m: asset.canonical_terrain_grid().units.cell_size_metres,
                    centres: region
                        .row_major_cell_indices
                        .iter()
                        .map(|cell| {
                            Vec3::new(
                                (*cell % asset.canonical_terrain_grid().width) as f32,
                                0.0,
                                -((*cell / asset.canonical_terrain_grid().width) as f32),
                            ) * asset.canonical_terrain_grid().units.cell_size_metres
                                + Vec3::Y
                                    * (asset.canonical_terrain_grid().units.base_height_metres
                                        + region.height_relative_to_base_metres)
                        })
                        .collect(),
                },
                NaturalWaterRegion {
                    style: AssetId::from_key(&format!(
                        "terrain/water/{}",
                        region.water_style_index
                    )),
                    area_m2: region.row_major_cell_indices.len() as f32
                        * asset
                            .canonical_terrain_grid()
                            .units
                            .cell_size_metres
                            .powi(2),
                },
                Transform::from_translation(local),
                ChildOf(entity),
            ));
        }
        commands.entity(entity).insert(TerrainWaterRegionsProjected);
    }
}

#[derive(Clone, Copy)]
struct LiveWaterCell {
    key: IVec2,
    style: u16,
    position: Vec3,
    area_m2: f32,
    owner: Entity,
    owner_coord: IVec2,
    owner_origin: Vec2,
}

#[derive(Component)]
pub(super) struct TerrainWaterTargetsPending(IRect);

pub(super) fn mark_edited_water_targets_pending(
    mut commands: Commands,
    mut changes: MessageReader<TerrainWaterChanged>,
    mut chunks: Query<Option<&mut TerrainWaterTargetsPending>, With<TerrainChunk>>,
) {
    let mut dirty = bevy::ecs::entity::EntityHashMap::<IRect>::default();
    for change in changes.read() {
        let bounds = IRect::from_corners(change.min.as_ivec2(), change.max.as_ivec2());
        dirty
            .entry(change.chunk)
            .and_modify(|previous| *previous = previous.union(bounds))
            .or_insert(bounds);
    }
    for (entity, bounds) in dirty {
        let Ok(pending) = chunks.get_mut(entity) else {
            continue;
        };
        if let Some(mut pending) = pending {
            pending.0 = pending.0.union(bounds);
        } else {
            commands
                .entity(entity)
                .insert(TerrainWaterTargetsPending(bounds));
        }
    }
}

pub(super) fn water_target_refresh_is_ready(
    pending: Query<(), With<TerrainWaterTargetsPending>>,
    uninitialized: Query<
        &TerrainChunk,
        (
            With<TerrainWaterRegionsProjected>,
            Without<TerrainWaterTargetConnectivity>,
        ),
    >,
    brushes: Query<
        (),
        With<openzt2_zoo_state::terrain::terrain_brush_interaction_state::ActiveTerrainBrush>,
    >,
) -> bool {
    brushes.is_empty()
        && (!pending.is_empty()
            || uninitialized
                .iter()
                .any(|chunk| chunk.source_coord == IVec2::ZERO))
}

/// Cell-to-target lookup for the connected components on one terrain.
#[derive(Component, Default)]
pub(super) struct TerrainWaterTargetConnectivity {
    cells: HashMap<IVec2, Entity>,
    active: bool,
}

#[derive(Component)]
pub(super) struct PreparedWaterTarget(Option<(NaturalWaterRegionCells, NaturalWaterRegion)>);

#[derive(Component)]
#[relationship(relationship_target = TerrainWaterTargets)]
pub(super) struct TerrainWaterTargetOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = TerrainWaterTargetOf, linked_spawn)]
pub(super) struct TerrainWaterTargets(Vec<Entity>);

#[derive(Component)]
pub(super) struct LiveWaterTargetCells(Vec<IVec2>);

const WATER_NEIGHBORS: [IVec2; 4] = [IVec2::NEG_X, IVec2::X, IVec2::NEG_Y, IVec2::Y];

/// Refreshes connected animal targets and sound emitters after a held stroke
/// ends. Their connectivity is independent of authored render membership.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn rebuild_edited_water_regions(
    mut commands: Commands,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_index: Single<&TerrainIndex>,
    chunks: Query<(Entity, &TerrainChunk, Option<&EditedTerrainSamples>)>,
    authored: Query<(Entity, &TerrainWaterRegionSource), With<NaturalWaterRegion>>,
    pending: Query<(Entity, &TerrainChunk, &TerrainWaterTargetsPending)>,
    mut connectivity: Query<&mut TerrainWaterTargetConnectivity>,
    authored_chunks: Query<(), With<TerrainWaterRegionsProjected>>,
    projections: Query<&TerrainWaterTargets>,
    mut targets: Query<(
        &mut LiveWaterTargetCells,
        Option<&mut NaturalWaterRegionCells>,
        Option<&mut NaturalWaterRegion>,
        Option<&mut PreparedWaterTarget>,
        &mut Transform,
        &ChildOf,
    )>,
) {
    let terrain_index = terrain_index.into_inner();
    let mut dirty_by_terrain = HashMap::<bevy::asset::AssetId<TerrainAsset>, Vec<IRect>>::new();
    for (entity, chunk, pending) in &pending {
        let Some(count) = terrain_assets
            .get(&chunk.asset)
            .and_then(TerrainAsset::terrain_chunk_cells_per_side)
            .map(i32::from)
        else {
            continue;
        };
        let origin = chunk.coord * count;
        dirty_by_terrain
            .entry(chunk.asset.id())
            .or_default()
            .push(IRect::from_corners(
                origin + (pending.0.min - IVec2::ONE).max(IVec2::ZERO),
                origin + pending.0.max.min(IVec2::splat(count - 1)),
            ));
        commands
            .entity(entity)
            .remove::<TerrainWaterTargetsPending>();
    }

    for (entity, chunk, _) in &chunks {
        if chunk.source_coord != IVec2::ZERO || connectivity.contains(entity) {
            continue;
        }
        let Some(asset) = terrain_assets.get(&chunk.asset) else {
            continue;
        };
        let grid = asset.canonical_terrain_grid();
        let initialized_chunks = chunks
            .iter()
            .filter(|(entity, candidate, _)| {
                candidate.asset == chunk.asset && authored_chunks.contains(*entity)
            })
            .count();
        if initialized_chunks == (grid.sector_columns * grid.sector_rows) as usize {
            dirty_by_terrain.entry(chunk.asset.id()).or_default();
        }
    }

    for (terrain, dirty) in dirty_by_terrain {
        let Some(asset) = terrain_assets.get(terrain) else {
            continue;
        };
        let Some(count) = asset
            .terrain_chunk_cells_per_side()
            .map(i32::from)
            .filter(|count| *count > 0)
        else {
            continue;
        };
        let Some((root, _, _)) = chunks
            .iter()
            .find(|(_, chunk, _)| chunk.asset.id() == terrain && chunk.source_coord == IVec2::ZERO)
        else {
            continue;
        };
        let current = connectivity.get_mut(root).ok();
        let initialize = current.is_none();
        let activate = !dirty.is_empty();
        let mut initial = TerrainWaterTargetConnectivity::default();
        let mut current = current;
        let index = current.as_deref_mut().unwrap_or(&mut initial);
        let mut seeds = BTreeSet::new();
        let first_activation = activate && !index.active;
        if first_activation {
            for (entity, source) in &authored {
                if source.terrain().id() == terrain {
                    commands
                        .entity(entity)
                        .remove::<(NaturalWaterRegion, NaturalWaterRegionCells)>();
                }
            }
        }
        if initialize {
            for (_, chunk, _) in chunks
                .iter()
                .filter(|(_, chunk, _)| chunk.asset.id() == terrain)
            {
                let origin = chunk.coord * count;
                for z in 0..count {
                    for x in 0..count {
                        seeds.insert((origin.y + z, origin.x + x));
                    }
                }
            }
        } else {
            for bounds in dirty {
                for z in bounds.min.y..=bounds.max.y {
                    for x in bounds.min.x..=bounds.max.x {
                        seeds.insert((z, x));
                    }
                }
            }
        }

        let mut sampled = HashMap::<IVec2, Option<LiveWaterCell>>::new();
        let mut sample = |key: IVec2| {
            *sampled.entry(key).or_insert_with(|| {
                let coord = IVec2::new(key.x.div_euclid(count), key.y.div_euclid(count));
                let (owner, chunk, edited) = chunks.get(*terrain_index.chunks.get(&coord)?).ok()?;
                if chunk.asset.id() != terrain {
                    return None;
                }
                let local = key - coord * count;
                let center = chunk.origin + (local.as_vec2() + Vec2::splat(0.5)) * chunk.spacing_m;
                let (geometry, Some((_, style))) =
                    sample_terrain_water(chunk, asset, edited, center)?
                else {
                    return None;
                };
                Some(LiveWaterCell {
                    key,
                    style,
                    position: Vec3::new(center.x, geometry.water_height_m?, center.y),
                    area_m2: chunk.spacing_m * chunk.spacing_m,
                    owner,
                    owner_coord: chunk.coord,
                    owner_origin: chunk.origin,
                })
            })
        };
        let mut visited = HashSet::new();
        let mut affected = bevy::ecs::entity::EntityHashSet::default();
        let mut components = Vec::new();
        while let Some((z, x)) = seeds.pop_first() {
            let key = IVec2::new(x, z);
            if !visited.insert(key) {
                continue;
            }
            let mut include_previous_target = |key| {
                if let Some(&target) = index.cells.get(&key) {
                    if affected.insert(target) {
                        if let Ok((cells, ..)) = targets.get(target) {
                            seeds.extend(cells.0.iter().map(|key| (key.y, key.x)));
                        }
                    }
                }
            };
            include_previous_target(key);
            let Some(first) = sample(key) else { continue };
            let mut pending = vec![first];
            let mut component = HashMap::new();
            while let Some(cell) = pending.pop() {
                component.insert(cell.key, cell);
                include_previous_target(cell.key);
                for offset in WATER_NEIGHBORS {
                    let next = cell.key + offset;
                    if !visited.contains(&next) {
                        if let Some(next) = sample(next).filter(|next| next.style == first.style) {
                            visited.insert(next.key);
                            pending.push(next);
                        }
                    }
                }
            }
            let first_key = *component
                .keys()
                .min_by_key(|key| (key.y, key.x))
                .expect("nonempty water component");
            components.push((first_key, component));
        }

        components.sort_unstable_by_key(|(key, _)| (key.y, key.x));
        let mut reused = bevy::ecs::entity::EntityHashSet::default();
        let mut assignments = Vec::new();
        for (first, component) in components {
            let mut owner = component[&first];
            let style = AssetId::from_key(&format!("terrain/water/{}", owner.style));
            let mut overlap = bevy::ecs::entity::EntityHashMap::<usize>::default();
            let mut keys = Vec::with_capacity(component.len());
            let mut centres = Vec::with_capacity(component.len());
            let mut area_m2 = 0.0_f64;
            let mut weighted_position = DVec3::ZERO;
            let mut pending = vec![first];
            let mut seen = HashSet::from([first]);
            while let Some(key) = pending.pop() {
                let cell = component[&key];
                keys.push(key);
                centres.push(cell.position);
                if let Some(&previous) = index.cells.get(&key) {
                    *overlap.entry(previous).or_default() += 1;
                }
                let area = f64::from(cell.area_m2);
                area_m2 += area;
                weighted_position += cell.position.as_dvec3() * area;
                if (cell.owner_coord.y, cell.owner_coord.x)
                    < (owner.owner_coord.y, owner.owner_coord.x)
                {
                    owner = cell;
                }
                for offset in WATER_NEIGHBORS {
                    let next = key + offset;
                    if component.contains_key(&next) && seen.insert(next) {
                        pending.push(next);
                    }
                }
            }
            let centroid = (weighted_position / area_m2).as_vec3();
            let local = centroid - Vec3::new(owner.owner_origin.x, 0.0, owner.owner_origin.y);
            let facts = NaturalWaterRegion {
                style,
                area_m2: area_m2 as f32,
            };
            let centres = NaturalWaterRegionCells {
                cell_size_m: owner.area_m2.sqrt(),
                centres: centres.into_boxed_slice(),
            };
            let reuse = overlap
                .into_iter()
                .filter(|(entity, _)| {
                    !reused.contains(entity)
                        && targets
                            .get(*entity)
                            .is_ok_and(|(_, _, facts, pending, ..)| {
                                facts
                                    .or_else(|| {
                                        pending.and_then(|pending| {
                                            pending.0.as_ref().map(|(_, facts)| facts)
                                        })
                                    })
                                    .is_some_and(|facts| facts.style == style)
                            })
                })
                .max_by_key(|(entity, count)| (*count, std::cmp::Reverse(*entity)))
                .map(|(entity, _)| entity);
            let entity = if let Some(entity) = reuse {
                reused.insert(entity);
                let Ok((mut cells, positions, region, prepared, mut transform, parent)) =
                    targets.get_mut(entity)
                else {
                    continue;
                };
                for key in &cells.0 {
                    index.cells.remove(key);
                }
                cells.0.clone_from(&keys);
                if let (Some(mut positions), Some(mut region)) = (positions, region) {
                    *positions = centres;
                    if region.area_m2 != facts.area_m2 {
                        region.area_m2 = facts.area_m2;
                    }
                } else if activate {
                    commands
                        .entity(entity)
                        .insert((centres, facts))
                        .remove::<PreparedWaterTarget>();
                } else if let Some(mut prepared) = prepared {
                    prepared.0 = Some((centres, facts));
                }
                if transform.translation != local {
                    transform.translation = local;
                }
                if parent.parent() != owner.owner {
                    commands.entity(entity).insert(ChildOf(owner.owner));
                }
                entity
            } else {
                let mut target = commands.spawn((
                    LiveWaterTargetCells(keys.clone()),
                    Transform::from_translation(local),
                    ChildOf(owner.owner),
                    TerrainWaterTargetOf(root),
                ));
                if activate {
                    target.insert((centres, facts));
                } else {
                    target.insert(PreparedWaterTarget(Some((centres, facts))));
                }
                target.id()
            };
            assignments.extend(keys.into_iter().map(|key| (key, entity)));
        }

        if first_activation {
            if let Ok(projected) = projections.get(root) {
                for entity in projected
                    .iter()
                    .filter(|&entity| !affected.contains(&entity))
                {
                    let Ok((_, _, _, Some(mut prepared), ..)) = targets.get_mut(entity) else {
                        continue;
                    };
                    if let Some((cells, region)) = prepared.0.take() {
                        commands
                            .entity(entity)
                            .insert((cells, region))
                            .remove::<PreparedWaterTarget>();
                    }
                }
            }
            index.active = true;
        }
        for entity in affected {
            if reused.contains(&entity) {
                continue;
            }
            if let Ok((cells, ..)) = targets.get(entity) {
                for key in &cells.0 {
                    index.cells.remove(key);
                }
            }
            commands.entity(entity).try_despawn();
        }
        index.cells.extend(assignments);
        if initialize {
            commands.entity(root).insert(initial);
        }
    }
}
