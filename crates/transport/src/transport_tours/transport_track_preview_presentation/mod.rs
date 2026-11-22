use openzt2_zoo_state::transport_tours::transport_highlight_style::TransportHighlightStyle;
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
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewMaterials;

pub(super) fn project_transport_track_construction_preview_authored_prefabs(
    mut commands: Commands,
    (style, materials): (
        Res<TransportHighlightStyle>,
        Res<super::transport_track_piece_presentation::TransportTrackMaterials>,
    ),
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    authored_materials: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain: PreviewTerrain,
    previews: Query<(
        Entity,
        Ref<ConstructionPreview>,
        Ref<TransportTrackConstructionPreview>,
        Option<Ref<ObjectPlacementPreviewMaterials>>,
        Option<&Children>,
    )>,
    projected_prefabs: Query<(), With<TransportTrackConstructionPreviewAuthoredPrefab>>,
    mut material_events: MessageReader<AssetEvent<MaterialAsset>>,
    mut effect_events: MessageReader<AssetEvent<EffectPassMaterial>>,
    (rope_anchors, mut standard_materials): (
        crate::transport_tours::transport_track_piece_presentation::SkyRopeAnchors,
        ResMut<Assets<StandardMaterial>>,
    ),
) {
    let definitions_changed = active_definitions.is_changed()
        || definition_assets.is_changed()
        || graphics_settings.is_changed();
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    let terrain_decal_material = &materials.terrain_decal;
    let material_changed = material_events
        .read()
        .filter(|event| {
            event.is_added(terrain_decal_material.id())
                || event.is_modified(terrain_decal_material.id())
                || event.is_loaded_with_dependencies(terrain_decal_material.id())
        })
        .count()
        != 0;
    let pass_handle = authored_materials
        .get(terrain_decal_material)
        .and_then(|material| {
            material
                .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
                .first()
        });
    let pass_changed = effect_events
        .read()
        .filter(|event| {
            pass_handle.is_some_and(|pass| {
                event.is_added(pass.id())
                    || event.is_modified(pass.id())
                    || event.is_loaded_with_dependencies(pass.id())
            })
        })
        .count()
        != 0;
    let terrain_decal_pass = pass_handle
        .and_then(|material| effect_pass_materials.get(material))
        .cloned();
    for (preview_entity, preview, track_preview, grid, children) in &previews {
        if !preview.is_changed()
            && !track_preview.is_changed()
            && !grid.as_ref().is_some_and(|grid| grid.is_changed())
            && !material_changed
            && !pass_changed
            && !definitions_changed
        {
            continue;
        }
        if let Some(children) = children {
            for child in children
                .iter()
                .filter(|child| projected_prefabs.contains(*child))
            {
                commands.entity(child).despawn();
            }
        }
        let Some(track) = definitions.find_track(preview.definition) else {
            continue;
        };
        if track.kind == TransportationTrackKind::Sky {
            spawn_sky_track_preview(
                &mut commands,
                definitions,
                track,
                preview_entity,
                &preview,
                &track_preview,
                grid.as_deref(),
                terrain_decal_pass.as_ref(),
                &mut SkyPreviewAssets {
                    meshes: &mut meshes,
                    effect_pass_materials: &mut effect_pass_materials,
                    effect_uniform_buffers: &mut effect_uniform_buffers,
                    standard_materials: &mut standard_materials,
                },
                &terrain,
                &rope_anchors,
            );
            continue;
        }
        let Some(template) = terrain_decal_pass.as_ref() else {
            continue;
        };
        let tint = invalid_track_preview_tint(preview.validity, &style);
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
                &terrain.index,
                &terrain.assets,
                &terrain.chunks,
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
                    TransportTrackConstructionPreviewAuthoredPrefab,
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
                &terrain.index,
                &terrain.assets,
                &terrain.chunks,
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
                Mesh3d(meshes.add(
                    crate::transport_tours::transport_track_piece_presentation::mirror_texture_rows(
                        mesh,
                    ),
                )),
                MeshMaterial3d(effect_pass_materials.add(material)),
                TransportTrackConstructionPreviewAuthoredPrefab,
            ));
        }
    }
}

fn invalid_track_preview_tint(
    validity: openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity,
    style: &TransportHighlightStyle,
) -> Option<[f32; 4]> {
    use openzt2_zoo_state::construction::construction_interaction_types::{
        PlacementFailure, PlacementValidity,
    };
    let color = match validity {
        PlacementValidity::Valid { .. } | PlacementValidity::Pending => return None,
        PlacementValidity::Invalid(PlacementFailure::TooSteep) => style.steep,
        PlacementValidity::Invalid(_) => style.invalid,
    };
    Some(color.to_srgba().to_vec4().to_array())
}

/// The assets a sky tower preview adds.
struct SkyPreviewAssets<'a> {
    meshes: &'a mut Assets<Mesh>,
    effect_pass_materials: &'a mut Assets<EffectPassMaterial>,
    effect_uniform_buffers: &'a mut Assets<ShaderBuffer>,
    standard_materials: &'a mut Assets<StandardMaterial>,
}

/// The ground a preview's decals are fitted to.
#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct PreviewTerrain<'w, 's> {
    index: Res<'w, TerrainIndex>,
    assets: Res<'w, Assets<TerrainAsset>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
}

/// The tower being placed, its footprint and grid, and the line to its top.
#[allow(clippy::too_many_arguments)]
fn spawn_sky_track_preview(
    commands: &mut Commands,
    definitions: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    track: &openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackDefinition,
    preview_entity: Entity,
    preview: &ConstructionPreview,
    track_preview: &TransportTrackConstructionPreview,
    grid: Option<
        &openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewMaterials,
    >,
    terrain_decal_pass: Option<&EffectPassMaterial>,
    assets: &mut SkyPreviewAssets,
    terrain: &PreviewTerrain,
    rope_anchors: &crate::transport_tours::transport_track_piece_presentation::SkyRopeAnchors,
) {
    let Some(presentation) = track.sky_tower_presentation.as_ref() else {
        return;
    };
    if track_preview.path_points.len() != 2 {
        return;
    }
    let root = commands
        .spawn((
            Transform::IDENTITY,
            Visibility::Inherited,
            ChildOf(preview_entity),
            TransportTrackConstructionPreviewAuthoredPrefab,
        ))
        .id();
    crate::transport_tours::transport_track_piece_presentation::spawn_authored_sky_tower(
        commands,
        definitions,
        root,
        &preview.transform,
        &track_preview.path_points,
        1,
        1,
        track_preview
            .tower_height_m
            .unwrap_or(presentation.initial_height_metres),
        presentation,
    );
    // The tower's footprint and the placement grid, as for objects.
    if let Some(materials) = grid {
        spawn_sky_tower_placement_grid(
            commands,
            root,
            preview,
            track_preview.path_points[1],
            materials,
            assets,
            terrain,
        );
    }
    // As in the original, a line runs from the last tower top (or
    // station wheel) to the top of the tower being placed.
    let height = track_preview
        .tower_height_m
        .unwrap_or(presentation.initial_height_metres);
    let start = rope_anchors
        .station_wheel(track_preview.from_endpoint, track_preview.path_points[0])
        .or_else(|| rope_anchors.tower_top(track_preview.from_endpoint))
        .unwrap_or(track_preview.path_points[0] + Vec3::Y * presentation.initial_height_metres);
    let end = track_preview
        .to_station
        .and_then(|station| rope_anchors.station_wheel(station, track_preview.path_points[1]))
        .unwrap_or(track_preview.path_points[1] + Vec3::Y * height);
    if let Some(mut line) = terrain_decal_pass.cloned() {
        let texture = definitions
            .texture_image(presentation.rope_texture)
            .unwrap_or_default();
        if line.configure_for_authored_blue_fang_line(texture) {
            line.recreate_uniform_buffers_for_runtime_material(assets.effect_uniform_buffers);
            crate::transport_tours::transport_track_piece_presentation::spawn_authored_sky_track_rope(
                    commands,
                    assets.meshes,
                    assets.effect_pass_materials.add(line),
                    root,
                    &preview.transform,
                    start,
                    end,
                    presentation,
                );
        }
    }
}

/// The footprint and grid under a sky tower being placed.
fn spawn_sky_tower_placement_grid(
    commands: &mut Commands,
    root: Entity,
    preview: &ConstructionPreview,
    tower_base_world: Vec3,
    materials: &openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewMaterials,
    assets: &mut SkyPreviewAssets,
    terrain: &PreviewTerrain,
) {
    let valid = matches!(
        preview.validity,
        openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity::Valid { .. }
    );
    let base = preview
        .transform
        .compute_affine()
        .inverse()
        .transform_point3(tower_base_world);
    // Fitted to the terrain, as road decals are, so a slope
    // doesn't bury them.
    let tower_base = tower_base_world;
    let rotation = preview.transform.rotation.inverse();
    let footprint = if valid {
        &materials.footprint_valid
    } else {
        &materials.footprint_invalid
    };
    for (width, material, lift) in [
        (1.0, footprint, 0.02),
        (
            materials.grid_radius[1] * 2.0,
            &materials.grid_cardinal[1],
            0.01,
        ),
    ] {
        let Some(mesh) = construct_square_surface_mesh_fitted_to_canonical_terrain_samples_with_rotated_texture_coordinates(
                width,
                tower_base,
                0.0,
                &terrain.index,
                &terrain.assets,
                &terrain.chunks,
            ) else {
                return;
            };
        // The fitted mesh may wind either way; draw both sides.
        let Some(source) = assets.standard_materials.get(material).cloned() else {
            return;
        };
        let material = assets.standard_materials.add(StandardMaterial {
            cull_mode: None,
            ..source
        });
        commands.spawn((
            Mesh3d(assets.meshes.add(mesh)),
            MeshMaterial3d(material),
            Transform::from_translation(base + Vec3::Y * lift).with_rotation(rotation),
            ChildOf(root),
        ));
    }
}
