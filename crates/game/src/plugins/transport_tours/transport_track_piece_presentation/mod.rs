use std::collections::HashSet;

use bevy::{
    asset::RenderAssetUsages,
    mesh::Indices,
    prelude::*,
    render::{render_resource::PrimitiveTopology, storage::ShaderBuffer},
};
use openzt2_game_data::world_definitions::transportation_and_tours::{
    GroundTransportTrackPiecePresentationDefinition, SkyTowerTrackPresentationDefinition,
    TransportationTrackKind,
};

use crate::assets::material::material_asset_types::MaterialAsset;
use crate::assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::settings::graphics_settings_types::GraphicsSettings;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::terrain::terrain_fitted_surface_mesh_construction::construct_square_surface_mesh_fitted_to_canonical_terrain_samples_with_rotated_texture_coordinates;

use super::{
    transport_topology_types::{TrackProfile, TrackSegment, TransportTrackJunction},
    transport_track_construction_types::TransportTrackConstructionPiece,
};

#[derive(Component)]
pub(super) struct AuthoredTransportTrackPiecePresentationProjected;

#[derive(Component)]
pub(super) struct AuthoredLinkedSkyTowerPresentationProjected {
    source_route: Entity,
    root: Entity,
    top_kind: u8,
}

pub(super) const AUTHORED_TERRAIN_DECAL_MATERIAL_PATH: &str = "Materials/terraindecal.bfmat";

fn signed_planar_cross_product(left: Vec3, right: Vec3) -> i8 {
    (left.x * right.z - left.z * right.x)
        .signum()
        .clamp(-1.0, 1.0) as i8
}

fn authored_ground_track_piece_type(direction: Vec3) -> u8 {
    let heading = direction
        .x
        .atan2(direction.z)
        .to_degrees()
        .rem_euclid(360.0);
    match ((heading / 45.0).round() as u8) & 7 {
        0 | 4 => 1,
        2 | 6 => 2,
        1 | 5 => 3,
        3 | 7 => 4,
        _ => unreachable!(),
    }
}

fn authored_ground_track_piece_appearance(piece_type: u8, flags: [i8; 2]) -> u8 {
    match (piece_type, flags) {
        (1, [-1, _]) => 6,
        (1, [1, _]) => 7,
        (1, [0, -1]) => 2,
        (1, [0, 1]) => 3,
        (2, [-1, _]) => 8,
        (2, [1, _]) => 1,
        (2, [0, -1]) => 4,
        (2, [0, 1]) => 5,
        (3, [-1, -1]) => 14,
        (3, [-1, 1]) => 3,
        (3, [-1, 0]) => 11,
        (3, [1, -1]) => 9,
        (3, [1, 1]) => 15,
        (3, [1, 0]) => 4,
        (3, [0, -1]) => 7,
        (3, [0, 1]) => 8,
        (4, [-1, -1]) => 16,
        (4, [-1, 1]) => 12,
        (4, [-1, 0]) => 1,
        (4, [1, -1]) => 10,
        (4, [1, 1]) => 13,
        (4, [1, 0]) => 2,
        (4, [0, -1]) => 5,
        (4, [0, 1]) => 6,
        _ => 0,
    }
}

pub(super) fn select_authored_ground_track_piece_presentation<'a>(
    definitions: &'a [GroundTransportTrackPiecePresentationDefinition],
    route: &[Vec3],
    index: usize,
) -> Option<&'a GroundTransportTrackPiecePresentationDefinition> {
    let direction = (route[index + 1] - route[index]).normalize_or_zero();
    let flags = [
        index.checked_sub(1).map_or(0, |previous| {
            signed_planar_cross_product(direction, route[index] - route[previous])
        }),
        route.get(index + 2).map_or(0, |next| {
            signed_planar_cross_product(direction, *next - route[index + 1])
        }),
    ];
    let piece_type = authored_ground_track_piece_type(direction);
    let appearance = authored_ground_track_piece_appearance(piece_type, flags);
    definitions.iter().find(|definition| {
        definition.piece_type == piece_type && definition.appearance == appearance
    })
}

fn authored_sky_tower_top_kind(route: &TrackSegment, routes: &Query<&TrackSegment>) -> u8 {
    const U_TURN_MINIMUM_NEIGHBOUR_DIRECTION_DOT_PRODUCT: f32 = 0.258_819_07;
    routes
        .iter()
        .find(|candidate| {
            #[allow(clippy::suspicious_operation_groupings, reason = "find the adjoining segment at this endpoint, excluding a return to the previous endpoint")]
            let joins_other_endpoint = (candidate.from == route.to && candidate.to != route.from)
                || (candidate.to == route.to && candidate.from != route.from);
            joins_other_endpoint
        })
        .map_or(1, |next| {
            let tower_to_previous = (route.from_position - route.to_position)
                .xz()
                .normalize_or_zero();
            let next_neighbour_position = if next.from == route.to {
                next.to_position
            } else {
                next.from_position
            };
            let tower_to_next = (next_neighbour_position - route.to_position)
                .xz()
                .normalize_or_zero();
            if tower_to_previous.dot(tower_to_next) > U_TURN_MINIMUM_NEIGHBOUR_DIRECTION_DOT_PRODUCT
            {
                2
            } else {
                0
            }
        })
}

pub(super) fn invalidate_sky_tower_presentations_after_connection_topology_changes(
    mut commands: Commands,
    routes: Query<&TrackSegment>,
    projected: Query<(Entity, &AuthoredLinkedSkyTowerPresentationProjected)>,
) {
    for (tower, projected) in &projected {
        let Ok(route) = routes.get(projected.source_route) else {
            commands.entity(projected.root).despawn();
            commands
                .entity(tower)
                .remove::<AuthoredLinkedSkyTowerPresentationProjected>();
            commands
                .entity(projected.source_route)
                .remove::<AuthoredTransportTrackPiecePresentationProjected>();
            continue;
        };
        let wanted = authored_sky_tower_top_kind(route, &routes);
        if projected.top_kind != wanted {
            commands.entity(projected.root).despawn();
            commands
                .entity(tower)
                .remove::<AuthoredLinkedSkyTowerPresentationProjected>();
            commands
                .entity(projected.source_route)
                .remove::<AuthoredTransportTrackPiecePresentationProjected>();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_authored_sky_tower(
    commands: &mut Commands,
    definitions: WorldDefinitionsView<'_>,
    parent: Entity,
    parent_transform: &Transform,
    points: &[Vec3],
    index: usize,
    top_kind: usize,
    promoted_height_metres: f32,
    presentation: &SkyTowerTrackPresentationDefinition,
) {
    let point = points[index];
    let height = promoted_height_metres.max(presentation.minimum_height_metres);
    let local = parent_transform
        .compute_affine()
        .inverse()
        .transform_point3(point);
    let direction = if index + 1 < points.len() {
        points[index + 1] - point
    } else {
        point - points[index - 1]
    };
    crate::plugins::world_spawn::expanding_column_presentation::spawn_expanding_column_presentation(
        commands,
        definitions,
        parent,
        Transform::from_translation(local),
        height,
        Quat::from_rotation_y(direction.x.atan2(direction.z)),
        &presentation.columns[top_kind],
    );
}

fn authored_rope_curve_points(
    start: Vec3,
    end: Vec3,
    presentation: &SkyTowerTrackPresentationDefinition,
) -> Vec<Vec3> {
    let length = start.distance(end);
    let count = (length * presentation.rope_points_per_metre)
        .ceil()
        .max(1.0) as usize;
    let droop = presentation.rope_droop_metres
        * (length / presentation.rope_droop_maximum_length_metres).min(1.0);
    (0..=count)
        .map(|index| {
            let fraction = index as f32 / count as f32;
            start.lerp(end, fraction) - Vec3::Y * (std::f32::consts::PI * fraction).sin() * droop
        })
        .collect()
}

pub(super) fn construct_elevated_ground_track_triangle_strip(
    route: &[Vec3],
    width: f32,
    texture_rotation_radians: f32,
    origin: Vec3,
) -> Option<Mesh> {
    let points = route;
    if points.len() < 2 || width <= 0.0 {
        return None;
    }
    let mut positions = Vec::with_capacity(points.len() * 2);
    let mut normals = Vec::with_capacity(points.len() * 2);
    let mut texture_coordinates = Vec::with_capacity(points.len() * 2);
    let mut indices = Vec::with_capacity((points.len() - 1) * 6);
    let mut travelled = 0.0;
    for (point_index, point) in points.iter().copied().enumerate() {
        let previous = points[point_index.saturating_sub(1)];
        let next = points[(point_index + 1).min(points.len() - 1)];
        let side = Vec3::Y.cross(next - previous).normalize_or_zero() * width * 0.5;
        if point_index != 0 {
            travelled += point.distance(previous);
        }
        positions.extend([
            (point - side - origin).to_array(),
            (point + side - origin).to_array(),
        ]);
        normals.extend([Vec3::Y.to_array(); 2]);
        for across in [0.0, 1.0] {
            let uv = Vec2::splat(0.5)
                + Mat2::from_angle(texture_rotation_radians)
                    * (Vec2::new(across, travelled / width) - Vec2::splat(0.5));
            texture_coordinates.push(uv.to_array());
        }
        if point_index != 0 {
            let current = u32::try_from(point_index * 2).ok()?;
            indices.extend([
                current - 2,
                current,
                current - 1,
                current - 1,
                current,
                current + 1,
            ]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, texture_coordinates);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

pub(super) fn ground_track_piece_is_elevated_above_terrain(
    points: [Vec3; 2],
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> bool {
    points.into_iter().any(|point| {
        terrain_chunk_at(terrain_index, point.xz())
            .and_then(|entity| terrain_chunks.get(entity).ok())
            .and_then(|(chunk, edited)| {
                sample_terrain(chunk, terrain_assets.get(&chunk.asset)?, edited, point.xz())
            })
            .is_some_and(|terrain| point.y > terrain.height_m + 0.05)
    })
}

#[allow(clippy::too_many_arguments)]
fn spawn_authored_sky_track_rope(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<EffectPassMaterial>,
    parent: Entity,
    parent_transform: &Transform,
    start: Vec3,
    end: Vec3,
    presentation: &SkyTowerTrackPresentationDefinition,
) {
    let inverse = parent_transform.compute_affine().inverse();
    let points = authored_rope_curve_points(start, end, presentation)
        .into_iter()
        .map(|point| inverse.transform_point3(point))
        .collect::<Vec<_>>();
    let mut positions = Vec::with_capacity(points.len() * 4);
    let mut normals = Vec::with_capacity(points.len() * 4);
    let mut texture_coordinates = Vec::with_capacity(points.len() * 4);
    let mut indices = Vec::with_capacity(points.len().saturating_sub(1) * 12);
    let mut distance = 0.0;
    for (index, point) in points.iter().copied().enumerate() {
        let previous = points[index.saturating_sub(1)];
        let next = points[(index + 1).min(points.len() - 1)];
        let tangent = (next - previous).normalize_or_zero();
        let horizontal =
            tangent.cross(Vec3::Y).normalize_or_zero() * presentation.rope_radius_metres;
        let vertical =
            tangent.cross(horizontal).normalize_or_zero() * presentation.rope_radius_metres;
        if index != 0 {
            distance += point.distance(points[index - 1]);
        }
        positions.extend([
            (point - horizontal).to_array(),
            (point + horizontal).to_array(),
            (point - vertical).to_array(),
            (point + vertical).to_array(),
        ]);
        normals.extend([Vec3::Y.to_array(); 4]);
        texture_coordinates.extend([
            [0.0, distance],
            [1.0, distance],
            [0.0, distance],
            [1.0, distance],
        ]);
        if index != 0 {
            let current = u32::try_from(index * 4).expect("rope point count fits u32");
            let prior = current - 4;
            indices.extend([
                prior,
                current,
                prior + 1,
                prior + 1,
                current,
                current + 1,
                prior + 2,
                prior + 3,
                current + 2,
                prior + 3,
                current + 3,
                current + 2,
            ]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, texture_coordinates);
    mesh.insert_indices(Indices::U32(indices));
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        ChildOf(parent),
    ));
}

#[allow(clippy::too_many_arguments)]
pub(super) fn project_committed_transport_track_piece_authored_prefabs(
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
    endpoint_transforms: Query<&GlobalTransform, With<TransportTrackJunction>>,
    existing_sky_towers: Query<&AuthoredLinkedSkyTowerPresentationProjected>,
    routes: Query<&TrackSegment>,
    pieces: Query<
        (
            Entity,
            &Transform,
            &TrackProfile,
            &TransportTrackConstructionPiece,
        ),
        Without<AuthoredTransportTrackPiecePresentationProjected>,
    >,
) {
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    let terrain_decal_material =
        asset_server.load::<MaterialAsset>(AUTHORED_TERRAIN_DECAL_MATERIAL_PATH);
    let terrain_decal_pass = authored_materials
        .get(&terrain_decal_material)
        .and_then(|material| {
            material
                .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
                .first()
        })
        .and_then(|material| effect_pass_materials.get(material))
        .cloned();
    let mut tower_targets_projected_this_run = HashSet::new();
    for (entity, transform, profile, piece) in &pieces {
        let Some(track) = definitions.find_track(profile.definition) else {
            continue;
        };
        if profile.kind == TransportationTrackKind::Sky {
            let (Some(presentation), Ok(route)) = (
                track.sky_tower_presentation.as_ref(),
                routes.get(piece.route_segment),
            ) else {
                continue;
            };
            if piece.piece_index == 0 && route.path_points.len() == 2 {
                let Ok(tower_global_transform) = endpoint_transforms.get(route.to) else {
                    continue;
                };
                let tower_world_transform = tower_global_transform.compute_transform();
                let (Some(texture), Some(template)) = (
                    definitions.texture_image(presentation.rope_texture),
                    terrain_decal_pass.as_ref(),
                ) else {
                    continue;
                };
                let mut line_material = template.clone();
                if !line_material.configure_for_authored_blue_fang_line(texture) {
                    error!("sky track could not bind the authored BFLine fixed-function state");
                    continue;
                }
                line_material
                    .recreate_uniform_buffers_for_runtime_material(&mut effect_uniform_buffers);
                let material = effect_pass_materials.add(line_material);
                let promoted_height_metres = presentation.initial_height_metres.max(
                    route.from_position.y - route.to_position.y
                        + track.sky_rope_clearance_metres.unwrap_or_default(),
                );
                if existing_sky_towers.get(route.to).is_err()
                    && tower_targets_projected_this_run.insert(route.to)
                {
                    let top_kind = authored_sky_tower_top_kind(route, &routes);
                    let tower_presentation_root = commands
                        .spawn((
                            Transform::IDENTITY,
                            Visibility::Inherited,
                            ChildOf(route.to),
                        ))
                        .id();
                    commands
                        .entity(route.to)
                        .insert(AuthoredLinkedSkyTowerPresentationProjected {
                            source_route: piece.route_segment,
                            root: tower_presentation_root,
                            top_kind,
                        });
                    spawn_authored_sky_tower(
                        &mut commands,
                        definitions,
                        tower_presentation_root,
                        &tower_world_transform,
                        &route.path_points,
                        1,
                        usize::from(top_kind),
                        promoted_height_metres,
                        presentation,
                    );
                }
                let rope_clearance = track.sky_rope_clearance_metres.unwrap_or_default();
                let source_height_metres = presentation
                    .initial_height_metres
                    .max(route.to_position.y - route.from_position.y + rope_clearance);
                spawn_authored_sky_track_rope(
                    &mut commands,
                    &mut meshes,
                    material,
                    entity,
                    transform,
                    route.path_points[0] + Vec3::Y * (source_height_metres - rope_clearance),
                    route.path_points[1] + Vec3::Y * (promoted_height_metres - rope_clearance),
                    presentation,
                );
            }
        } else {
            let (Some(template), Ok(route)) =
                (terrain_decal_pass.as_ref(), routes.get(piece.route_segment))
            else {
                continue;
            };
            let Some(presentation) = select_authored_ground_track_piece_presentation(
                &track.ground_piece_presentations,
                &route.path_points,
                usize::from(piece.piece_index),
            ) else {
                error!("ground track has no authored pieceType/appearance texture case");
                commands
                    .entity(entity)
                    .insert(AuthoredTransportTrackPiecePresentationProjected);
                continue;
            };
            let elevated = ground_track_piece_is_elevated_above_terrain(
                [
                    route.path_points[usize::from(piece.piece_index)],
                    route.path_points[usize::from(piece.piece_index) + 1],
                ],
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
                error!("ground track could not bind the authored terrain-decal Effects pass");
                commands
                    .entity(entity)
                    .insert(AuthoredTransportTrackPiecePresentationProjected);
                continue;
            }
            material.recreate_uniform_buffers_for_runtime_material(&mut effect_uniform_buffers);
            if elevated {
                let piece_index = usize::from(piece.piece_index);
                let Some(mesh) = construct_elevated_ground_track_triangle_strip(
                    &route.path_points[piece_index..=piece_index + 1],
                    presentation.ground_decal_size_metres[0],
                    presentation.elevated_path_rotation_radians,
                    transform.translation,
                ) else {
                    continue;
                };
                commands.spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(effect_pass_materials.add(material)),
                    Transform::from_rotation(transform.rotation.inverse()),
                    ChildOf(entity),
                ));
                commands
                    .entity(entity)
                    .insert(AuthoredTransportTrackPiecePresentationProjected);
                continue;
            }
            let decal_centre = transform.transform_point(Vec3::new(
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
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(effect_pass_materials.add(material)),
                Transform::from_translation(
                    transform
                        .compute_affine()
                        .inverse()
                        .transform_point3(decal_centre),
                )
                .with_rotation(transform.rotation.inverse()),
                ChildOf(entity),
            ));
        }
        commands
            .entity(entity)
            .insert(AuthoredTransportTrackPiecePresentationProjected);
    }
}
