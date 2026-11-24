use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy;

use openzt2_zoo_state::terrain::terrain_brush_types::TerrainBrushPreview;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_fitted_surface_mesh_construction::construct_square_surface_mesh_fitted_to_canonical_terrain_samples;

use super::authored_terrain_brush_modes::AuthoredTerrainBrushModes;
use super::terrain_change_tracking_types::{TerrainDirtyFlags, TerrainRenderPending};

#[derive(Component, Clone, PartialEq)]
pub(super) struct TerrainBrushCursorShape {
    radius_metres: f32,
    texture: Option<Handle<Image>>,
}

pub(super) fn resolve_terrain_brush_cursor_shape(
    mut commands: Commands,
    policy: Res<ConstructionPlacementPolicy>,
    modes: AuthoredTerrainBrushModes,
    mut previews: Query<(
        Entity,
        Ref<TerrainBrushPreview>,
        Option<&mut TerrainBrushCursorShape>,
    )>,
) {
    for (entity, preview, shape) in &mut previews {
        if shape.is_some() && !preview.is_changed() && !policy.is_changed() && !modes.is_changed() {
            continue;
        }
        let next = TerrainBrushCursorShape {
            radius_metres: preview.radius_m,
            texture: modes.cursor(preview.kind, &policy).cloned(),
        };
        if let Some(mut shape) = shape {
            shape.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

/// Fits the active terrain tool's cursor mesh to the terrain surface.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn project_terrain_brush_cursor_onto_terrain_surfaces(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    terrain_index: Single<&TerrainIndex>,
    dirty_chunks: Query<(&TerrainChunk, &TerrainRenderPending), Changed<TerrainRenderPending>>,
    previews: Query<(
        Entity,
        Ref<TerrainBrushCursorShape>,
        Ref<Transform>,
        Option<&Mesh3d>,
        Option<&MeshMaterial3d<StandardMaterial>>,
    )>,
) {
    let terrain_index = terrain_index.into_inner();
    let changed_height_bounds: Vec<_> = dirty_chunks
        .iter()
        .filter_map(|(chunk, pending)| {
            pending
                .0
                .flags
                .contains(TerrainDirtyFlags::HEIGHT)
                .then(|| {
                    let min =
                        chunk.origin + (pending.0.min.as_vec2() - Vec2::ONE) * chunk.spacing_m;
                    let max =
                        chunk.origin + (pending.0.max.as_vec2() + Vec2::ONE) * chunk.spacing_m;
                    (min, max)
                })
        })
        .collect();
    for (entity, shape, transform, mesh, material) in &previews {
        let Some(cursor_texture) = shape.texture.as_ref() else {
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
        let center = transform.translation.xz();
        let radius = Vec2::splat(shape.radius_metres);
        if mesh.is_some()
            && !shape.is_changed()
            && !transform.is_changed()
            && !changed_height_bounds.iter().any(|(min, max)| {
                min.cmple(center + radius).all() && max.cmpge(center - radius).all()
            })
        {
            continue;
        }
        let fitted_mesh = construct_square_surface_mesh_fitted_to_canonical_terrain_samples(
            shape.radius_metres * 2.0,
            transform.translation,
            &terrain_index,
            &terrain_assets,
            &terrain_chunks,
        );
        let Some(fitted_mesh) = fitted_mesh else {
            continue;
        };
        if let Some(material) = material {
            if let Some(mut material) = materials.get_mut(&material.0) {
                if material.base_color_texture.as_ref() != Some(cursor_texture) {
                    material.base_color_texture = Some(cursor_texture.clone());
                }
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
                    base_color_texture: Some(cursor_texture.clone()),
                    alpha_mode: AlphaMode::Blend,
                    depth_bias: 1.0,
                    unlit: true,
                    ..default()
                })),
                RenderLayers::layer(TERRAIN_RENDER_LAYER),
            ));
        }
    }
}
