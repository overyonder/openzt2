//! Loaded canonical terrain data and its typed Bevy dependency handles.

use std::{io, path::PathBuf};

use bevy::{audio::AudioSource, platform::collections::HashMap, prelude::*};
use openzt2_game_data::terrain::{TerrainBiome, TerrainGrid, TerrainShorelineRippleTiming};

use crate::effect::ParticleEffectDocumentAsset;
use crate::material::material_asset_types::MaterialAsset;
use crate::scene_prefab::ScenePrefabAsset;

#[derive(Asset, TypePath, Debug)]
pub struct TerrainAsset {
    pub(in crate::terrain) canonical_terrain_grid: TerrainGrid,
    /// Immutable source-membership index, not settled live water-side ownership.
    pub(in crate::terrain) authored_water_region_membership: Box<[u32]>,
    pub(in crate::terrain) water_biomes_by_material:
        std::collections::BTreeMap<openzt2_game_data::AssetId, usize>,
    #[dependency]
    pub(in crate::terrain) chunk_meshes: Vec<Handle<Mesh>>,
    #[dependency]
    pub(in crate::terrain) terrain_effect_material: Handle<MaterialAsset>,
    #[dependency]
    pub(in crate::terrain) water_bump_combination_effect_material: Handle<MaterialAsset>,
    #[dependency]
    pub(in crate::terrain) water_surface_effect_material: Handle<MaterialAsset>,
    #[dependency]
    pub(in crate::terrain) water_low_detail_materials: Vec<Option<Handle<MaterialAsset>>>,
    #[dependency]
    pub(in crate::terrain) water_medium_detail_materials: Vec<Option<Handle<MaterialAsset>>>,
    #[dependency]
    pub(in crate::terrain) waterfall_decal_effect_material: Handle<MaterialAsset>,
    #[dependency]
    pub(in crate::terrain) texture_images_by_normalized_source_path: HashMap<String, Handle<Image>>,
    #[dependency]
    pub(in crate::terrain) waterfall_scene_prefab: Option<Handle<ScenePrefabAsset>>,
    #[dependency]
    pub(in crate::terrain) waterfall_audio: Option<Handle<AudioSource>>,
    #[dependency]
    pub(in crate::terrain) water_scene_prefabs_by_normalized_system:
        HashMap<String, Handle<ScenePrefabAsset>>,
    #[dependency]
    pub(in crate::terrain) water_materials_by_normalized_source_path:
        HashMap<String, Handle<MaterialAsset>>,
    #[dependency]
    pub(in crate::terrain) water_particle_effects_by_normalized_system:
        HashMap<String, Handle<ParticleEffectDocumentAsset>>,
    pub(in crate::terrain) named_water_particle_sources:
        HashMap<openzt2_game_data::AssetId, PathBuf>,
}

impl TerrainAsset {
    /// Resolves the saved configuration key through the DAT's biome-name order.
    /// An unknown key uses the first resolved configuration, as the source reader does.
    pub fn water_biome_for_region(&self, region_index: usize) -> Option<&TerrainBiome> {
        let grid = &self.canonical_terrain_grid;
        let region = grid.water_regions.get(region_index)?;
        usize::try_from(region.water_style_index)
            .ok()
            .and_then(|index| grid.biomes.get(index))
            .or_else(|| grid.biomes.first())
    }

    pub fn water_low_detail_material(
        &self,
        biome: &TerrainBiome,
    ) -> Option<&Handle<MaterialAsset>> {
        let index = self
            .canonical_terrain_grid
            .biomes
            .iter()
            .position(|candidate| std::ptr::eq(candidate, biome))?;
        self.water_low_detail_materials.get(index)?.as_ref()
    }

    pub fn water_medium_detail_material(
        &self,
        biome: &TerrainBiome,
    ) -> Option<&Handle<MaterialAsset>> {
        let index = self
            .canonical_terrain_grid
            .biomes
            .iter()
            .position(|candidate| std::ptr::eq(candidate, biome))?;
        self.water_medium_detail_materials.get(index)?.as_ref()
    }

    pub fn tank_water_material_presentation(
        &self,
        id: openzt2_game_data::AssetId,
    ) -> Option<(
        &openzt2_game_data::terrain::TerrainBiomeWaterPresentation,
        &Handle<MaterialAsset>,
        &Handle<MaterialAsset>,
        &Handle<MaterialAsset>,
    )> {
        let index = *self.water_biomes_by_material.get(&id)?;
        Some((
            self.canonical_terrain_grid
                .biomes
                .get(index)?
                .water_presentation
                .as_ref()?,
            self.water_low_detail_materials.get(index)?.as_ref()?,
            self.water_medium_detail_materials.get(index)?.as_ref()?,
            &self.water_surface_effect_material,
        ))
    }

    pub fn water_scene_prefab(&self, system: &str) -> Option<&Handle<ScenePrefabAsset>> {
        self.water_scene_prefabs_by_normalized_system
            .get(&system.to_ascii_lowercase())
    }

    pub fn shoreline_ripple_for_region(
        &self,
        region_index: usize,
    ) -> Option<(&Handle<ScenePrefabAsset>, TerrainShorelineRippleTiming)> {
        let shoreline = &self
            .water_biome_for_region(region_index)?
            .water_presentation
            .as_ref()?
            .shoreline;
        let prefab = self.water_scene_prefab(shoreline.ripple_system.as_deref()?)?;
        Some((prefab, shoreline.ripple_timing()))
    }

    pub fn water_material(&self, source_path: &str) -> Option<&Handle<MaterialAsset>> {
        self.water_materials_by_normalized_source_path
            .get(&source_path.to_ascii_lowercase())
    }

    pub fn water_particle_effect(
        &self,
        system: &str,
    ) -> Option<&Handle<ParticleEffectDocumentAsset>> {
        self.water_particle_effects_by_normalized_system
            .get(&system.to_ascii_lowercase())
    }

    /// Resolves a named water particle system on demand.
    pub fn load_named_water_particle_effect(
        &self,
        system: openzt2_game_data::AssetId,
        assets: &AssetServer,
    ) -> Option<Handle<ParticleEffectDocumentAsset>> {
        Some(assets.load(self.named_water_particle_sources.get(&system)?.clone()))
    }

    pub(in crate::terrain) fn index_authored_water_region_membership(
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
    pub fn sample_has_authored_water_region(&self, index: usize) -> Option<bool> {
        self.sample_authored_water_region_index(index)
            .map(|owner| owner.is_some())
    }

    /// The outer None denotes an invalid sample or ambiguous saved ownership.
    /// A unique saved row still requires live side-attachment validation.
    pub fn sample_authored_water_region_index(&self, index: usize) -> Option<Option<usize>> {
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

    pub fn chunk_mesh(&self, x: u32, z: u32) -> Option<&Handle<Mesh>> {
        let grid = self.canonical_terrain_grid();
        if x >= grid.sector_columns || z >= grid.sector_rows {
            return None;
        }
        let index = usize::try_from(z)
            .ok()?
            .checked_mul(grid.sector_columns as usize)?
            .checked_add(x as usize)?;
        self.chunk_meshes.get(index)
    }

    pub fn terrain_effect_material(&self) -> &Handle<MaterialAsset> {
        &self.terrain_effect_material
    }

    pub fn water_surface_effect_material(&self) -> &Handle<MaterialAsset> {
        &self.water_surface_effect_material
    }

    pub fn water_bump_combination_effect_material(&self) -> &Handle<MaterialAsset> {
        &self.water_bump_combination_effect_material
    }

    pub fn waterfall_decal_effect_material(&self) -> &Handle<MaterialAsset> {
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
