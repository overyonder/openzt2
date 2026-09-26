use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use crate::assets::material::material_asset_types::MaterialAsset;
use crate::assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_interaction_types::ConstructionPreview;
use crate::plugins::settings::graphics_settings_types::GraphicsSettings;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::terrain::terrain_fitted_surface_mesh_construction::construct_square_surface_mesh_fitted_to_canonical_terrain_samples_with_rotated_texture_coordinates;
use super::transport_track_construction_types::TransportTrackConstructionPreview;
use super::transport_track_construction_types::TransportTrackConstructionPreviewAuthoredPrefab;

pub(super) fn project_transport_track_construction_preview_authored_prefabs(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    authored_materials: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    previews: Query<(
        Entity,
        Ref<ConstructionPreview>,
        Ref<TransportTrackConstructionPreview>,
    )>,
    projected_prefabs: Query<(Entity, &TransportTrackConstructionPreviewAuthoredPrefab)>,
) {
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    let terrain_decal_material = asset_server.load::<MaterialAsset>(
        super::transport_track_piece_presentation::AUTHORED_TERRAIN_DECAL_MATERIAL_PATH,
    );
    let terrain_decal_pass = authored_materials
        .get(&terrain_decal_material)
        .and_then(|material| {
            material
                .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
                .first()
        })
        .and_then(|material| effect_pass_materials.get(material))
        .cloned();
    for (preview_entity, preview, track_preview) in &previews {
        if !preview.is_changed() && !track_preview.is_changed() {
            continue;
        }
        for (entity, projected) in &projected_prefabs {
            if projected.owner == preview_entity {
                commands.entity(entity).despawn();
            }
        }
        let Some(track) = definitions.find_track(preview.definition) else {
            continue;
        };
        if track.kind == TransportationTrackKind::Sky {
            let Some(presentation) = track.sky_tower_presentation.as_ref() else {
                continue;
            };
            if track_preview.path_points.len() != 2 {
                continue;
            }
            let root = commands
                .spawn((
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    ChildOf(preview_entity),
                    TransportTrackConstructionPreviewAuthoredPrefab {
                        owner: preview_entity,
                    },
                ))
                .id();
            super::transport_track_piece_presentation::spawn_authored_sky_tower(
                &mut commands,
                definitions,
                root,
                &preview.transform,
                &track_preview.path_points,
                1,
                1,
                presentation.initial_height_metres.max(
                    track_preview.path_points[0].y - track_preview.path_points[1].y
                        + track.sky_rope_clearance_metres.unwrap_or_default(),
                ),
                presentation,
            );
            continue;
        }
        let Some(template) = terrain_decal_pass.as_ref() else {
            continue;
        };
        for (piece_index, points) in track_preview.path_points.windows(2).enumerate() {
            let world_transform =
                super::transport_track_construction_transaction::transport_track_piece_transform(
                    points[0], points[1],
                );
            let Some(presentation) = super::transport_track_piece_presentation::
                select_authored_ground_track_piece_presentation(
                    &track.ground_piece_presentations,
                    &track_preview.path_points,
                    piece_index,
                )
            else {
                continue;
            };
            let elevated = super::transport_track_piece_presentation::ground_track_piece_is_elevated_above_terrain(
                [points[0], points[1]],
                &terrain_index,
                &terrain_assets,
                &terrain_chunks,
            );
            let texture_identifier = if elevated {
                presentation.elevated_path_texture
            } else {
                presentation.ground_decal_texture
            };
            let Some(texture) = definitions.texture_image(texture_identifier) else {
                continue;
            };
            let mut material = template.clone();
            let material_configured = if elevated {
                material.configure_for_authored_blue_fang_line(texture)
            } else {
                material.configure_for_authored_single_texture_terrain_decal(texture)
            };
            if !material_configured {
                continue;
            }
            material.recreate_uniform_buffers_for_runtime_material(&mut effect_uniform_buffers);
            if elevated {
                let Some(mesh) = super::transport_track_piece_presentation::construct_elevated_ground_track_triangle_strip(
                    points,
                    presentation.ground_decal_size_metres[0],
                    presentation.elevated_path_rotation_radians,
                    preview.transform.translation,
                ) else { continue; };
                commands.spawn((
                    Transform::from_rotation(preview.transform.rotation.inverse()),
                    Visibility::Inherited,
                    ChildOf(preview_entity),
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(effect_pass_materials.add(material)),
                    TransportTrackConstructionPreviewAuthoredPrefab {
                        owner: preview_entity,
                    },
                ));
                continue;
            }
            let decal_centre = world_transform.transform_point(Vec3::new(
                presentation.ground_decal_offset_metres[0],
                0.0,
                presentation.ground_decal_offset_metres[1],
            ));
            let Some(mesh) = construct_square_surface_mesh_fitted_to_canonical_terrain_samples_with_rotated_texture_coordinates(
                presentation.ground_decal_size_metres[0],
                decal_centre,
                presentation.ground_decal_rotation_radians,
                &terrain_index,
                &terrain_assets,
                &terrain_chunks,
            ) else { continue; };
            commands.spawn((
                Transform::from_translation(
                    preview
                        .transform
                        .compute_affine()
                        .inverse()
                        .transform_point3(decal_centre),
                )
                .with_rotation(preview.transform.rotation.inverse()),
                Visibility::Inherited,
                ChildOf(preview_entity),
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(effect_pass_materials.add(material)),
                TransportTrackConstructionPreviewAuthoredPrefab {
                    owner: preview_entity,
                },
            ));
        }
    }
}
