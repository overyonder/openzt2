use bevy::{camera::visibility::RenderLayers, prelude::*};
use openzt2_game_data::terrain::TerrainWaterDepth;

use crate::{
    assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    plugins::construction::construction_tool_and_placement_policy_types::{
        BiomeSurface, ConstructionPlacementPolicy,
    },
};

use super::{
    terrain_brush_types::{TerrainBrushKind, TerrainBrushPreview},
    terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER,
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
    terrain_fitted_surface_mesh_construction::construct_square_surface_mesh_fitted_to_canonical_terrain_samples,
};

/// Fits the active terrain tool's cursor mesh to the terrain surface.
#[allow(clippy::too_many_arguments)]
pub(super) fn project_terrain_brush_cursor_onto_terrain_surfaces(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    terrain_index: Res<TerrainIndex>,
    construction_policy: Res<ConstructionPlacementPolicy>,
    mut previews: Query<
        (
            Entity,
            &TerrainBrushPreview,
            &mut Transform,
            Option<&Mesh3d>,
            Option<&MeshMaterial3d<StandardMaterial>>,
        ),
        Or<(Changed<TerrainBrushPreview>, Without<Mesh3d>)>,
    >,
) {
    for (entity, preview, mut transform, mesh, material) in &mut previews {
        let Some(cursor_texture_path) =
            terrain_brush_cursor_texture_path(preview.kind, construction_policy.biome_surface)
        else {
            if let Some(mesh) = mesh {
                meshes.remove(&mesh.0);
            }
            if let Some(material) = material {
                materials.remove(&material.0);
                commands
                    .entity(entity)
                    .remove::<(Mesh3d, MeshMaterial3d<StandardMaterial>, RenderLayers)>();
            }
            continue;
        };
        let footprint_diameter_metres = preview.radius_m * 2.0;
        let fitted_mesh = construct_square_surface_mesh_fitted_to_canonical_terrain_samples(
            footprint_diameter_metres,
            transform.translation,
            &terrain_index,
            &terrain_assets,
            &terrain_chunks,
        );
        let Some(fitted_mesh) = fitted_mesh else {
            continue;
        };
        let cursor_texture = asset_server.load(cursor_texture_path);
        if let Some(material) = material {
            if let Some(mut material) = materials.get_mut(&material.0) {
                material.base_color_texture = Some(cursor_texture.clone());
            }
        }
        if let Some(mesh) = mesh {
            if let Some(mut mesh) = meshes.get_mut(&mesh.0) {
                *mesh = fitted_mesh;
            }
        } else {
            commands.entity(entity).insert((
                Mesh3d(meshes.add(fitted_mesh)),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color_texture: Some(cursor_texture),
                    alpha_mode: AlphaMode::Blend,
                    depth_bias: 1.0,
                    unlit: true,
                    ..default()
                })),
                RenderLayers::layer(TERRAIN_RENDER_LAYER),
            ));
        }
        transform.rotation = Quat::IDENTITY;
        transform.scale = Vec3::ONE;
    }
}

fn terrain_brush_cursor_texture_path(
    kind: TerrainBrushKind,
    biome_surface: BiomeSurface,
) -> Option<&'static str> {
    match kind {
        TerrainBrushKind::Raise => Some("UI/cursor_texture/hill.dds"),
        TerrainBrushKind::Lower => Some("UI/cursor_texture/valley.dds"),
        TerrainBrushKind::Smooth => Some("UI/cursor_texture/smooth.dds"),
        TerrainBrushKind::Flatten { .. } => Some("UI/cursor_texture/flatten.dds"),
        TerrainBrushKind::PaintWater { depth, .. } => match depth {
            TerrainWaterDepth::Deep => Some("UI/cursor_texture/deepwater.dds"),
            TerrainWaterDepth::Shallow => Some("UI/cursor_texture/shallowwater.dds"),
            TerrainWaterDepth::Dry => None,
        },
        TerrainBrushKind::Paint { .. } => match biome_surface {
            BiomeSurface::Ground => Some("UI/cursor_texture/ground.dds"),
            BiomeSurface::MixedGround => Some("UI/cursor_texture/mix.dds"),
            BiomeSurface::GroundCover => Some("UI/cursor_texture/ground-cover.dds"),
            BiomeSurface::FoliageMix => Some("UI/cursor_texture/sparse.dds"),
            BiomeSurface::DeepWater | BiomeSurface::ShallowWater => None,
        },
        TerrainBrushKind::AddWater | TerrainBrushKind::RemoveWater => None,
    }
}
