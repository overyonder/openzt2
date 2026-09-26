//! Loaded canonical terrain data and its typed Bevy dependency handles.

use std::{collections::HashMap, io};

use bevy::{audio::AudioSource, gltf::Gltf, prelude::*};
use openzt2_game_data::terrain::TerrainGrid;

use crate::assets::material::material_asset_types::MaterialAsset;
use crate::assets::scene_prefab::ScenePrefabAsset;

#[derive(Asset, TypePath, Debug)]
pub struct TerrainAsset {
    pub(in crate::assets::terrain) canonical_terrain_grid: TerrainGrid,
    /// Immutable source-membership index, not settled live water-side ownership.
    pub(in crate::assets::terrain) authored_water_region_membership: Box<[u32]>,
    pub(in crate::assets::terrain) lowered_terrain_model: Handle<Gltf>,
    pub(in crate::assets::terrain) terrain_effect_material: Handle<MaterialAsset>,
    pub(in crate::assets::terrain) water_bump_combination_effect_material: Handle<MaterialAsset>,
    pub(in crate::assets::terrain) water_surface_effect_material: Handle<MaterialAsset>,
    pub(in crate::assets::terrain) waterfall_decal_effect_material: Handle<MaterialAsset>,
    pub(in crate::assets::terrain) texture_images_by_normalized_source_path:
        HashMap<String, Handle<Image>>,
    pub(in crate::assets::terrain) waterfall_scene_prefab: Option<Handle<ScenePrefabAsset>>,
    pub(in crate::assets::terrain) waterfall_audio: Option<Handle<AudioSource>>,
}

impl TerrainAsset {
    pub(in crate::assets::terrain) fn index_authored_water_region_membership(
        grid: &TerrainGrid,
    ) -> io::Result<Box<[u32]>> {
        if grid.water_regions.is_empty() {
            return Ok(Box::default());
        }
        let mut membership = vec![0_u32; grid.samples.len()];
        for (region_index, region) in grid.water_regions.iter().enumerate() {
            let region_key = u32::try_from(region_index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .filter(|key| *key != u32::MAX)
                .ok_or_else(|| io::Error::other("too many terrain water regions"))?;
            for index in &region.row_major_cell_indices {
                let index = usize::try_from(*index).map_err(io::Error::other)?;
                let owner = membership.get_mut(index).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "terrain water region references a missing sample",
                    )
                })?;
                // A repeated cell in one row is harmless; competing rows need
                // native attachment ordering and cannot establish an owner.
                *owner = if *owner == 0 || *owner == region_key {
                    region_key
                } else {
                    u32::MAX
                };
            }
        }
        Ok(membership.into_boxed_slice())
    }

    /// Returns saved region membership only; attachment may still be rejected
    /// by native side bounds or superseded by current water edits.
    pub(crate) fn sample_has_authored_water_region(&self, index: usize) -> Option<bool> {
        self.sample_authored_water_region_index(index)
            .map(|owner| owner.is_some())
    }

    /// The outer None denotes an invalid sample or ambiguous saved ownership.
    /// A unique saved row still requires live side-attachment validation.
    pub(crate) fn sample_authored_water_region_index(&self, index: usize) -> Option<Option<usize>> {
        self.canonical_terrain_grid.samples.get(index)?;
        if self.authored_water_region_membership.is_empty() {
            return Some(None);
        }
        match *self.authored_water_region_membership.get(index)? {
            0 => Some(None),
            u32::MAX => None,
            owner => Some(Some((owner - 1) as usize)),
        }
    }

    pub fn canonical_terrain_grid(&self) -> &TerrainGrid {
        &self.canonical_terrain_grid
    }

    pub fn lowered_terrain_model(&self) -> &Handle<Gltf> {
        &self.lowered_terrain_model
    }

    pub(crate) fn terrain_effect_material(&self) -> &Handle<MaterialAsset> {
        &self.terrain_effect_material
    }

    pub(crate) fn water_surface_effect_material(&self) -> &Handle<MaterialAsset> {
        &self.water_surface_effect_material
    }

    pub(crate) fn water_bump_combination_effect_material(&self) -> &Handle<MaterialAsset> {
        &self.water_bump_combination_effect_material
    }

    pub(crate) fn waterfall_decal_effect_material(&self) -> &Handle<MaterialAsset> {
        &self.waterfall_decal_effect_material
    }

    pub fn texture_image(&self, source_path: &str) -> Option<&Handle<Image>> {
        self.texture_images_by_normalized_source_path
            .get(&source_path.to_ascii_lowercase())
    }

    pub fn terrain_chunk_cells_per_side(&self) -> Option<u16> {
        let terrain_grid = &self.canonical_terrain_grid;
        (terrain_grid.sector_columns != 0 && terrain_grid.sector_rows != 0).then_some(())?;
        let width_in_cells = terrain_grid.width.checked_sub(1)?;
        let height_in_cells = terrain_grid.height.checked_sub(1)?;
        (width_in_cells % terrain_grid.sector_columns == 0
            && height_in_cells % terrain_grid.sector_rows == 0)
            .then_some(())?;
        let horizontal_cells_per_sector = width_in_cells / terrain_grid.sector_columns;
        let vertical_cells_per_sector = height_in_cells / terrain_grid.sector_rows;
        (horizontal_cells_per_sector == vertical_cells_per_sector)
            .then(|| u16::try_from(horizontal_cells_per_sector).ok())
            .flatten()
    }

    pub fn waterfall_scene_prefab(&self) -> Option<&Handle<ScenePrefabAsset>> {
        self.waterfall_scene_prefab.as_ref()
    }

    pub fn waterfall_audio(&self) -> Option<&Handle<AudioSource>> {
        self.waterfall_audio.as_ref()
    }
}
