use std::collections::HashSet;

use openzt2_game_data::terrain::TerrainGrid;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainWaterAffiliation;

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure;
use super::terrain_snapshot_types::{TerrainEditedChunkSnapshotRecord, TerrainSnapshotRecord};

const TERRAIN_SNAPSHOT_MAGIC: &[u8; 4] = b"TRN1";
const MAXIMUM_TERRAIN_SNAPSHOT_BYTES: usize = 512 * 1024 * 1024;
const MAXIMUM_TERRAIN_SAMPLE_COUNT: usize = 4 * 1024 * 1024;
const MAXIMUM_EDITED_SAMPLE_COUNT: usize = 8 * 1024 * 1024;
const MAXIMUM_WATER_REGION_COUNT: usize = 65_536;

pub(super) fn encode_terrain_snapshot_record(
    record: &TerrainSnapshotRecord,
) -> Result<Vec<u8>, Failure> {
    validate_terrain_snapshot_record(record)?;
    let grid = postcard::to_allocvec(&record.grid).map_err(|_| Failure::CorruptSnapshotSection)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(TERRAIN_SNAPSHOT_MAGIC);
    append_count(&mut bytes, grid.len())?;
    bytes.extend_from_slice(&grid);
    bytes.extend_from_slice(&record.original_water_region_count.to_le_bytes());
    append_count(&mut bytes, record.edited_water_surface_heights.len())?;
    for (row, height) in &record.edited_water_surface_heights {
        bytes.extend_from_slice(&row.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
    }
    append_count(&mut bytes, record.retired_water_region_rows.len())?;
    for row in &record.retired_water_region_rows {
        bytes.extend_from_slice(&row.to_le_bytes());
    }
    append_count(&mut bytes, record.edited_chunks.len())?;
    for chunk in &record.edited_chunks {
        for coordinate in chunk.source_coord {
            bytes.extend_from_slice(&coordinate.to_le_bytes());
        }
        bytes.extend_from_slice(&chunk.side.to_le_bytes());
        let samples = &chunk.samples;
        for height in &samples.heights_cm {
            bytes.extend_from_slice(&height.to_le_bytes());
        }
        bytes.extend_from_slice(&samples.blend_weights);
        bytes.extend_from_slice(&samples.ground_cover);
        for style in &samples.water_styles {
            bytes.extend_from_slice(&style.to_le_bytes());
        }
        bytes.extend_from_slice(&samples.water_biome_channels);
        for height in &samples.water_cm {
            bytes.extend_from_slice(&height.to_le_bytes());
        }
        for affiliation in &samples.water_affiliations {
            match affiliation {
                None => bytes.push(0),
                Some(TerrainWaterAffiliation::SourceRow(row)) => {
                    bytes.push(1);
                    bytes.extend_from_slice(&row.to_le_bytes());
                }
                Some(TerrainWaterAffiliation::Unresolved) => bytes.push(2),
                Some(TerrainWaterAffiliation::Edited(_)) => {
                    return Err(Failure::BrokenPersistentEntityReference);
                }
            }
        }
        append_count(&mut bytes, chunk.edited_water_owner_samples.len())?;
        for index in &chunk.edited_water_owner_samples {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        if bytes.len() > MAXIMUM_TERRAIN_SNAPSHOT_BYTES {
            return Err(Failure::CapacityExceeded);
        }
    }
    if bytes.len() > MAXIMUM_TERRAIN_SNAPSHOT_BYTES {
        return Err(Failure::CapacityExceeded);
    }
    Ok(bytes)
}

pub(super) fn decode_terrain_snapshot_record(
    mut bytes: &[u8],
) -> Result<TerrainSnapshotRecord, Failure> {
    if bytes.len() > MAXIMUM_TERRAIN_SNAPSHOT_BYTES {
        return Err(Failure::CapacityExceeded);
    }
    if take_bytes(&mut bytes, 4)? != TERRAIN_SNAPSHOT_MAGIC {
        return Err(Failure::CorruptSnapshotSection);
    }
    let grid_byte_count = read_u32(&mut bytes)? as usize;
    let grid_bytes = take_bytes(&mut bytes, grid_byte_count)?;
    let (grid, remaining): (TerrainGrid, _) =
        postcard::take_from_bytes(grid_bytes).map_err(|_| Failure::CorruptSnapshotSection)?;
    if !remaining.is_empty() {
        return Err(Failure::CorruptSnapshotSection);
    }
    let original_water_region_count = read_u32(&mut bytes)?;
    let surface_count = read_u32(&mut bytes)? as usize;
    if surface_count > MAXIMUM_WATER_REGION_COUNT {
        return Err(Failure::CapacityExceeded);
    }
    let mut edited_water_surface_heights = Vec::new();
    for _ in 0..surface_count {
        let row = read_u32(&mut bytes)?;
        let height = f32::from_le_bytes(
            take_bytes(&mut bytes, 4)?
                .try_into()
                .map_err(|_| Failure::CorruptSnapshotSection)?,
        );
        edited_water_surface_heights.push((row, height));
    }
    let mut record = TerrainSnapshotRecord {
        grid,
        original_water_region_count,
        edited_water_surface_heights,
        edited_chunks: Vec::new(),
        retired_water_region_rows: Vec::new(),
    };
    validate_terrain_snapshot_record(&record)?;
    let retired_count = read_u32(&mut bytes)? as usize;
    if retired_count > record.grid.water_regions.len() {
        return Err(Failure::CorruptSnapshotSection);
    }
    for _ in 0..retired_count {
        record.retired_water_region_rows.push(read_u32(&mut bytes)?);
    }
    let chunk_count = read_u32(&mut bytes)? as usize;
    let maximum_chunks = u64::from(record.grid.sector_columns) * u64::from(record.grid.sector_rows);
    if chunk_count as u64 > maximum_chunks {
        return Err(Failure::CorruptSnapshotSection);
    }
    let mut total_samples = 0_usize;
    for _ in 0..chunk_count {
        let source_coord = [read_i32(&mut bytes)?, read_i32(&mut bytes)?];
        let side = read_u16(&mut bytes)?;
        validate_chunk_geometry(&record.grid, source_coord, side)?;
        let count = usize::from(side).pow(2);
        total_samples = total_samples
            .checked_add(count)
            .ok_or(Failure::CapacityExceeded)?;
        if total_samples > MAXIMUM_EDITED_SAMPLE_COUNT {
            return Err(Failure::CapacityExceeded);
        }
        // Every sample has 24 bytes before its variable-length affiliation.
        if bytes.len() < count.checked_mul(25).ok_or(Failure::CapacityExceeded)? {
            return Err(Failure::CorruptSnapshotSection);
        }
        let heights_cm = take_bytes(&mut bytes, count * 2)?
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let blend_weights = take_bytes(&mut bytes, count * 16)?.into();
        let ground_cover = take_bytes(&mut bytes, count)?.into();
        let water_styles = take_bytes(&mut bytes, count * 2)?
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let water_biome_channels = take_bytes(&mut bytes, count)?.into();
        let water_cm = take_bytes(&mut bytes, count * 2)?
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let mut affiliations = Vec::with_capacity(count);
        for _ in 0..count {
            affiliations.push(match take_bytes(&mut bytes, 1)?[0] {
                0 => None,
                1 => Some(TerrainWaterAffiliation::SourceRow(read_u32(&mut bytes)?)),
                2 => Some(TerrainWaterAffiliation::Unresolved),
                _ => return Err(Failure::CorruptSnapshotSection),
            });
        }
        let edited_owner_count = read_u32(&mut bytes)? as usize;
        if edited_owner_count > count {
            return Err(Failure::CorruptSnapshotSection);
        }
        let mut edited_water_owner_samples = Vec::with_capacity(edited_owner_count);
        for _ in 0..edited_owner_count {
            edited_water_owner_samples.push(read_u32(&mut bytes)?);
        }
        record.edited_chunks.push(TerrainEditedChunkSnapshotRecord {
            source_coord,
            side,
            edited_water_owner_samples,
            samples: EditedTerrainSamples {
                heights_cm,
                blend_weights,
                ground_cover,
                water_styles,
                water_biome_channels,
                water_cm,
                water_affiliations: affiliations.into_boxed_slice(),
            },
        });
    }
    if !bytes.is_empty() {
        return Err(Failure::CorruptSnapshotSection);
    }
    validate_terrain_snapshot_record(&record)?;
    Ok(record)
}

pub(super) fn validate_terrain_snapshot_record(
    record: &TerrainSnapshotRecord,
) -> Result<(), Failure> {
    let grid = &record.grid;
    if record.original_water_region_count as usize > grid.water_regions.len()
        || grid
            .water_regions
            .iter()
            .skip(record.original_water_region_count as usize)
            .any(|region| !region.row_major_cell_indices.is_empty())
    {
        return Err(Failure::CorruptSnapshotSection);
    }
    let sample_count = u64::from(grid.width) * u64::from(grid.height);
    if sample_count > MAXIMUM_TERRAIN_SAMPLE_COUNT as u64
        || grid.water_regions.len() > MAXIMUM_WATER_REGION_COUNT
    {
        return Err(Failure::CapacityExceeded);
    }
    if grid.width < 5
        || grid.height < 5
        || grid.sector_columns == 0
        || grid.sector_rows == 0
        || grid.samples.len() as u64 != sample_count
        || grid.biomes.is_empty()
        || grid.biomes.len() > 16
        || !(grid.width - 1).is_multiple_of(4)
        || !(grid.height - 1).is_multiple_of(4)
        || !(grid.width - 1).is_multiple_of(grid.sector_columns)
        || !(grid.height - 1).is_multiple_of(grid.sector_rows)
    {
        return Err(Failure::CorruptSnapshotSection);
    }
    let cells = (grid.width - 1) / grid.sector_columns;
    if cells != (grid.height - 1) / grid.sector_rows
        || cells == 0
        || cells >= u32::from(u16::MAX)
        || grid.slope_tiles.len() as u64
            != u64::from((grid.width - 1) / 4) * u64::from((grid.height - 1) / 4)
    {
        return Err(Failure::CorruptSnapshotSection);
    }
    let units = grid.units;
    let presentation = &grid.presentation;
    let fade = presentation.water_distance_fade;
    let editing = presentation.water_editing;
    let finite = [
        units.cell_size_metres,
        units.base_height_metres,
        units.maximum_height_metres,
        presentation.detail_texture_repetition,
        presentation.cliff_rise_per_metre[0],
        presentation.cliff_rise_per_metre[1],
        fade.time_seconds,
        fade.minimum_distance,
        fade.maximum_distance,
        fade.search,
        fade.minimum_fresnel,
        fade.maximum_fresnel,
        editing.height_snap_interval_metres,
        editing.height_snap_base_metres,
        editing.plane_merge_tolerance_metres,
    ]
    .into_iter()
    .all(f32::is_finite)
        && units.minimum_water_height_metres.is_none_or(f32::is_finite);
    if !finite
        || units.cell_size_metres <= 0.0
        || !(units.cell_size_metres * (grid.width - 1) as f32).is_finite()
        || !(units.cell_size_metres * (grid.height - 1) as f32).is_finite()
    {
        return Err(Failure::CorruptSnapshotSection);
    }
    for sample in &grid.samples {
        if usize::from(sample.biome_index) >= grid.biomes.len()
            || !sample.height_relative_to_base_metres.is_finite()
            || !(units.base_height_metres + sample.height_relative_to_base_metres).is_finite()
            || !sample.support_floor_metres.is_none_or(f32::is_finite)
            || !sample.height_cap_metres.is_none_or(f32::is_finite)
        {
            return Err(Failure::CorruptSnapshotSection);
        }
    }
    let mut total_members = 0_usize;
    for region in &grid.water_regions {
        total_members = total_members
            .checked_add(region.row_major_cell_indices.len())
            .ok_or(Failure::CapacityExceeded)?;
        if total_members > MAXIMUM_EDITED_SAMPLE_COUNT {
            return Err(Failure::CapacityExceeded);
        }
        if !region.height_relative_to_base_metres.is_finite()
            || !(units.base_height_metres + region.height_relative_to_base_metres).is_finite()
            || !region.depth_ratio.is_finite()
            || region
                .row_major_cell_indices
                .iter()
                .any(|index| u64::from(*index) >= sample_count)
        {
            return Err(Failure::CorruptSnapshotSection);
        }
    }
    if let Some(waterfall) = &grid.waterfall {
        if ![
            waterfall.decal_width_metres,
            waterfall.decal_height_metres,
            waterfall.height_offset_metres,
            waterfall.detail_v_scroll,
            waterfall.particle_scrunch,
        ]
        .into_iter()
        .all(f32::is_finite)
        {
            return Err(Failure::CorruptSnapshotSection);
        }
    }
    let mut retired = HashSet::new();
    for row in &record.retired_water_region_rows {
        if *row >= record.original_water_region_count || !retired.insert(*row) {
            return Err(Failure::CorruptSnapshotSection);
        }
    }
    let mut edited_surfaces = HashSet::new();
    for (row, height) in &record.edited_water_surface_heights {
        if *row as usize >= grid.water_regions.len()
            || grid.water_regions[*row as usize].water_style_index as usize >= grid.biomes.len()
            || !height.is_finite()
            || retired.contains(row)
            || !edited_surfaces.insert(*row)
        {
            return Err(Failure::CorruptSnapshotSection);
        }
    }
    let mut coordinates = HashSet::new();
    let mut edited_sample_count = 0_usize;
    for chunk in &record.edited_chunks {
        validate_chunk_geometry(grid, chunk.source_coord, chunk.side)?;
        if !coordinates.insert(chunk.source_coord) {
            return Err(Failure::CorruptSnapshotSection);
        }
        let count = usize::from(chunk.side).pow(2);
        edited_sample_count = edited_sample_count
            .checked_add(count)
            .ok_or(Failure::CapacityExceeded)?;
        if edited_sample_count > MAXIMUM_EDITED_SAMPLE_COUNT {
            return Err(Failure::CapacityExceeded);
        }
        let samples = &chunk.samples;
        if samples.heights_cm.len() != count
            || samples.blend_weights.len() != count * 16
            || samples.ground_cover.len() != count
            || samples.water_styles.len() != count
            || samples.water_biome_channels.len() != count
            || samples.water_cm.len() != count
            || samples.water_affiliations.len() != count
        {
            return Err(Failure::CorruptSnapshotSection);
        }
        if samples
            .blend_weights
            .chunks_exact(16)
            .any(|weights| weights[grid.biomes.len()..].iter().any(|value| *value != 0))
            || samples
                .water_biome_channels
                .iter()
                .any(|channel| usize::from(*channel) >= grid.biomes.len())
        {
            return Err(Failure::CorruptSnapshotSection);
        }
        for affiliation in &samples.water_affiliations {
            match affiliation {
                Some(TerrainWaterAffiliation::SourceRow(row))
                    if *row as usize >= grid.water_regions.len() =>
                {
                    return Err(Failure::BrokenPersistentEntityReference);
                }
                Some(TerrainWaterAffiliation::Edited(_)) => {
                    return Err(Failure::BrokenPersistentEntityReference);
                }
                _ => {}
            }
        }
        let mut edited_owner_samples = HashSet::new();
        for index in &chunk.edited_water_owner_samples {
            if !edited_owner_samples.insert(*index)
                || !matches!(samples.water_affiliations.get(*index as usize),
                Some(Some(TerrainWaterAffiliation::SourceRow(row))) if !retired.contains(row))
            {
                return Err(Failure::BrokenPersistentEntityReference);
            }
        }
    }
    Ok(())
}

fn validate_chunk_geometry(
    grid: &TerrainGrid,
    coordinate: [i32; 2],
    side: u16,
) -> Result<(), Failure> {
    if coordinate[0] < 0
        || coordinate[1] < 0
        || coordinate[0] as u32 >= grid.sector_columns
        || coordinate[1] as u32 >= grid.sector_rows
        || u32::from(side) != (grid.width - 1) / grid.sector_columns + 1
    {
        return Err(Failure::CorruptSnapshotSection);
    }
    Ok(())
}

fn append_count(bytes: &mut Vec<u8>, count: usize) -> Result<(), Failure> {
    bytes.extend_from_slice(
        &u32::try_from(count)
            .map_err(|_| Failure::CapacityExceeded)?
            .to_le_bytes(),
    );
    Ok(())
}

fn take_bytes<'a>(bytes: &mut &'a [u8], count: usize) -> Result<&'a [u8], Failure> {
    let (value, rest) = bytes
        .split_at_checked(count)
        .ok_or(Failure::CorruptSnapshotSection)?;
    *bytes = rest;
    Ok(value)
}

fn read_u32(bytes: &mut &[u8]) -> Result<u32, Failure> {
    Ok(u32::from_le_bytes(
        take_bytes(bytes, 4)?
            .try_into()
            .map_err(|_| Failure::CorruptSnapshotSection)?,
    ))
}
fn read_i32(bytes: &mut &[u8]) -> Result<i32, Failure> {
    Ok(i32::from_le_bytes(
        take_bytes(bytes, 4)?
            .try_into()
            .map_err(|_| Failure::CorruptSnapshotSection)?,
    ))
}
fn read_u16(bytes: &mut &[u8]) -> Result<u16, Failure> {
    Ok(u16::from_le_bytes(
        take_bytes(bytes, 2)?
            .try_into()
            .map_err(|_| Failure::CorruptSnapshotSection)?,
    ))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use openzt2_game_data::terrain::{
        TerrainBiome, TerrainPresentation, TerrainSample, TerrainSlopeTile, TerrainUnits,
        TerrainWaterDepth, TerrainWaterRegion,
    };

    pub(in crate::persistence::slot_io) fn painted_nonflat_water_snapshot() -> TerrainSnapshotRecord
    {
        let biome = |name: &str| TerrainBiome {
            name: name.to_owned(),
            ground_texture: String::new(),
            cover_texture: String::new(),
            mix_mask: String::new(),
            cliff_textures: Default::default(),
            ground_brushes: Default::default(),
            cover_brushes: Default::default(),
            ground_no_blend_brushes: Default::default(),
            cover_no_blend_brushes: Default::default(),
            draw_order: 0,
            shallow_water_depth_mm: Some(300),
            deep_water_depth_mm: Some(3000),
            water_shore_offset_mm: None,
            water_presentation: None,
        };
        let grid = TerrainGrid {
            width: 5,
            height: 5,
            sector_columns: 1,
            sector_rows: 1,
            units: TerrainUnits {
                cell_size_metres: 2.0,
                base_height_metres: 5.125,
                maximum_height_metres: 40.0,
                minimum_water_height_metres: Some(-3.0),
            },
            presentation: TerrainPresentation {
                detail_texture: "detail.dds".into(),
                detail_texture_repetition: 3.0,
                cliff_rise_per_metre: [0.2, 0.6],
                surface_resolution: 1,
                surface_margin_cells: 0,
                full_alpha_composition: true,
                water_distance_fade: Default::default(),
                water_editing: Default::default(),
            },
            biomes: vec![biome("grassland"), biome("wetlands")],
            samples: (0..25)
                .map(|index| TerrainSample {
                    height_relative_to_base_metres: index as f32 * 0.12345,
                    biome_index: (index % 2) as u8,
                    has_ground_cover: index % 3 == 0,
                    water_depth: TerrainWaterDepth::Shallow,
                    water_surface_flag: index % 2 == 0,
                    excludes_elevated_path: index == 3,
                    support_floor_metres: Some(-10.25),
                    height_cap_metres: Some(42.5),
                    height_is_linked: index == 4,
                    has_child_height_link: index == 5,
                })
                .collect(),
            water_regions: vec![
                TerrainWaterRegion {
                    height_relative_to_base_metres: 3.625,
                    water_style_index: 1,
                    row_major_cell_indices: vec![7, 12, 17],
                    depth_ratio: 0.75,
                    uses_tank_minimum_depth: true,
                },
                TerrainWaterRegion {
                    height_relative_to_base_metres: 2.25,
                    water_style_index: 0,
                    row_major_cell_indices: Vec::new(),
                    depth_ratio: 0.5,
                    uses_tank_minimum_depth: false,
                },
            ],
            waterfall: None,
            slope_tiles: vec![TerrainSlopeTile {
                uses_alternate_diagonal: true,
            }],
        };
        let mut blend_weights = vec![0; 25 * 16];
        for weights in blend_weights.chunks_exact_mut(16) {
            weights[0] = 128;
            weights[1] = 127;
        }
        let mut affiliations = vec![None; 25];
        affiliations[7] = Some(TerrainWaterAffiliation::SourceRow(0));
        affiliations[12] = Some(TerrainWaterAffiliation::Unresolved);
        affiliations[17] = Some(TerrainWaterAffiliation::SourceRow(1));
        TerrainSnapshotRecord {
            grid,
            original_water_region_count: 1,
            edited_water_surface_heights: vec![(1, 7.375)],
            retired_water_region_rows: vec![0],
            edited_chunks: vec![TerrainEditedChunkSnapshotRecord {
                source_coord: [0, 0],
                side: 5,
                edited_water_owner_samples: vec![17],
                samples: EditedTerrainSamples {
                    heights_cm: (0..25).map(|index| 500 + index * 7).collect(),
                    blend_weights: blend_weights.into_boxed_slice(),
                    ground_cover: vec![1; 25].into_boxed_slice(),
                    water_styles: vec![2; 25].into_boxed_slice(),
                    water_biome_channels: vec![1; 25].into_boxed_slice(),
                    water_cm: vec![875; 25].into_boxed_slice(),
                    water_affiliations: affiliations.into_boxed_slice(),
                },
            }],
        }
    }

    #[test]
    fn nonflat_grid_full_blends_and_water_relationships_round_trip_exactly() {
        let original = painted_nonflat_water_snapshot();
        let encoded = encode_terrain_snapshot_record(&original).unwrap();
        assert_eq!(decode_terrain_snapshot_record(&encoded).unwrap(), original);
        for truncated in [0, 4, encoded.len() - 1] {
            assert!(decode_terrain_snapshot_record(&encoded[..truncated]).is_err());
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert!(decode_terrain_snapshot_record(&trailing).is_err());
    }

    #[test]
    fn invalid_water_references_and_nonfinite_geometry_are_rejected() {
        let mut invalid_edited_style = painted_nonflat_water_snapshot();
        let edited_row = invalid_edited_style.edited_water_surface_heights[0].0 as usize;
        invalid_edited_style.grid.water_regions[edited_row].water_style_index =
            invalid_edited_style.grid.biomes.len() as u32;
        assert!(encode_terrain_snapshot_record(&invalid_edited_style).is_err());
        let mut authored_style = painted_nonflat_water_snapshot();
        authored_style.grid.water_regions[0].water_style_index = u32::MAX;
        assert!(encode_terrain_snapshot_record(&authored_style).is_ok());
        let mut snapshot = painted_nonflat_water_snapshot();
        snapshot.edited_chunks[0].samples.water_affiliations[7] =
            Some(TerrainWaterAffiliation::SourceRow(42));
        assert!(encode_terrain_snapshot_record(&snapshot).is_err());
        snapshot.edited_chunks[0].samples.water_affiliations[7] = Some(
            TerrainWaterAffiliation::Edited(bevy::prelude::Entity::PLACEHOLDER),
        );
        assert!(encode_terrain_snapshot_record(&snapshot).is_err());
        snapshot.edited_chunks[0].samples.water_affiliations[7] = None;
        snapshot.grid.samples[3].height_relative_to_base_metres = f32::NAN;
        assert!(encode_terrain_snapshot_record(&snapshot).is_err());
    }

    #[test]
    fn capture_preserves_source_candidates_and_distinguishes_edited_owners() {
        use super::super::terrain_snapshot_capture::capture_terrain_snapshot_record;
        use bevy::{ecs::system::SystemState, prelude::*};
        use openzt2_game_assets::terrain::terrain_test_asset::terrain_test_asset;
        use openzt2_zoo_state::terrain::{
            terrain_chunk_types::TerrainChunk,
            terrain_water_presentation_types::{
                TerrainWaterRegionEdit, TerrainWaterRegionQuery, TerrainWaterRegionSource,
            },
        };

        let mut fixture = painted_nonflat_water_snapshot();
        fixture.grid.water_regions.truncate(1);
        let asset = terrain_test_asset(fixture.grid.clone());
        let handle = Handle::default();
        let mut world = World::new();
        world.spawn(TerrainWaterRegionSource::new(handle.clone(), 0));
        world.spawn(TerrainWaterRegionEdit {
            terrain: handle.clone(),
            biome_index: 1,
            surface_height_metres: 20.0,
            depth_ratio: 0.25,
            uses_tank_minimum_depth: true,
        });
        let source = TerrainWaterRegionSource::new(handle.clone(), 0);
        let source_owner = world
            .spawn(TerrainWaterRegionSource::new(handle.clone(), 0))
            .id();
        let edited_owner = world
            .spawn(TerrainWaterRegionEdit {
                terrain: handle.clone(),
                biome_index: 0,
                surface_height_metres: 7.375,
                depth_ratio: 0.5,
                uses_tank_minimum_depth: false,
            })
            .id();
        let mut samples = fixture.edited_chunks.remove(0).samples;
        samples.water_affiliations[17] = Some(TerrainWaterAffiliation::Edited(edited_owner));
        let chunk = TerrainChunk {
            source_coord: IVec2::ZERO,
            coord: IVec2::ZERO,
            asset: handle.clone(),
            origin: Vec2::ZERO,
            spacing_m: 2.0,
            side: 5,
        };
        let mut state = SystemState::<TerrainWaterRegionQuery>::new(&mut world);
        let regions = state.get(&world).unwrap();
        let snapshot = capture_terrain_snapshot_record(
            &handle,
            &asset,
            std::iter::once((&chunk, Some(&samples))),
            &regions,
            |owner| owner == source_owner || owner == edited_owner,
            std::iter::once((source_owner, &source)),
            std::iter::empty(),
        )
        .unwrap();
        assert_eq!(
            snapshot.grid.water_regions[0],
            fixture.grid.water_regions[0]
        );
        assert_eq!(snapshot.original_water_region_count, 1);
        assert_eq!(snapshot.grid.water_regions.len(), 2);
        assert!(snapshot.grid.water_regions[1]
            .row_major_cell_indices
            .is_empty());
        assert_eq!(
            snapshot.edited_chunks[0].edited_water_owner_samples,
            vec![17]
        );
        assert_eq!(
            snapshot.edited_chunks[0].samples.water_affiliations[17],
            Some(TerrainWaterAffiliation::SourceRow(1))
        );
        let encoded = encode_terrain_snapshot_record(&snapshot).unwrap();
        assert_eq!(decode_terrain_snapshot_record(&encoded).unwrap(), snapshot);
    }
}
