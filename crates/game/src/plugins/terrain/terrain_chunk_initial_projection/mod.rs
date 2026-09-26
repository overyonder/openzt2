use std::collections::hash_map::Entry;

use bevy::{
    camera::visibility::RenderLayers,
    gltf::{Gltf, GltfMesh},
    prelude::*,
};

use crate::assets::{
    material::{
        material_asset_types::MaterialAsset, runtime::effect_pass_gpu_data::EffectPassMaterial,
    },
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
};
use crate::plugins::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use crate::plugins::settings::graphics_settings_types::GraphicsSettings;

use super::{
    terrain_biome_detail_types::TerrainDetailProjectionState,
    terrain_chunk_presentation_types::{
        TerrainChunkRejected, TerrainRenderChunk, TerrainRenderRevision, TerrainSurfaceImage,
        TERRAIN_RENDER_LAYER,
    },
    terrain_chunk_types::{TerrainChunk, TerrainIndex},
    terrain_collision_rebuilding_types::{TerrainCollisionPending, TerrainCollisionRevision},
    terrain_surface_image_composition::compose_terrain_surface_image,
};

#[derive(Component)]
pub(super) struct AdditionalAuthoredTerrainEffectPass;

pub(super) fn validate_index_and_project_loaded_terrain_chunks(
    mut commands: Commands,
    terrain_assets: Res<Assets<TerrainAsset>>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut images: ResMut<Assets<Image>>,
    authored_materials: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut terrain_index: ResMut<TerrainIndex>,
    chunks: Query<
        (Entity, &TerrainChunk),
        (Without<TerrainRenderChunk>, Without<TerrainChunkRejected>),
    >,
) {
    for (entity, chunk) in &chunks {
        let Some(asset) = terrain_assets.get(&chunk.asset) else {
            continue;
        };
        let Some(model) = gltfs.get(asset.lowered_terrain_model()) else {
            continue;
        };
        let Some(terrain_material) = authored_materials.get(asset.terrain_effect_material()) else {
            continue;
        };
        let Some(terrain_effect_passes) = terrain_material
            .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
            .iter()
            .map(|handle| effect_pass_materials.get(handle).cloned())
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let Some(chunk_cells) = asset.terrain_chunk_cells_per_side() else {
            commands.entity(entity).insert(TerrainChunkRejected);
            continue;
        };
        let expected_side = chunk_cells.saturating_add(1);
        let expected_spacing = asset.canonical_terrain_grid().units.cell_size_metres;
        let valid_coord = chunk.source_coord.cmpge(IVec2::ZERO).all()
            && chunk.source_coord.x < asset.canonical_terrain_grid().sector_columns as i32
            && chunk.source_coord.y < asset.canonical_terrain_grid().sector_rows as i32
            && chunk.coord == IVec2::new(chunk.source_coord.x, -chunk.source_coord.y - 1);
        if !valid_coord
            || chunk.side != expected_side
            || chunk.side < 2
            || !chunk.spacing_m.is_finite()
            || (chunk.spacing_m - expected_spacing).abs() > f32::EPSILON
        {
            commands.entity(entity).insert(TerrainChunkRejected);
            continue;
        }
        let span = expected_spacing * f32::from(chunk_cells);
        if terrain_index.chunk_span_m == 0.0 {
            terrain_index.chunk_span_m = span;
            terrain_index.origin = chunk.origin - chunk.coord.as_vec2() * span;
        } else if (terrain_index.chunk_span_m - span).abs() > f32::EPSILON
            || (chunk.origin - (terrain_index.origin + chunk.coord.as_vec2() * span))
                .length_squared()
                > f32::EPSILON
        {
            commands.entity(entity).insert(TerrainChunkRejected);
            continue;
        }
        let mesh_name = format!(
            "terrain_chunk_{}_{}",
            chunk.source_coord.x, chunk.source_coord.y
        );
        let Some(mesh) = model
            .named_meshes
            .get(mesh_name.as_str())
            .and_then(|handle| gltf_meshes.get(handle))
            .and_then(|mesh| mesh.primitives.first())
            .map(|primitive| primitive.mesh.clone())
        else {
            commands.entity(entity).insert(TerrainChunkRejected);
            continue;
        };
        let Some(surface) = compose_terrain_surface_image(chunk, asset, &[], &images) else {
            continue;
        };
        if let Entry::Occupied(_) = terrain_index.chunks.entry(chunk.coord) {
            commands.entity(entity).insert(TerrainChunkRejected);
            continue;
        }
        terrain_index.chunks.insert(chunk.coord, entity);
        let texture = images.add(surface);
        let Some(detail_texture) = asset
            .texture_image(&asset.canonical_terrain_grid().presentation.detail_texture)
            .cloned()
        else {
            commands.entity(entity).insert(TerrainChunkRejected);
            continue;
        };
        let Some(projected_terrain_effect_passes) = terrain_effect_passes
            .into_iter()
            .map(|mut pass| {
                (pass.replace_texture_asset_at_d3d9_stage(0, texture.clone())
                    && pass.replace_texture_asset_at_d3d9_stage(1, detail_texture.clone()))
                .then_some(pass)
            })
            .collect::<Option<Vec<_>>>()
        else {
            commands.entity(entity).insert(TerrainChunkRejected);
            continue;
        };
        let mut projected_materials = projected_terrain_effect_passes
            .into_iter()
            .map(|pass| effect_pass_materials.add(pass));
        let Some(material) = projected_materials.next() else {
            commands.entity(entity).insert(TerrainChunkRejected);
            continue;
        };
        commands.entity(entity).insert((
            TerrainRenderChunk,
            Mesh3d(mesh.clone()),
            MeshMaterial3d::<EffectPassMaterial>(material),
            TerrainSurfaceImage(texture),
            Transform::from_xyz(chunk.origin.x, 0.0, chunk.origin.y),
            Visibility::Inherited,
            RenderLayers::layer(TERRAIN_RENDER_LAYER),
            TerrainCollisionRevision(0),
            TerrainRenderRevision(0),
            TerrainDetailProjectionState::default(),
            TerrainCollisionPending,
            AuthoredEffectTechniquePassSubmissionOrder::new(entity, 0)
                .expect("the first authored terrain effect pass index fits u16"),
        ));
        for (pass_index, material) in projected_materials.enumerate() {
            commands.spawn((
                AdditionalAuthoredTerrainEffectPass,
                Mesh3d(mesh.clone()),
                MeshMaterial3d::<EffectPassMaterial>(material),
                Transform::IDENTITY,
                Visibility::Inherited,
                RenderLayers::layer(TERRAIN_RENDER_LAYER),
                ChildOf(entity),
                AuthoredEffectTechniquePassSubmissionOrder::new(entity, pass_index + 1)
                    .expect("the authored terrain effect pass index fits u16"),
            ));
        }
    }
}

pub(super) fn apply_changed_effect_quality_to_projected_terrain_chunks(
    mut commands: Commands,
    graphics_settings: Res<GraphicsSettings>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    authored_materials: Res<Assets<MaterialAsset>>,
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut chunks: Query<
        (
            Entity,
            &TerrainChunk,
            &TerrainSurfaceImage,
            &mut MeshMaterial3d<EffectPassMaterial>,
            &Mesh3d,
            Option<&Children>,
        ),
        With<TerrainRenderChunk>,
    >,
    additional_passes: Query<(), With<AdditionalAuthoredTerrainEffectPass>>,
) {
    if !graphics_settings.is_changed() {
        return;
    }
    for (entity, chunk, surface_image, mut material, mesh, children) in &mut chunks {
        let Some(terrain_asset) = terrain_assets.get(&chunk.asset) else {
            continue;
        };
        let Some(selected_passes) = authored_materials
            .get(terrain_asset.terrain_effect_material())
            .and_then(|terrain_material| {
                terrain_material
                    .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
                    .iter()
                    .map(|pass| effect_pass_materials.get(pass).cloned())
                    .collect::<Option<Vec<_>>>()
            })
        else {
            continue;
        };
        let Some(detail_texture) = terrain_asset
            .texture_image(
                &terrain_asset
                    .canonical_terrain_grid()
                    .presentation
                    .detail_texture,
            )
            .cloned()
        else {
            continue;
        };
        let Some(mut projected_materials) = selected_passes
            .into_iter()
            .map(|mut pass| {
                (pass.replace_texture_asset_at_d3d9_stage(0, surface_image.0.clone())
                    && pass.replace_texture_asset_at_d3d9_stage(1, detail_texture.clone()))
                .then(|| effect_pass_materials.add(pass))
            })
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let Some(first_material) = projected_materials.first().cloned() else {
            continue;
        };
        material.0 = first_material;
        for child in children.into_iter().flat_map(|children| children.iter()) {
            if additional_passes.contains(child) {
                commands.entity(child).despawn();
            }
        }
        for (pass_index, additional_material) in projected_materials.drain(1..).enumerate() {
            commands.spawn((
                AdditionalAuthoredTerrainEffectPass,
                mesh.clone(),
                MeshMaterial3d::<EffectPassMaterial>(additional_material),
                Transform::IDENTITY,
                Visibility::Inherited,
                RenderLayers::layer(TERRAIN_RENDER_LAYER),
                ChildOf(entity),
                AuthoredEffectTechniquePassSubmissionOrder::new(entity, pass_index + 1)
                    .expect("the authored terrain effect pass index fits u16"),
            ));
        }
    }
}
