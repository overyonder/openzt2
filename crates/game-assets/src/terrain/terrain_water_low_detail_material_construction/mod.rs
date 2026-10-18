//! Fixed-function terrain water materials using the native material lowering path.

use std::io;

use bevy::{asset::LoadContext, prelude::*};
use openzt2_game_data::terrain::TerrainGrid;

use crate::{
    asset_source::AssetArchives,
    material::{
        evaluated_effect_material_asset_construction::create_material_asset_from_evaluated_d3d9_effect,
        material_asset_types::MaterialAsset,
    },
    model_source::{
        model::{NativeMaterialSource, NativeTextureEffectSource, NativeTextureSource},
        netimmerse_fixed_function_material_effect_lowering::lower_netimmerse_material_into_evaluated_d3d9_effect,
    },
};

pub(super) fn create_terrain_water_low_detail_materials(
    grid: &TerrainGrid,
    archives: &AssetArchives,
    context: &mut LoadContext<'_>,
) -> io::Result<Box<[Option<Handle<MaterialAsset>>]>> {
    create_terrain_water_fixed_detail_materials(grid, archives, context, false)
}

pub(super) fn create_terrain_water_medium_detail_materials(
    grid: &TerrainGrid,
    archives: &AssetArchives,
    context: &mut LoadContext<'_>,
) -> io::Result<Box<[Option<Handle<MaterialAsset>>]>> {
    create_terrain_water_fixed_detail_materials(grid, archives, context, true)
}

fn create_terrain_water_fixed_detail_materials(
    grid: &TerrainGrid,
    archives: &AssetArchives,
    context: &mut LoadContext<'_>,
    medium: bool,
) -> io::Result<Box<[Option<Handle<MaterialAsset>>]>> {
    let mut materials = Vec::with_capacity(grid.biomes.len());
    for (biome_index, biome) in grid.biomes.iter().enumerate() {
        let Some(water) = &biome.water_presentation else {
            materials.push(None);
            continue;
        };
        let path = &water.texture_animations[0].texture;
        let resolved = archives
            .resolve_ui_reference(context.path().path(), path)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("missing water texture {path}"),
                )
            })?;
        let quality = if medium { "medium" } else { "low" };
        let color = if medium {
            water.vertex_colour_medium
        } else {
            water.vertex_colour_low
        }
        .map(|value| f32::from(value) / 255.0);
        let optional_texture = |path: &str, uv_set| {
            (!path.is_empty()).then(|| archives.resolve_ui_reference(context.path().path(), path)).flatten()
                .map(|resolved| NativeTextureSource {
                    asset_path: crate::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(&resolved.to_string_lossy()),
                    uv_set, clamp_mode: 3, filter_mode: 0,
                })
        };
        let material = NativeMaterialSource {
            name: format!("terrain-water-{biome_index}-{quality}"),
            base_color: [0.0, 0.0, 0.0, color[3]],
            ambient: [color[0] * 0.5, color[1] * 0.5, color[2] * 0.5],
            specular: [0.0; 3],
            emissive: [0.0; 3],
            glossiness: 4.0,
            base_texture: Some(NativeTextureSource {
                asset_path: crate::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(&resolved.to_string_lossy()),
                uv_set: 0,
                clamp_mode: 3,
                filter_mode: 0,
            }),
            detail_texture: None,
            glow_texture: None,
            gloss_texture: medium.then(|| optional_texture(&water.gloss_map, 0)).flatten(),
            texture_effect: medium.then_some(NativeTextureEffectSource::SphereEnvironment {
                texture: None, clamp_mode: 3, filter_mode: 1,
                model_to_effect_rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            }),
            decal_0_texture: medium.then(|| optional_texture(&water.texture_animations[1].texture, 1)).flatten(),
            texture_apply_mode: 2,
            vertex_color_mode: 0,
            lighting_mode: 1,
            alpha_blend: true,
            source_blend_mode: 6,
            destination_blend_mode: 7,
            alpha_cutoff: None,
            alpha_test_mode: 0,
            depth_test: true,
            depth_write: true,
            depth_test_mode: 6,
            cull_mode: 3,
        };
        let lowered = lower_netimmerse_material_into_evaluated_d3d9_effect(&material)?;
        let label = format!("water-fixed-{biome_index}-{quality}");
        let material = create_material_asset_from_evaluated_d3d9_effect(
            lowered.evaluated_effect,
            lowered.texture_asset_paths,
            &label,
            context,
            &std::sync::Arc::default(),
            &lowered.reflection_projections,
        )?;
        materials.push(Some(context.add_labeled_asset(label, material)));
    }
    Ok(materials.into_boxed_slice())
}
