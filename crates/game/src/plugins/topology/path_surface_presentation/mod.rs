use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::AssetId;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::terrain::terrain_edit_types::TerrainChanged;

use super::topology_graph_types::PathTile;

#[derive(Resource, Default)]
pub(super) struct ElevatedPathSurfacePresentationAssets {
    elevated_mesh: Option<Handle<Mesh>>,
    materials: HashMap<AssetId, Handle<StandardMaterial>>,
}

#[derive(Component)]
pub(super) struct PathTerrainFitted;

/// An elevated path tile's mesh. Ground paths are drawn into terrain images.
#[derive(Component)]
pub(super) struct ElevatedPathSurfacePresentation;

/// Sets ground-path transforms to the terrain height without changing their cells.
pub(super) fn fit_ground_path_tiles_to_authored_terrain(
    mut commands: Commands,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    terrain_index: Res<TerrainIndex>,
    mut paths: Query<(Entity, &PathTile, &mut Transform), Without<PathTerrainFitted>>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, path, mut transform) in &mut paths {
        let Some(definition) = definitions.find_path(path.definition) else {
            continue;
        };
        if definition.elevated {
            commands.entity(entity).insert(PathTerrainFitted);
            continue;
        }
        let world_xz = transform.translation.xz();
        let Some(height_m) = terrain_chunk_at(&terrain_index, world_xz)
            .and_then(|chunk| terrain_chunks.get(chunk).ok())
            .and_then(|(chunk, edited)| {
                let asset = terrain_assets.get(&chunk.asset)?;
                sample_terrain(chunk, asset, edited, world_xz).map(|point| point.height_m)
            })
        else {
            continue;
        };
        transform.translation.y = height_m + 0.015;
        commands.entity(entity).insert(PathTerrainFitted);
    }
}

/// Marks ground paths intersecting an edit for height resampling this frame.
pub(super) fn invalidate_ground_path_terrain_fitting_after_terrain_changes(
    mut commands: Commands,
    mut changed: MessageReader<TerrainChanged>,
    terrain_chunks: Query<&TerrainChunk>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    paths: Query<(Entity, &PathTile, &Transform), With<PathTerrainFitted>>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let changed_regions = changed
        .read()
        .filter_map(|change| {
            let chunk = terrain_chunks.get(change.chunk).ok()?;
            Some((
                chunk.origin + change.min.as_vec2() * chunk.spacing_m,
                chunk.origin + change.max.as_vec2() * chunk.spacing_m,
            ))
        })
        .collect::<Vec<_>>();
    if changed_regions.is_empty() {
        return;
    }
    for (entity, path, transform) in &paths {
        let Some(half_width_m) = (|| {
            let definition = definitions.find_path(path.definition)?;
            (!definition.elevated).then(|| f32::from(definition.width_cm) * 0.005)
        })() else {
            continue;
        };
        let centre = transform.translation.xz();
        if changed_regions.iter().any(|(min, max)| {
            let extent = Vec2::splat(half_width_m);
            centre.cmpge(*min - extent).all() && centre.cmple(*max + extent).all()
        }) {
            commands.entity(entity).remove::<PathTerrainFitted>();
        }
    }
}

/// Builds elevated path meshes. Ground paths are drawn into terrain chunk images.
pub(super) fn hydrate_elevated_path_surface_mesh_and_material_presentations(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut presentation_assets: ResMut<ElevatedPathSurfacePresentationAssets>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    paths: Query<(Entity, &PathTile), Without<ElevatedPathSurfacePresentation>>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, path) in &paths {
        let Some((definition, image)) = (|| {
            let definition = definitions.find_path(path.definition)?;
            if !definition.elevated {
                return None;
            }
            let texture = AssetId(definition.surface_texture.0);
            definitions
                .texture_image(texture)
                .map(|image| (definition, image))
        })() else {
            continue;
        };
        let texture = AssetId(definition.surface_texture.0);
        let material = presentation_assets
            .materials
            .entry(texture)
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color_texture: Some(image),
                    alpha_mode: AlphaMode::Blend,
                    perceptual_roughness: 1.0,
                    reflectance: 0.0,
                    ..default()
                })
            })
            .clone();
        let width = f32::from(definition.width_cm) * 0.01;
        let mesh = presentation_assets
            .elevated_mesh
            .get_or_insert_with(|| meshes.add(Plane3d::default().mesh().size(1.0, 1.0)))
            .clone();
        commands.entity(entity).with_child((
            Name::new("path surface"),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_scale(Vec3::new(width, 1.0, width)),
        ));
        commands
            .entity(entity)
            .insert(ElevatedPathSurfacePresentation);
    }
}
