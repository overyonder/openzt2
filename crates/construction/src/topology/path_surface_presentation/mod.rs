use super::path_presentation_neighborhood::{cells_for_region, terrain_region};
use super::{
    elevated_path_layout::elevated_path_tiles,
    path_support_hydration::lowest_ground_height_within_fit_radius,
    topology_construction_preview_types::{PathDrawPlan, PathPreview},
};
use bevy::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    platform::collections::HashMap,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;

use openzt2_zoo_state::topology::topology_graph_types::{PathTile, TileSurface, TopologyGrid};

#[derive(Resource, Default)]
pub(super) struct ElevatedPathSurfacePresentationAssets {
    elevated_meshes: HashMap<(u16, [i16; 5]), Handle<Mesh>>,
    materials: HashMap<AssetId, Handle<StandardMaterial>>,
    preview_materials: HashMap<bool, Handle<StandardMaterial>>,
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
    terrain_index: Single<&TerrainIndex>,
    mut paths: Query<(Entity, &PathTile, &mut Transform), Without<PathTerrainFitted>>,
) {
    let terrain_index = terrain_index.into_inner();
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
    paths: Query<&PathTile, With<PathTerrainFitted>>,
    grid: Single<&TopologyGrid>,
    index: Single<&openzt2_zoo_state::topology::topology_graph_types::TopologyIndex>,
) {
    let grid = grid.into_inner();
    let index = index.into_inner();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let reach = f32::from(definitions.maximum_ground_path_width_cm()) * 0.005;
    let mut affected = bevy::ecs::entity::EntityHashSet::new();
    for (min, max) in changed
        .read()
        .filter_map(|change| terrain_region(change, &terrain_chunks))
    {
        let (first, last) = cells_for_region(*grid, min, max, reach);
        affected.extend(index.paths_within(first, last).filter(|&entity| {
            let Ok(path) = paths.get(entity) else {
                return false;
            };
            let Some(definition) = definitions.find_path(path.definition) else {
                return false;
            };
            let point = grid.cell_translation(path.cell).xz();
            let half = Vec2::splat(f32::from(definition.width_cm) * 0.005);
            !definition.elevated && point.cmpge(min - half).all() && point.cmple(max + half).all()
        }));
    }
    for entity in affected {
        commands.entity(entity).remove::<PathTerrainFitted>();
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
    paths: Query<(Entity, &PathTile, &TileSurface), Without<ElevatedPathSurfacePresentation>>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, path, surface) in &paths {
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
        let mesh = presentation_assets
            .elevated_meshes
            .entry((definition.width_cm, surface.height_cm))
            .or_insert_with(|| {
                meshes.add(elevated_path_surface_mesh(definition.width_cm, *surface))
            })
            .clone();
        commands.entity(entity).with_child((
            Name::new("path surface"),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::IDENTITY,
        ));
        commands
            .entity(entity)
            .insert(ElevatedPathSurfacePresentation);
    }
}

#[derive(Component)]
pub(super) struct ElevatedPathPreviewSurface;

pub(super) fn project_elevated_path_preview(
    mut commands: Commands,
    (active, definitions): (Res<WorldDefinitions>, Res<Assets<WorldDefinitionAsset>>),
    (server, mut meshes, mut materials, mut presentation): (
        Res<AssetServer>,
        ResMut<Assets<Mesh>>,
        ResMut<Assets<StandardMaterial>>,
        ResMut<ElevatedPathSurfacePresentationAssets>,
    ),
    (grid, terrain): (Single<Ref<TopologyGrid>>, Single<Ref<TerrainIndex>>),
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut terrain_changes: MessageReader<TerrainChanged>,
    previews: Query<(
        Entity,
        Ref<PathPreview>,
        Ref<PathDrawPlan>,
        &Transform,
        Option<&Children>,
    )>,
    surfaces: Query<(), With<ElevatedPathPreviewSurface>>,
) {
    let refit = terrain_changes.read().count() != 0 || terrain.is_changed() || grid.is_changed();
    let source_changed = active.is_changed() || definitions.is_changed();
    let Some(definitions) = active.get(&definitions) else {
        return;
    };
    for (entity, preview, plan, transform, children) in &previews {
        let Some(definition) = definitions
            .find_path(preview.definition)
            .filter(|path| path.elevated)
        else {
            continue;
        };
        let existing = children
            .into_iter()
            .flat_map(|children| children.iter())
            .filter(|&child| surfaces.contains(child));
        if !refit
            && !source_changed
            && !preview.is_changed()
            && !plan.is_changed()
            && existing.clone().next().is_some()
        {
            continue;
        }
        let Some(tiles) = elevated_path_tiles(&preview, definition) else {
            continue;
        };
        for child in existing {
            commands.entity(child).try_despawn();
        }
        let valid = matches!(plan.validity, PlacementValidity::Valid { .. });
        let material = presentation
            .preview_materials
            .entry(valid)
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color_texture: Some(
                        server.load("entities/objects/paths/shared/elevated_path_highlight.tga"),
                    ),
                    base_color: if valid {
                        Color::WHITE
                    } else {
                        Color::srgb(1.0, 0.12, 0.12)
                    },
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                })
            })
            .clone();
        let direction = (preview.to - preview.from).xy().as_vec2();
        let rotation = Quat::from_rotation_y(direction.x.atan2(direction.y));
        for (cell, surface) in tiles {
            let position = grid.cell_translation(cell);
            let mesh = presentation
                .elevated_meshes
                .entry((definition.width_cm, surface.height_cm))
                .or_insert_with(|| {
                    meshes.add(elevated_path_surface_mesh(definition.width_cm, surface))
                })
                .clone();
            commands.spawn((
                ElevatedPathPreviewSurface,
                Mesh3d(mesh),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(position - transform.translation + Vec3::Y * 0.015),
                Visibility::Inherited,
                ChildOf(entity),
            ));
            let column = definition.support_column.as_ref();
            let Some(ground) = lowest_ground_height_within_fit_radius(
                position.xz(),
                column.map_or(0.0, |column| column.fit_lowest_radius_metres),
                &terrain,
                &terrain_chunks,
                &terrain_assets,
            ) else {
                continue;
            };
            let top = position.y + f32::from(surface.height_cm[4]) * 0.01;
            if top <= ground {
                continue;
            }
            let base = Transform::from_translation(
                Vec3::new(position.x, ground, position.z) - transform.translation,
            );
            let support = commands
                .spawn((
                    ElevatedPathPreviewSurface,
                    base,
                    Visibility::Inherited,
                    ChildOf(entity),
                ))
                .id();
            if let Some(column) = column {
                if !openzt2_world::world_spawn::expanding_column_presentation::spawn_expanding_column_presentation(
                    &mut commands, definitions, support, Transform::IDENTITY, top - ground, rotation, column,
                ) {
                    commands.entity(support).despawn();
                }
            } else if let Some(prefab) = definitions.scene(definition.support_prefab) {
                commands
                    .entity(support)
                    .insert(PrefabPresentation::new(prefab));
            } else {
                commands.entity(support).despawn();
            }
        }
    }
}

pub(super) fn elevated_path_surface_mesh(width_cm: u16, surface: TileSurface) -> Mesh {
    let half = f32::from(width_cm) * 0.005;
    let corners = [
        (-half, -half),
        (half, -half),
        (half, half),
        (-half, half),
        (0.0, 0.0),
    ];
    let positions = corners
        .into_iter()
        .zip(surface.height_cm)
        .map(|((x, z), height)| [x, f32::from(height) * 0.01, z])
        .collect::<Vec<_>>();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.5]],
    );
    mesh.insert_indices(Indices::U32(vec![0, 4, 1, 1, 4, 2, 2, 4, 3, 3, 4, 0]));
    mesh.compute_normals();
    mesh
}
