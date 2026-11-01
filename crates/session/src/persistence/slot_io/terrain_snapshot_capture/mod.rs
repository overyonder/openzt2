use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure;
use super::terrain_snapshot_section_encoding::validate_terrain_snapshot_record;
use super::terrain_snapshot_types::{TerrainEditedChunkSnapshotRecord, TerrainSnapshotRecord};
use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::terrain::TerrainWaterRegion;
use openzt2_zoo_state::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk};
use openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainWaterAffiliation;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterRegionQuery, TerrainWaterRegionSource,
};

pub(super) fn capture_terrain_snapshot_record<'a>(
    handle: &Handle<TerrainAsset>,
    asset: &TerrainAsset,
    chunks: impl Iterator<Item = (&'a TerrainChunk, Option<&'a EditedTerrainSamples>)>,
    regions: &TerrainWaterRegionQuery,
    owner_is_in_world: impl Fn(Entity) -> bool,
    active_source_owners: impl Iterator<Item = (Entity, &'a TerrainWaterRegionSource)>,
    retired_sources: impl Iterator<Item = &'a TerrainWaterRegionSource>,
) -> Result<TerrainSnapshotRecord, Failure> {
    let mut snapshot = TerrainSnapshotRecord {
        grid: asset.canonical_terrain_grid().clone(),
        original_water_region_count: u32::try_from(
            asset.canonical_terrain_grid().water_regions.len(),
        )
        .map_err(|_| Failure::CapacityExceeded)?,
        edited_water_surface_heights: Vec::new(),
        edited_chunks: Vec::new(),
        retired_water_region_rows: Vec::new(),
    };
    validate_terrain_snapshot_record(&snapshot)?;
    let mut owners = Vec::new();
    for (owner, source) in active_source_owners {
        if !owner_is_in_world(owner)
            || source.terrain().id() != handle.id()
            || regions.get(owner).is_none()
        {
            continue;
        }
        let row =
            u32::try_from(source.source_region_index()).map_err(|_| Failure::CapacityExceeded)?;
        if row >= snapshot.original_water_region_count
            || owners
                .iter()
                .any(|(_, candidate_row)| *candidate_row == row)
        {
            return Err(Failure::BrokenPersistentEntityReference);
        }
        owners.push((owner, row));
    }
    for row in 0..snapshot.grid.water_regions.len() {
        let Some((owner, _)) = owners
            .iter()
            .find(|(_, candidate_row)| *candidate_row == row as u32)
        else {
            continue;
        };
        let view = regions
            .get(*owner)
            .ok_or(Failure::BrokenPersistentEntityReference)?;
        if view.terrain().id() != handle.id() {
            return Err(Failure::BrokenPersistentEntityReference);
        }
        if view.edited_state().is_none() {
            continue;
        }
        let biome = view.biome(asset).ok_or(Failure::UnknownAssetIdentifier)?;
        let biome_index = asset
            .canonical_terrain_grid()
            .biomes
            .iter()
            .position(|candidate| std::ptr::eq(candidate, biome))
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let region = &mut snapshot.grid.water_regions[row];
        region.height_relative_to_base_metres = view
            .surface_height(asset)
            .ok_or(Failure::CorruptSnapshotSection)?
            - snapshot.grid.units.base_height_metres;
        region.water_style_index = biome_index as u32;
        region.depth_ratio = view
            .depth_ratio(asset)
            .ok_or(Failure::CorruptSnapshotSection)?;
        region.uses_tank_minimum_depth = view
            .uses_tank_minimum_depth(asset)
            .ok_or(Failure::CorruptSnapshotSection)?;
    }
    snapshot.retired_water_region_rows = retired_sources
        .filter(|source| source.terrain().id() == handle.id())
        .map(|source| {
            u32::try_from(source.source_region_index()).map_err(|_| Failure::CapacityExceeded)
        })
        .collect::<Result<_, _>>()?;
    snapshot.retired_water_region_rows.sort_unstable();
    snapshot.retired_water_region_rows.dedup();
    let mut chunks = chunks
        .filter(|(chunk, _)| chunk.asset.id() == handle.id())
        .collect::<Vec<_>>();
    chunks.sort_unstable_by_key(|(chunk, _)| (chunk.source_coord.y, chunk.source_coord.x));
    for (chunk, edited) in chunks {
        let Some(edited) = edited else {
            continue;
        };
        let mut samples = edited.clone();
        let mut edited_water_owner_samples = Vec::new();
        if samples.water_affiliations.len() != usize::from(chunk.side).pow(2) {
            return Err(Failure::CorruptSnapshotSection);
        }
        for (local, affiliation) in samples.water_affiliations.iter_mut().enumerate() {
            let Some(TerrainWaterAffiliation::Edited(owner)) = *affiliation else {
                continue;
            };
            // Retired or removed owners are already treated as no owner by live membership projection.
            let Some(view) = regions.get(owner) else {
                *affiliation = None;
                continue;
            };
            if !owner_is_in_world(owner) || view.terrain().id() != handle.id() {
                return Err(Failure::BrokenPersistentEntityReference);
            }
            let row =
                if let Some((_, row)) = owners.iter().find(|(candidate, _)| *candidate == owner) {
                    *row
                } else {
                    let biome = view.biome(asset).ok_or(Failure::UnknownAssetIdentifier)?;
                    let biome_index = asset
                        .canonical_terrain_grid()
                        .biomes
                        .iter()
                        .position(|candidate| std::ptr::eq(candidate, biome))
                        .ok_or(Failure::UnknownAssetIdentifier)?;
                    let row = u32::try_from(snapshot.grid.water_regions.len())
                        .map_err(|_| Failure::CapacityExceeded)?;
                    snapshot.grid.water_regions.push(TerrainWaterRegion {
                        height_relative_to_base_metres: view
                            .surface_height(asset)
                            .ok_or(Failure::CorruptSnapshotSection)?
                            - snapshot.grid.units.base_height_metres,
                        water_style_index: biome_index as u32,
                        row_major_cell_indices: Vec::new(),
                        depth_ratio: view
                            .depth_ratio(asset)
                            .ok_or(Failure::CorruptSnapshotSection)?,
                        uses_tank_minimum_depth: view
                            .uses_tank_minimum_depth(asset)
                            .ok_or(Failure::CorruptSnapshotSection)?,
                    });
                    owners.push((owner, row));
                    row
                };
            *affiliation = Some(TerrainWaterAffiliation::SourceRow(row));
            edited_water_owner_samples.push(local as u32);
        }
        snapshot
            .edited_chunks
            .push(TerrainEditedChunkSnapshotRecord {
                source_coord: chunk.source_coord.to_array(),
                side: chunk.side,
                samples,
                edited_water_owner_samples,
            });
    }
    let mut remaining_owners = regions
        .iter()
        .filter(|(owner, view)| owner_is_in_world(*owner) && view.terrain().id() == handle.id())
        .collect::<Vec<_>>();
    remaining_owners.sort_unstable_by_key(|(owner, _)| *owner);
    for (owner, view) in remaining_owners {
        if let Some((_, row)) = owners.iter().find(|(candidate, _)| *candidate == owner) {
            if let Some(edit) = view.edited_state() {
                snapshot
                    .edited_water_surface_heights
                    .push((*row, edit.surface_height_metres));
            }
            continue;
        }
        let biome = view.biome(asset).ok_or(Failure::UnknownAssetIdentifier)?;
        let biome_index = asset
            .canonical_terrain_grid()
            .biomes
            .iter()
            .position(|candidate| std::ptr::eq(candidate, biome))
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let row = u32::try_from(snapshot.grid.water_regions.len())
            .map_err(|_| Failure::CapacityExceeded)?;
        if let Some(edit) = view.edited_state() {
            snapshot
                .edited_water_surface_heights
                .push((row, edit.surface_height_metres));
        }
        snapshot.grid.water_regions.push(TerrainWaterRegion {
            height_relative_to_base_metres: view
                .surface_height(asset)
                .ok_or(Failure::CorruptSnapshotSection)?
                - snapshot.grid.units.base_height_metres,
            water_style_index: biome_index as u32,
            row_major_cell_indices: Vec::new(),
            depth_ratio: view
                .depth_ratio(asset)
                .ok_or(Failure::CorruptSnapshotSection)?,
            uses_tank_minimum_depth: view
                .uses_tank_minimum_depth(asset)
                .ok_or(Failure::CorruptSnapshotSection)?,
        });
    }
    validate_terrain_snapshot_record(&snapshot)?;
    Ok(snapshot)
}
