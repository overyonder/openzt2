use std::collections::HashMap;

pub(super) mod authored_membership;

use bevy::math::DVec3;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainWaterChanged;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::NaturalWaterRegion;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::NaturalWaterRegionCells;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterMembershipIndex;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionSource;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionsProjected;
use openzt2_zoo_state::terrain::terrain_water_view_state::{
    TerrainWaterRegionFade, TerrainWaterRegionVisibility,
};
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_authored_terrain;

/// Retired edited owners survive only while an undo transaction references them.
pub(super) fn cleanup_unreferenced_retired_water_region_entities(
    mut commands: Commands,
    retired: Query<Entity, (With<openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionRetired>, Without<TerrainWaterRegionSource>)>,
    edits: Query<(
        &openzt2_zoo_state::terrain::terrain_edit_types::TerrainEdit,
        Option<
            &openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress,
        >,
    )>,
) {
    use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
    for owner in &retired {
        let retained = edits.iter().any(|(edit, progress)| {
            progress.is_none_or(|progress| progress.phase != EditCommitPhase::Failed)
                && edit
                    .water_regions
                    .iter()
                    .any(|change| change.owner == owner)
        });
        if !retained {
            commands.entity(owner).despawn();
        }
    }
}

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

/// Rebuilds spatial-audio emitters after a committed water edit. These connected
/// audio regions do not supply water-effect affiliation. Sample/style state
/// remains in the terrain overlays.
pub(super) fn rebuild_edited_water_regions(
    mut commands: Commands,
    mut changes: MessageReader<TerrainWaterChanged>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    chunks: Query<(Entity, &TerrainChunk, Option<&EditedTerrainSamples>)>,
    regions: Query<(Entity, Option<&TerrainWaterRegionSource>), With<NaturalWaterRegion>>,
    membership_indexes: Query<Entity, With<TerrainWaterMembershipIndex>>,
) {
    if changes.read().count() == 0 {
        return;
    }
    for (entity, source) in &regions {
        if source.is_some() {
            commands
                .entity(entity)
                .remove::<(NaturalWaterRegion, NaturalWaterRegionCells)>();
        } else {
            commands.entity(entity).despawn();
        }
    }
    for entity in &membership_indexes {
        commands
            .entity(entity)
            .remove::<TerrainWaterMembershipIndex>();
    }

    let mut assets = Vec::<Handle<TerrainAsset>>::new();
    for (_, chunk, _) in &chunks {
        if !assets.iter().any(|asset| asset.id() == chunk.asset.id()) {
            assets.push(chunk.asset.clone());
        }
    }
    for handle in assets {
        let Some(asset) = terrain_assets.get(&handle) else {
            continue;
        };
        let Some(cells_per_chunk) = asset.terrain_chunk_cells_per_side().map(i32::from) else {
            continue;
        };
        let mut cells = Vec::<LiveWaterCell>::new();
        for (entity, chunk, edited) in &chunks {
            if chunk.asset.id() != handle.id() {
                continue;
            }
            for z in 0..chunk.side.saturating_sub(1) {
                for x in 0..chunk.side.saturating_sub(1) {
                    let center = chunk.origin
                        + (Vec2::new(f32::from(x), f32::from(z)) + Vec2::splat(0.5))
                            * chunk.spacing_m;
                    let Some(sample) = sample_authored_terrain(chunk, asset, edited, center) else {
                        continue;
                    };
                    let (Some(water), Some(height)) =
                        (sample.water, sample.geometry.water_height_m)
                    else {
                        continue;
                    };
                    cells.push(LiveWaterCell {
                        key: chunk.coord * cells_per_chunk + IVec2::new(i32::from(x), i32::from(z)),
                        style: water.style,
                        position: Vec3::new(center.x, height, center.y),
                        area_m2: chunk.spacing_m * chunk.spacing_m,
                        owner: entity,
                        owner_coord: chunk.coord,
                        owner_origin: chunk.origin,
                    });
                }
            }
        }
        cells.sort_unstable_by_key(|cell| (cell.key.y, cell.key.x));
        let lookup = cells
            .iter()
            .enumerate()
            .map(|(index, cell)| (cell.key, index))
            .collect::<HashMap<_, _>>();
        let mut visited = vec![false; cells.len()];
        let mut pending = Vec::new();
        for seed in 0..cells.len() {
            if visited[seed] {
                continue;
            }
            let style = cells[seed].style;
            visited[seed] = true;
            pending.push(seed);
            let mut area_m2 = 0.0_f64;
            let mut weighted_position = DVec3::ZERO;
            let mut owner = cells[seed];
            let mut region_centres = Vec::new();
            while let Some(index) = pending.pop() {
                let cell = cells[index];
                region_centres.push(cell.position);
                let area = f64::from(cell.area_m2);
                area_m2 += area;
                weighted_position += cell.position.as_dvec3() * area;
                if (cell.owner_coord.y, cell.owner_coord.x)
                    < (owner.owner_coord.y, owner.owner_coord.x)
                {
                    owner = cell;
                }
                for offset in [IVec2::NEG_X, IVec2::X, IVec2::NEG_Y, IVec2::Y] {
                    let Some(&neighbour) = lookup.get(&(cell.key + offset)) else {
                        continue;
                    };
                    if !visited[neighbour] && cells[neighbour].style == style {
                        visited[neighbour] = true;
                        pending.push(neighbour);
                    }
                }
            }
            let centroid = (weighted_position / area_m2).as_vec3();
            let local = centroid - Vec3::new(owner.owner_origin.x, 0.0, owner.owner_origin.y);
            commands.spawn((
                NaturalWaterRegionCells {
                    cell_size_m: owner.area_m2.sqrt(),
                    centres: region_centres.into_boxed_slice(),
                },
                NaturalWaterRegion {
                    style: AssetId::from_key(&format!("terrain/water/{style}")),
                    area_m2: area_m2 as f32,
                },
                Transform::from_translation(local),
                ChildOf(owner.owner),
            ));
        }
    }
}
