use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_fitted_surface_mesh_construction::construct_square_surface_mesh_fitted_to_canonical_terrain_samples_with_rotated_texture_coordinates;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionPreview;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionPreviewAuthoredPrefab;

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
        Option<
            &openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewMaterials,
        >,
    )>,
    projected_prefabs: Query<(Entity, &TransportTrackConstructionPreviewAuthoredPrefab)>,
    rope_anchors: crate::transport_tours::transport_track_piece_presentation::SkyRopeAnchors,
) {
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    let terrain_decal_material = asset_server.load::<MaterialAsset>(
        crate::transport_tours::transport_track_piece_presentation::AUTHORED_TERRAIN_DECAL_MATERIAL_PATH,
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
    for (preview_entity, preview, track_preview, grid) in &previews {
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
            crate::transport_tours::transport_track_piece_presentation::spawn_authored_sky_tower(
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
            // The tower's footprint and the placement grid, as for objects.
            if let Some(materials) = grid {
                let valid = matches!(
                    preview.validity,
                    openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity::Valid { .. }
                );
                let base = preview
                    .transform
                    .compute_affine()
                    .inverse()
                    .transform_point3(track_preview.path_points[1]);
                let plane = meshes.add(Plane3d::default().mesh().size(1.0, 1.0));
                let footprint = if valid {
                    &materials.footprint_valid
                } else {
                    &materials.footprint_invalid
                };
                commands.spawn((
                    Mesh3d(plane.clone()),
                    MeshMaterial3d(footprint.clone()),
                    Transform::from_translation(base + Vec3::Y * 0.025)
                        .with_rotation(preview.transform.rotation.inverse()),
                    ChildOf(root),
                ));
                commands.spawn((
                    Mesh3d(plane),
                    MeshMaterial3d(materials.grid_cardinal[0].clone()),
                    Transform::from_translation(base + Vec3::Y * 0.015)
                        .with_rotation(preview.transform.rotation.inverse())
                        .with_scale(Vec3::new(
                            materials.grid_radius[0] * 2.0,
                            1.0,
                            materials.grid_radius[0] * 2.0,
                        )),
                    ChildOf(root),
                ));
            }
            // As in the original, a line runs from the last tower top (or
            // station wheel) to the top of the tower being placed.
            let clearance = track.sky_rope_clearance_metres.unwrap_or_default();
            let height = presentation
                .initial_height_metres
                .max(track_preview.path_points[0].y - track_preview.path_points[1].y + clearance);
            let start = rope_anchors
                .station_wheel(track_preview.from_endpoint, track_preview.path_points[0])
                .unwrap_or(
                    track_preview.path_points[0] + Vec3::Y * presentation.initial_height_metres,
                );
            let end = track_preview
                .to_station
                .and_then(|station| {
                    rope_anchors.station_wheel(station, track_preview.path_points[1])
                })
                .unwrap_or(track_preview.path_points[1] + Vec3::Y * height);
            if let Some(mut line) = terrain_decal_pass.clone() {
                let texture = definitions
                    .texture_image(presentation.rope_texture)
                    .unwrap_or_default();
                if line.configure_for_authored_blue_fang_line(texture) {
                    line.recreate_uniform_buffers_for_runtime_material(&mut effect_uniform_buffers);
                    crate::transport_tours::transport_track_piece_presentation::spawn_authored_sky_track_rope(
                        &mut commands,
                        &mut meshes,
                        effect_pass_materials.add(line),
                        root,
                        &preview.transform,
                        start,
                        end,
                        presentation,
                    );
                }
            }
            continue;
        }
        let Some(template) = terrain_decal_pass.as_ref() else {
            continue;
        };
        let tint = invalid_track_preview_tint(preview.validity);
        for (piece_index, points) in track_preview.path_points.windows(2).enumerate() {
            let world_transform =
                crate::transport_tours::transport_track_construction_transaction::transport_track_piece_transform(
                    points[0], points[1],
                );
            let Some(presentation) = crate::transport_tours::transport_track_piece_presentation::
                select_authored_ground_track_piece_presentation(
                    &track.ground_piece_presentations,
                    &track_preview.path_points,
                    piece_index,
                )
            else {
                continue;
            };
            let elevated = crate::transport_tours::transport_track_piece_presentation::ground_track_piece_is_elevated_above_terrain(
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
            if let Some(tinted) = tint.and_then(|tint| {
                material.clone_tinted(Vec4::from_array(tint), &mut effect_uniform_buffers)
            }) {
                material = tinted;
            }
            if elevated {
                let Some(mesh) = crate::transport_tours::transport_track_piece_presentation::construct_elevated_ground_track_triangle_strip(
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

/// `ZTGroundTrackMode` in `UI/modes/modes.xml` colours a road that can't be
/// placed `place_no` (200, 20, 10, 90) and one that is too steep `too_steep`
/// (255, 255, 0, 255). The road decal ignores vertex colour, so the preview
/// draws with a copy of its material lit only by that colour, which
/// multiplies the road texture; it stays as opaque as the road.
// ponytail: the two colours are copied from modes.xml, not lowered from it.
fn invalid_track_preview_tint(
    validity: openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity,
) -> Option<[f32; 4]> {
    use openzt2_zoo_state::construction::construction_interaction_types::{
        PlacementFailure, PlacementValidity,
    };
    match validity {
        PlacementValidity::Valid { .. } | PlacementValidity::Pending => None,
        PlacementValidity::Invalid(PlacementFailure::TooSteep) => Some([1.0, 1.0, 0.0, 1.0]),
        PlacementValidity::Invalid(_) => Some([200.0 / 255.0, 20.0 / 255.0, 10.0 / 255.0, 1.0]),
    }
}
