use super::ground_track_policy::{GroundTrackPaintAsset, GroundTrackPaintSource};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackRouteItem;
use openzt2_zoo_state::transport_tours::transport_topology_types::{
    SkyTowerEndpoint, SkyTowerEndpoints,
};
use openzt2_zoo_state::transport_tours::transport_topology_types::{
    TrackEnd, TrackIncidence, TrackRoute, TrackStart,
};
use std::collections::HashSet;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::render::storage::ShaderBuffer;
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::world_definitions::transportation_and_tours::GroundTransportTrackPiecePresentationDefinition;
use openzt2_game_data::world_definitions::transportation_and_tours::SkyTowerTrackPresentationDefinition;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::topology::{
    topology_edit_types::PathConnectivityChanged,
    topology_graph_types::{PathTile, TileSurface, TopologyGrid, TopologyIndex},
};

use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainPaintedTexture;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_fitted_surface_mesh_construction::construct_square_surface_mesh_fitted_to_canonical_terrain_samples_with_rotated_texture_coordinates;

use openzt2_zoo_state::transport_tours::transport_topology_types::TrackProfile;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackSegment;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackJunction;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::{
    TrackPieces, TransportTrackConstructionPiece,
};

#[derive(Component)]
pub(super) struct AuthoredTransportTrackPiecePresentationProjected;

#[derive(Component)]
#[relationship(relationship_target = RouteDrawings)]
pub(super) struct DrawingForRoute(Entity);

#[derive(Component)]
#[relationship_target(relationship = DrawingForRoute, linked_spawn)]
pub(super) struct RouteDrawings(Vec<Entity>);

#[derive(Component)]
pub(super) struct SkyTowerDrawing;

const AUTHORED_TERRAIN_DECAL_MATERIAL_PATH: &str = "Materials/terraindecal.bfmat";

#[derive(Resource)]
pub(super) struct TransportTrackMaterials {
    pub(super) terrain_decal: Handle<MaterialAsset>,
}

pub(super) fn load_transport_track_materials(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    commands.insert_resource(TransportTrackMaterials {
        terrain_decal: asset_server.load(AUTHORED_TERRAIN_DECAL_MATERIAL_PATH),
    });
}

/// The compass point a direction heads to: 0 = north (-Z, source +Y),
/// 2 = east (+X), counting clockwise in eighths.
fn compass_point(direction: Vec3) -> u8 {
    let heading = direction
        .x
        .atan2(-direction.z)
        .to_degrees()
        .rem_euclid(360.0);
    ((heading / 45.0).round() as u8) & 7
}

/// `pieceType` of `ZTTransportGroundTrackTextureData`: 1 north-south,
/// 2 east-west, 3 north-east to south-west, 4 north-west to south-east.
const fn authored_ground_track_piece_type(heading: u8) -> u8 {
    match heading & 3 {
        0 => 1,
        2 => 2,
        1 => 3,
        _ => 4,
    }
}

/// `appearance` of a piece whose end at compass point `end` meets a piece
/// leaving towards `away`, named by its texture: `NtoSE` comes from the north
/// and turns south-east at its south end. A diagonal shares the appearance of
/// the straight it meets, so `SWtoCurve` is laid where a road from the
/// south-west meets a straight at the diagonal's north-east end.
const fn authored_ground_track_turn_appearance(end: u8, away: u8) -> Option<u8> {
    Some(match (end, away) {
        (4, 3) => 2,
        (4, 5) => 3,
        (0, 7) => 6,
        (0, 1) => 7,
        (2, 3) => 1,
        (2, 1) => 8,
        (6, 5) => 4,
        (6, 7) => 5,
        (7, _) => 1,
        (1, _) => 3,
        (3, _) => 5,
        (5, _) => 7,
        _ => return None,
    })
}

/// The heading of the piece at `index`, and the turn appearance at its start
/// and end where a neighbouring piece leaves at an angle.
fn authored_ground_track_turns(route: &[Vec3], index: usize) -> (u8, [Option<u8>; 2]) {
    let heading = compass_point(route[index + 1] - route[index]);
    let turn = |end: u8, leaving: Vec3| {
        let away = compass_point(leaving);
        (away != end)
            .then(|| authored_ground_track_turn_appearance(end, away))
            .flatten()
    };
    let start = index
        .checked_sub(1)
        .and_then(|previous| turn((heading + 4) & 7, route[previous] - route[index]));
    let end = route
        .get(index + 2)
        .and_then(|next| turn(heading, *next - route[index + 1]));
    (heading, [start, end])
}

pub(super) fn select_authored_ground_track_piece_presentation<'a>(
    definitions: &'a [GroundTransportTrackPiecePresentationDefinition],
    route: &[Vec3],
    index: usize,
) -> Option<&'a GroundTransportTrackPiecePresentationDefinition> {
    let (heading, turns) = authored_ground_track_turns(route, index);
    let piece_type = authored_ground_track_piece_type(heading);
    let appearance = match (piece_type, turns) {
        (_, [None, None]) => 0,
        (3 | 4, [Some(_), Some(_)]) => {
            let start_away = compass_point(route[index - 1] - route[index]);
            let end_away = compass_point(route[index + 2] - route[index + 1]);
            let sign = |end: u8, away: u8| (away + 8 - end) % 8 == 1;
            let mut flags = [sign((heading + 4) & 7, start_away), sign(heading, end_away)];
            let canonical = if piece_type == 3 { 1 } else { 7 };
            if heading == canonical {
                flags.swap(0, 1);
            }
            match (piece_type, flags) {
                (3, [false, false]) => 14,
                (3, [false, true]) => 11,
                (3, [true, false]) => 9,
                (3, [true, true]) => 15,
                (4, [false, false]) => 16,
                (4, [false, true]) => 12,
                (4, [true, false]) => 10,
                _ => 13,
            }
        }
        (_, [Some(first), _] | [None, Some(first)]) => first,
    };
    definitions.iter().find(|definition| {
        definition.piece_type == piece_type && definition.appearance == appearance
    })
}

/// Lays a road texture as the original paints it: V runs north (-Z).
pub(super) fn mirror_texture_rows(mut mesh: Mesh) -> Mesh {
    for attribute in [Mesh::ATTRIBUTE_UV_0, Mesh::ATTRIBUTE_UV_1] {
        if let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) =
            mesh.attribute_mut(attribute)
        {
            for uv in uvs {
                uv[1] = 1.0 - uv[1];
            }
        }
    }
    mesh
}

fn authored_sky_tower_top_kind(
    route: &TrackRouteItem,
    routes: &Query<TrackRoute>,
    incidence: &TrackIncidence,
) -> u8 {
    const U_TURN_MINIMUM_NEIGHBOUR_DIRECTION_DOT_PRODUCT: f32 = 0.258_819_07;
    incidence.incident(route.to())
        .filter_map(|entity| routes.get(entity).ok())
        .find(|candidate| {
            #[allow(clippy::suspicious_operation_groupings, reason = "find the adjoining segment at this endpoint, excluding a return to the previous endpoint")]
            let joins_other_endpoint = (candidate.from() == route.to() && candidate.to() != route.from())
                || (candidate.to() == route.to() && candidate.from() != route.from());
            joins_other_endpoint
        })
        .map_or(1, |next| {
            let tower_to_previous = (route.from_position - route.to_position)
                .xz()
                .normalize_or_zero();
            let next_neighbour_position = if next.from() == route.to() {
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

/// A ground route's points with, at each end where exactly one other route
/// joins it, that route's next point, so a piece at a joint turns towards
/// the road it meets; and the index of the route's first point in them.
fn ground_route_points_with_joined_neighbours(
    route: &TrackRouteItem,
    routes: &Query<TrackRoute>,
    incidence: &TrackIncidence,
) -> (Vec<Vec3>, usize) {
    // A station's two ends share its entity, so an end is the pair.
    let neighbour = |junction: (Entity, u8)| {
        let mut joined = incidence
            .incident(junction.0)
            .filter_map(|entity| routes.get(entity).ok())
            .filter_map(|candidate| {
                if core::ptr::eq(candidate.geometry, route.geometry) {
                    return None;
                }
                let points = &candidate.path_points;
                if (candidate.from(), candidate.from_endpoint_index) == junction {
                    points.get(1).copied()
                } else if (candidate.to(), candidate.to_endpoint_index) == junction {
                    points.len().checked_sub(2).map(|index| points[index])
                } else {
                    None
                }
            });
        match (joined.next(), joined.next()) {
            (Some(point), None) => Some(point),
            _ => None,
        }
    };
    let before = neighbour((route.from(), route.from_endpoint_index));
    let points = before
        .into_iter()
        .chain(route.path_points.iter().copied())
        .chain(neighbour((route.to(), route.to_endpoint_index)))
        .collect();
    (points, usize::from(before.is_some()))
}

/// The decal or elevated strip a ground piece draws, redrawn with it.
#[derive(Component)]
pub(super) struct GroundTrackPieceDecal;

#[derive(Message)]
pub(super) struct DiscardedTrackJointEndpoints {
    pub(super) route: Entity,
    pub(super) endpoints: [Entity; 2],
}

#[derive(Message)]
pub(super) struct TransportTrackJointChanged(Entity);

pub(super) fn capture_discarded_track_joint_endpoints(
    discarded: On<
        Discard<(
            TrackSegment,
            TrackStart,
            TrackEnd,
            super::transport_route_position_calculations::TrackTravelDistances,
        )>,
    >,
    routes: Query<TrackRoute>,
    mut joints: MessageWriter<DiscardedTrackJointEndpoints>,
) {
    if let Ok(route) = routes.get(discarded.entity) {
        joints.write(DiscardedTrackJointEndpoints {
            route: discarded.entity,
            endpoints: [route.from(), route.to()],
        });
    }
}

/// Redraws the road pieces next to a joint a route was added to or removed
/// from, so they turn towards (or no longer towards) the road there.
pub(super) fn invalidate_track_drawings_at_changed_joints(
    mut commands: Commands,
    mut changed_endpoints: MessageWriter<TransportTrackJointChanged>,
    changed_routes: Query<
        TrackRoute,
        Or<(
            Changed<TrackSegment>,
            Changed<TrackStart>,
            Changed<TrackEnd>,
        )>,
    >,
    changed_cables: Query<
        (
            &TrackStart,
            &TrackEnd,
            &super::transport_route_position_calculations::TrackTravelDistances,
        ),
        Changed<super::transport_route_position_calculations::TrackTravelDistances>,
    >,
    mut discarded: MessageReader<DiscardedTrackJointEndpoints>,
    incidence: TrackIncidence,
    route_pieces: Query<&TrackPieces>,
    drawings: Query<&RouteDrawings>,
    pieces: Query<(
        Entity,
        &TransportTrackConstructionPiece,
        &TrackProfile,
        Option<&Children>,
    )>,
    decals: Query<(), With<GroundTrackPieceDecal>>,
) {
    let mut changed = discarded
        .read()
        .flat_map(|discarded| discarded.endpoints)
        .collect::<HashSet<_>>();
    for route in &changed_routes {
        changed.extend([route.from(), route.to()]);
    }
    for (start, end, distances) in &changed_cables {
        if distances.sky_cable().is_some() {
            changed.extend([start.0, end.0]);
        }
    }
    if changed.is_empty() {
        return;
    }
    for &endpoint in &changed {
        changed_endpoints.write(TransportTrackJointChanged(endpoint));
    }
    let neighbours: bevy::ecs::entity::EntityHashSet = changed
        .into_iter()
        .flat_map(|endpoint| incidence.incident(endpoint))
        .collect();
    for route in neighbours {
        if let Ok(drawings) = drawings.get(route) {
            for drawing in drawings.iter() {
                commands.entity(drawing).try_despawn();
            }
        }
        let Ok(route_pieces) = route_pieces.get(route) else {
            continue;
        };
        for (piece, _, _, children) in pieces.iter_many(route_pieces.iter()).filter_map(Result::ok)
        {
            for child in children
                .into_iter()
                .flat_map(|children| children.iter())
                .filter(|child| decals.contains(*child))
            {
                commands.entity(child).try_despawn();
            }
            commands
                .entity(piece)
                .remove::<AuthoredTransportTrackPiecePresentationProjected>();
        }
        commands
            .entity(route)
            .remove::<AuthoredTransportTrackPiecePresentationProjected>();
    }
}

pub(super) fn invalidate_ground_track_surface_drawings(
    mut commands: Commands,
    (graphics, policy, active, definition_assets): (
        Res<GraphicsSettings>,
        GroundTrackPaintAsset,
        Res<WorldDefinitions>,
        Res<Assets<WorldDefinitionAsset>>,
    ),
    mut images: MessageReader<AssetEvent<Image>>,
    mut paths: MessageReader<PathConnectivityChanged>,
    grid: Single<&TopologyGrid>,
    pieces: Query<
        (Entity, &TrackProfile, &Transform, Option<&Children>),
        With<AuthoredTransportTrackPiecePresentationProjected>,
    >,
    decals: Query<(), With<GroundTrackPieceDecal>>,
) {
    let changed: Vec<_> = paths.read().map(|change| change.0.xy()).collect();
    let assets_changed = graphics.is_changed()
        || policy.is_changed()
        || images.read().fold(false, |changed, event| {
            changed
                || match event {
                    AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                        policy.get().is_some_and(|policy| policy.uses_image(*id))
                    }
                    _ => false,
                }
        });
    let width = active.get(&definition_assets).map_or(0, |definitions| {
        definitions
            .paths()
            .filter(|path| path.elevated)
            .map(|path| path.width_cm)
            .max()
            .unwrap_or_default()
    });
    let reach = (f32::from(width) * 0.005 / grid.spacing_m).ceil() as i32;
    if !assets_changed && changed.is_empty() {
        return;
    }
    for (entity, profile, transform, children) in &pieces {
        if profile.kind != TransportationTrackKind::Ground {
            continue;
        }
        let centre = grid.position_cell(transform.translation).xy();
        if !assets_changed
            && !changed
                .iter()
                .any(|cell| (centre - cell).abs().max_element() <= reach)
        {
            continue;
        }
        for child in children
            .into_iter()
            .flat_map(|children| children.iter())
            .filter(|&child| decals.contains(child))
        {
            commands.entity(child).try_despawn();
        }
        commands
            .entity(entity)
            .remove::<AuthoredTransportTrackPiecePresentationProjected>();
    }
}

pub(super) fn remove_discarded_route_drawings(
    discarded: On<Discard<TrackSegment>>,
    drawings: Query<&RouteDrawings>,
    mut commands: Commands,
) {
    if let Ok(drawings) = drawings.get(discarded.entity) {
        for drawing in drawings.iter() {
            commands.entity(drawing).try_despawn();
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
    // The construction marker over a tower's top, shown while a line ends
    // there (skytower's constructionMarker binders).
    if let Some(marker) = definitions.scene(openzt2_game_data::AssetId::from_virtual_path(
        &openzt2_game_assets::model_source::native_model_source_lowering::native_model_scene_labelled_asset_path(
            "ui/fx/underconstruction/underconstruction_sky.nif",
        ),
    )) {
        commands.spawn((
            Transform::from_translation(local + Vec3::Y * height),
            Visibility::Hidden,
            ChildOf(parent),
            openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation::new(marker),
            SkyTowerConstructionMarker,
        ));
    }
    openzt2_world::world_spawn::expanding_column_presentation::spawn_expanding_column_presentation(
        commands,
        definitions,
        parent,
        Transform::from_translation(local),
        height,
        Quat::from_rotation_y(direction.x.atan2(direction.z)),
        &presentation.columns[top_kind],
    );
}

pub(super) fn authored_rope_curve_points(
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
            if index == count {
                end
            } else {
                start.lerp(end, fraction)
                    - Vec3::Y * (std::f32::consts::PI * fraction).sin() * droop
            }
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
    // The fixed-function shader always reads a second texture coordinate set.
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, texture_coordinates.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, texture_coordinates);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

#[derive(SystemParam)]
pub(super) struct TrackPathSurfaces<'w, 's> {
    worlds: Query<'w, 's, (&'static TopologyGrid, &'static TopologyIndex)>,
    paths: Query<'w, 's, (&'static PathTile, &'static TileSurface)>,
}

impl TrackPathSurfaces<'_, '_> {
    pub(super) fn supports(
        &self,
        root: Entity,
        points: [Vec3; 2],
        definitions: WorldDefinitionsView<'_>,
    ) -> bool {
        let Ok((grid, index)) = self.worlds.get(root) else {
            return false;
        };
        let reach = definitions
            .paths()
            .filter(|path| path.elevated)
            .map(|path| path.width_cm)
            .max()
            .unwrap_or_default();
        let reach = IVec2::splat((f32::from(reach) * 0.005 / grid.spacing_m).ceil() as i32);
        // The two ends and centre must each have an elevated path surface
        // at the track's level. A path passing beneath the road is unrelated.
        [points[0], points[0].midpoint(points[1]), points[1]]
            .into_iter()
            .all(|point| {
                let cell = grid.position_cell(point).xy();
                index
                    .paths_within(cell - reach, cell + reach)
                    .any(|entity| {
                        let Ok((tile, surface)) = self.paths.get(entity) else {
                            return false;
                        };
                        let Some(path) = definitions
                            .find_path(tile.definition)
                            .filter(|path| path.elevated)
                        else {
                            return false;
                        };
                        let centre = grid.cell_translation(tile.cell);
                        let local = (point.xz() - centre.xz()) / (f32::from(path.width_cm) * 0.005);
                        if !local.abs().cmple(Vec2::ONE).all() {
                            return false;
                        }
                        let [a, b, c, d, middle] =
                            surface.height_cm.map(|height| f32::from(height) * 0.01);
                        let u = local.abs().max_element();
                        let edge = if local.x.abs() >= local.y.abs() {
                            let t = (u + local.y) * 0.5;
                            if local.x >= 0.0 {
                                (u - t) * b + t * c
                            } else {
                                (u - t) * a + t * d
                            }
                        } else {
                            let t = (u + local.x) * 0.5;
                            if local.y >= 0.0 {
                                (u - t) * d + t * c
                            } else {
                                (u - t) * a + t * b
                            }
                        };
                        (point.y - (centre.y + (1.0 - u) * middle + edge)).abs() <= 0.005
                    })
            })
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_authored_sky_track_rope(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<EffectPassMaterial>,
    parent: Entity,
    parent_transform: &Transform,
    world_points: &[Vec3],
    presentation: &SkyTowerTrackPresentationDefinition,
) {
    let inverse = parent_transform.compute_affine().inverse();
    let points = world_points
        .iter()
        .copied()
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
    // The fixed-function shader always reads a second texture coordinate set.
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, texture_coordinates.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, texture_coordinates);
    mesh.insert_indices(Indices::U32(indices));
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        ChildOf(parent),
        SkyTrackRope,
    ));
}

/// The construction marker over a sky tower.
#[derive(Component)]
pub(super) struct SkyTowerConstructionMarker;

/// A sky tower shows its construction marker while no line leaves it, as
/// the original marks where a line can be continued.
pub(super) fn show_construction_markers_at_open_sky_towers(
    drawings: Query<(&ChildOf, &Children), With<SkyTowerDrawing>>,
    changed_drawings: Query<Entity, (With<SkyTowerDrawing>, Changed<Children>)>,
    tower_children: Query<&Children, With<TransportTrackJunction>>,
    mut joints: MessageReader<TransportTrackJointChanged>,
    incidence: TrackIncidence,
    mut markers: Query<&mut Visibility, With<SkyTowerConstructionMarker>>,
) {
    let mut dirty: bevy::ecs::entity::EntityHashSet = changed_drawings.iter().collect();
    dirty.extend(
        joints
            .read()
            .filter_map(|joint| tower_children.get(joint.0).ok())
            .flat_map(|children| children.iter()),
    );
    for (tower, children) in drawings.iter_many(dirty).filter_map(Result::ok) {
        let open = incidence.outgoing(tower.parent()).next().is_none();
        let next = if open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        for marker in children.iter() {
            if let Ok(mut visibility) = markers.get_mut(marker) {
                visibility.set_if_neq(next);
            }
        }
    }
}

/// A sky track's rope, which keeps its colour while its towers and stations
/// are highlighted.
#[derive(Component)]
pub(super) struct SkyTrackRope;

/// A sky station's rope wheels (`link_wheel`, `link_wheel2`), where its ropes
/// turn.
#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct SkyRopeAnchors<'w, 's> {
    stations: Query<'w, 's, (), With<TransportStation>>,
    towers: SkyTowerEndpoints<'w, 's>,
    children: Query<'w, 's, &'static Children>,
    nodes: Query<
        'w,
        's,
        (
            &'static openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier,
            &'static GlobalTransform,
        ),
    >,
}

impl SkyRopeAnchors<'_, '_> {
    pub(super) fn tower_top(&self, tower: Entity) -> Option<Vec3> {
        self.towers.position(tower, SkyTowerEndpoint::Upper)
    }

    /// The station's wheel nearest to its rope end at `near`.
    pub(super) fn station_wheel(&self, station: Entity, near: Vec3) -> Option<Vec3> {
        if !self.stations.contains(station) {
            return None;
        }
        let wheels = ["link_wheel", "link_wheel2"].map(openzt2_game_data::AssetId::from_key);
        self.children
            .iter_descendants(station)
            .filter_map(|node| self.nodes.get(node).ok())
            .filter(|(id, _)| wheels.contains(&id.0))
            .map(|(_, transform)| transform.translation())
            .min_by(|a, b| {
                a.xz()
                    .distance(near.xz())
                    .total_cmp(&b.xz().distance(near.xz()))
            })
    }
}

#[derive(Message)]
pub(super) struct DiscardedSkyCableAnchor(Entity);

pub(super) fn notify_discarded_sky_cable_anchor(
    discarded: On<Discard<(PrefabAuthoredAttachmentIdentifier, SkyTowerEndpoint)>>,
    parents: Query<&ChildOf>,
    angles: Query<&PrefabAuthoredAttachmentIdentifier>,
    stations: Query<(), With<TransportStation>>,
    tower_ends: Query<&SkyTowerEndpoint>,
    mut changed: MessageWriter<DiscardedSkyCableAnchor>,
) {
    if tower_ends
        .get(discarded.entity)
        .is_ok_and(|end| *end == SkyTowerEndpoint::Upper)
    {
        if let Ok(parent) = parents.get(discarded.entity) {
            changed.write(DiscardedSkyCableAnchor(parent.parent()));
        }
    } else if angles.get(discarded.entity).is_ok_and(|name| {
        ["link_wheel", "link_wheel2"]
            .map(openzt2_game_data::AssetId::from_key)
            .contains(&name.0)
    }) {
        if let Some(station) = parents
            .iter_ancestors::<ChildOf>(discarded.entity)
            .find(|&ancestor| stations.contains(ancestor))
        {
            changed.write(DiscardedSkyCableAnchor(station));
        }
    }
}

pub(super) fn reconcile_sky_track_cable_geometry(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    catalogue: Res<WorldDefinitions>,
    rope_anchors: SkyRopeAnchors,
    mut discarded: MessageReader<DiscardedSkyCableAnchor>,
    parents: Query<&ChildOf>,
    changed_towers: Query<
        Entity,
        (
            With<TransportTrackJunction>,
            Or<(
                Changed<
                    openzt2_zoo_state::transport_tours::transport_topology_types::SkyTowerHeight,
                >,
                Changed<GlobalTransform>,
            )>,
        ),
    >,
    changed_wheels: Query<(
        Entity,
        &openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier,
    ), Changed<GlobalTransform>>,
    mut routes: Query<(
        Entity,
        Ref<TrackSegment>,
        Ref<TrackStart>,
        Ref<TrackEnd>,
        Ref<TrackProfile>,
        Option<&mut super::transport_route_position_calculations::TrackTravelDistances>,
    )>,
) {
    use super::transport_route_position_calculations::TrackTravelDistances;
    let definitions_changed = definitions.is_changed() || catalogue.is_changed();
    let Some(definitions) = catalogue.get(&definitions) else {
        return;
    };
    let wheel_names = ["link_wheel", "link_wheel2"].map(openzt2_game_data::AssetId::from_key);
    let mut changed: bevy::ecs::entity::EntityHashSet = changed_towers.iter().collect();
    changed.extend(discarded.read().map(|anchor| anchor.0));
    changed.extend(changed_wheels.iter().filter_map(|(entity, name)| {
        if !wheel_names.contains(&name.0) {
            return None;
        }
        parents
            .iter_ancestors::<ChildOf>(entity)
            .find(|&ancestor| rope_anchors.stations.contains(ancestor))
    }));
    for (entity, segment, start, end, profile, current) in &mut routes {
        if profile.kind != TransportationTrackKind::Sky
            || (!definitions_changed
                && !segment.is_changed()
                && !start.is_changed()
                && !end.is_changed()
                && !profile.is_changed()
                && !changed.contains(&start.0)
                && !changed.contains(&end.0))
        {
            continue;
        }
        let next = (|| {
            let presentation = definitions
                .find_track(profile.definition)?
                .sky_tower_presentation
                .as_ref()?;
            let anchor = |endpoint, near| {
                rope_anchors
                    .station_wheel(endpoint, near)
                    .or_else(|| rope_anchors.tower_top(endpoint))
            };
            let from = anchor(start.0, segment.from_position)?;
            let to = anchor(end.0, segment.to_position)?;
            TrackTravelDistances::from_sky_cable(authored_rope_curve_points(from, to, presentation))
        })();
        match (current, next) {
            (Some(mut current), Some(next)) => {
                current.set_if_neq(next);
            }
            (None, Some(next)) => {
                commands.entity(entity).insert(next);
            }
            (Some(_), None) => {
                commands.entity(entity).remove::<TrackTravelDistances>();
            }
            (None, None) => {}
        }
    }
}

/// A road piece's decal over its painted road, shown only while the piece is
/// highlighted.
#[derive(Component)]
pub(super) struct PaintedTrackHighlight;

/// The road a ground piece paints into the terrain (`paintTexture`), and the
/// size of a decal of the whole texture laid over it. Paint textures are
/// authored at the terrain's texel size with their copy window centred, so a
/// 43-texel window sits in 64 texels and an 86-texel one in 128.
#[derive(Default)]
pub(super) struct GroundTrackPaintImages(
    bevy::platform::collections::HashMap<
        (bevy::asset::AssetId<Image>, bevy::asset::AssetId<Image>, u8),
        Handle<Image>,
    >,
);

fn authored_ground_track_paint(
    source: &GroundTrackPaintSource,
    flip: u8,
    centre: Vec2,
    images: &mut Assets<Image>,
    cache: &mut GroundTrackPaintImages,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> Option<(TerrainPaintedTexture, f32)> {
    let (chunk, _) = terrain_chunks
        .get(terrain_chunk_at(terrain_index, centre)?)
        .ok()?;
    let terrain = terrain_assets.get(&chunk.asset)?.canonical_terrain_grid();
    let grid = &terrain.presentation;
    let texel_metres = f32::from(grid.surface_margin_cells)
        .mul_add(2.0, f32::from(chunk.side.checked_sub(1)?))
        * chunk.spacing_m
        / f32::from(grid.surface_resolution);
    let key = (source.texture.id(), source.mask.id(), flip);
    let texture = if let Some(handle) = cache.0.get(&key) {
        handle.clone()
    } else {
        let texture = images.get(&source.texture)?;
        let mask = images.get(&source.mask)?;
        let [width, height] = texture.size().to_array();
        let mask_size = mask.size();
        let mut bytes = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let color = texture.get_color_at(x, y).ok()?.to_srgba();
                let mx = x * mask_size.x / width;
                let my = y * mask_size.y / height;
                let mx = if flip & 1 == 0 {
                    mx
                } else {
                    mask_size.x - mx - 1
                };
                let my = if flip & 2 == 0 {
                    my
                } else {
                    mask_size.y - my - 1
                };
                let mask_colour = mask.get_color_at(mx, my).ok()?;
                let mask_sample = match mask.texture_descriptor.format {
                    TextureFormat::Rgba8UnormSrgb | TextureFormat::Bgra8UnormSrgb => {
                        let channels = mask_colour.to_srgba();
                        channels.red * channels.alpha
                    }
                    _ => {
                        let channels = mask_colour.to_linear();
                        channels.red * channels.alpha
                    }
                };
                let bytes_rgba: [u8; 4] = [color.red, color.green, color.blue, mask_sample * 0.5]
                    .map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8);
                bytes.extend(bytes_rgba);
            }
        }
        let image = Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            bytes,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let handle = images.add(image);
        cache.0.insert(key, handle.clone());
        handle
    };
    let texels = images.get(&texture)?.size().as_vec2();
    if source.size.cmple(Vec2::ZERO).any() {
        return None;
    }
    Some((
        TerrainPaintedTexture {
            centre,
            size_m: source.size.x * texel_metres,
            texture,
            uv_min: source.start / texels,
            uv_max: (source.start + source.size) / texels,
        },
        texels.x * texel_metres,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn project_committed_transport_track_piece_authored_prefabs(
    mut commands: Commands,
    materials: Res<TransportTrackMaterials>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    authored_materials: Res<Assets<MaterialAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    (ground_tracks, mut images, mut paint_cache, mut image_events): (
        GroundTrackPaintAsset,
        ResMut<Assets<Image>>,
        Local<GroundTrackPaintImages>,
        MessageReader<AssetEvent<Image>>,
    ),
    mut effect_pass_materials: ResMut<Assets<EffectPassMaterial>>,
    mut effect_uniform_buffers: ResMut<Assets<ShaderBuffer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_index: Single<&TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    (
        endpoint_transforms,
        rope_anchors,
        children,
        sky_drawings,
        incidence,
        routes,
        distances,
        surfaces,
    ): (
        Query<&GlobalTransform, With<TransportTrackJunction>>,
        SkyRopeAnchors,
        Query<&Children>,
        Query<(), With<SkyTowerDrawing>>,
        TrackIncidence,
        Query<TrackRoute>,
        Query<&super::transport_route_position_calculations::TrackTravelDistances>,
        TrackPathSurfaces,
    ),
    pieces: Query<
        (
            Entity,
            &Transform,
            &TrackProfile,
            &TransportTrackConstructionPiece,
            &WorldMember,
        ),
        Without<AuthoredTransportTrackPiecePresentationProjected>,
    >,
) {
    for event in image_events.read() {
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } = event {
            paint_cache
                .0
                .retain(|(texture, mask, _), _| id != texture && id != mask);
        }
    }
    let terrain_index = terrain_index.into_inner();
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    let terrain_decal_pass = authored_materials
        .get(&materials.terrain_decal)
        .and_then(|material| {
            material
                .evaluated_pass_material_assets_for_effect_quality(graphics_settings.effects)
                .first()
        })
        .and_then(|material| effect_pass_materials.get(material))
        .cloned();
    let mut tower_targets_projected_this_run = HashSet::new();
    for (entity, transform, profile, piece, member) in &pieces {
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
                let Some(cable) = distances
                    .get(piece.route_segment)
                    .ok()
                    .and_then(|distances| distances.sky_cable())
                else {
                    continue;
                };
                let tower_world_transform = endpoint_transforms
                    .get(route.to())
                    .ok()
                    .map(GlobalTransform::compute_transform);
                let Some(template) = terrain_decal_pass.as_ref() else {
                    continue;
                };
                // The shipped rope names elevpathtest.tga, which no archive
                // contains; the original draws the rope untextured then.
                let texture = definitions
                    .texture_image(presentation.rope_texture)
                    .unwrap_or_default();
                let mut line_material = template.clone();
                if !line_material.configure_for_authored_blue_fang_line(texture) {
                    error!("sky track could not bind the authored BFLine fixed-function state");
                    continue;
                }
                line_material
                    .recreate_uniform_buffers_for_runtime_material(&mut effect_uniform_buffers);
                let material = effect_pass_materials.add(line_material);
                let promoted_height_metres = rope_anchors.tower_top(route.to()).map_or_else(
                    || {
                        super::sky_tower_navigation::authored_sky_tower_height(
                            track,
                            route.from_position,
                            route.to_position,
                            0,
                        )
                        .unwrap_or_default()
                    },
                    |top| top.y - route.to_position.y,
                );
                if let Some(tower_world_transform) = tower_world_transform.filter(|_| {
                    !children.get(route.to()).is_ok_and(|children| {
                        children.iter().any(|child| sky_drawings.contains(child))
                    }) && tower_targets_projected_this_run.insert(route.to())
                }) {
                    let top_kind = authored_sky_tower_top_kind(&route, &routes, &incidence);
                    let tower_presentation_root = commands
                        .spawn((
                            Transform::IDENTITY,
                            Visibility::Inherited,
                            ChildOf(route.to()),
                            SkyTowerDrawing,
                            DrawingForRoute(piece.route_segment),
                        ))
                        .id();
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
                let rope_root = commands
                    .spawn((
                        Transform::IDENTITY,
                        Visibility::Inherited,
                        ChildOf(entity),
                        DrawingForRoute(piece.route_segment),
                    ))
                    .id();
                spawn_authored_sky_track_rope(
                    &mut commands,
                    &mut meshes,
                    material,
                    rope_root,
                    transform,
                    cable,
                    presentation,
                );
            }
        } else {
            let (Some(template), Ok(route)) =
                (terrain_decal_pass.as_ref(), routes.get(piece.route_segment))
            else {
                continue;
            };
            let (joined_points, first) =
                ground_route_points_with_joined_neighbours(&route, &routes, &incidence);
            let Some(presentation) = select_authored_ground_track_piece_presentation(
                &track.ground_piece_presentations,
                &joined_points,
                usize::from(piece.piece_index) + first,
            ) else {
                error!("ground track has no authored pieceType/appearance texture case");
                commands
                    .entity(entity)
                    .insert(AuthoredTransportTrackPiecePresentationProjected);
                continue;
            };
            let elevated = surfaces.supports(
                member.root,
                [
                    route.path_points[usize::from(piece.piece_index)],
                    route.path_points[usize::from(piece.piece_index) + 1],
                ],
                definitions,
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
                    GroundTrackPieceDecal,
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
            let Some((source, flip)) = ground_tracks.get().and_then(|policy| policy.paint(
                presentation.piece_type, presentation.appearance,
                graphics_settings.terrain_texture_size == openzt2_game_assets::graphics_settings_types::ThreeLevelGraphicsDetail::Low,
            )) else { continue; };
            let Some(paint) = authored_ground_track_paint(
                source,
                flip,
                transform.translation.xz(),
                &mut images,
                &mut paint_cache,
                &terrain_index,
                &terrain_assets,
                &terrain_chunks,
            ) else {
                continue;
            };
            let (paint, decal_size) = paint;
            let Some(mesh) = construct_square_surface_mesh_fitted_to_canonical_terrain_samples_with_rotated_texture_coordinates(
                decal_size,
                decal_centre,
                presentation.ground_decal_rotation_radians,
                &terrain_index,
                &terrain_assets,
                &terrain_chunks,
            ) else { continue; };
            let decal = commands
                .spawn((
                    Mesh3d(meshes.add(mirror_texture_rows(mesh))),
                    MeshMaterial3d(effect_pass_materials.add(material)),
                    Transform::from_translation(
                        transform
                            .compute_affine()
                            .inverse()
                            .transform_point3(decal_centre),
                    )
                    .with_rotation(transform.rotation.inverse()),
                    ChildOf(entity),
                    GroundTrackPieceDecal,
                ))
                .id();
            // As in the original, the road is painted into the ground; the
            // decal only shows a highlight.
            commands.entity(entity).insert(paint);
            commands
                .entity(decal)
                .insert((PaintedTrackHighlight, Visibility::Hidden));
        }
        commands
            .entity(entity)
            .insert(AuthoredTransportTrackPiecePresentationProjected);
    }
}

#[cfg(test)]
mod tests;

#[derive(Component)]
#[relationship(relationship_target = TransportConstructionMarkers)]
pub(super) struct TransportConstructionMarkerOf(Entity);

#[derive(Component, PartialEq, Eq)]
pub(super) struct TransportConstructionMarkerOrdinal(u8);

#[derive(Component)]
#[relationship_target(relationship = TransportConstructionMarkerOf)]
pub(super) struct TransportConstructionMarkers(Vec<Entity>);

pub(super) fn bind_authored_transport_construction_marker_roles(
    mut commands: Commands,
    nodes: Query<&PrefabAuthoredAttachmentIdentifier>,
    changed_nodes: Query<
        Entity,
        (
            With<PrefabAuthoredAttachmentIdentifier>,
            Or<(
                Changed<PrefabAuthoredAttachmentIdentifier>,
                Changed<ChildOf>,
            )>,
        ),
    >,
    added_owners: Query<
        Entity,
        Or<(
            Added<TransportStation>,
            Added<TransportTrackConstructionPiece>,
        )>,
    >,
    owners: Query<
        Has<TransportStation>,
        Or<(
            With<TransportStation>,
            With<TransportTrackConstructionPiece>,
        )>,
    >,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    mut roles: Query<(
        &TransportConstructionMarkerOf,
        &mut TransportConstructionMarkerOrdinal,
    )>,
) {
    let find_owner = |entity| {
        parents
            .iter_ancestors::<ChildOf>(entity)
            .find(|&node| owners.contains(node))
    };
    let mut affected: bevy::ecs::entity::EntityHashSet = added_owners.iter().collect();
    for entity in changed_nodes {
        if let Ok((role, _)) = roles.get(entity) {
            affected.insert(role.0);
            if find_owner(entity).is_none() {
                commands.entity(entity).remove::<(
                    TransportConstructionMarkerOf,
                    TransportConstructionMarkerOrdinal,
                )>();
            }
        }
        affected.extend(find_owner(entity));
    }
    let names = [
        "constructionmarker1",
        "constructionmarker2",
        "constructionmarker3",
        "constructionmarker4",
    ]
    .map(openzt2_game_data::AssetId::from_key);
    for owner in affected {
        let Ok(station) = owners.get(owner) else {
            continue;
        };
        let mut authored = Vec::new();
        for entity in children.iter_descendants(owner) {
            let Ok(name) = nodes.get(entity) else {
                continue;
            };
            if find_owner(entity) != Some(owner) {
                continue;
            }
            let ordinal = names.iter().position(|id| *id == name.0);
            if let Some(ordinal) = ordinal {
                authored.push((entity, ordinal as u8));
            } else if roles.contains(entity) {
                commands.entity(entity).remove::<(
                    TransportConstructionMarkerOf,
                    TransportConstructionMarkerOrdinal,
                )>();
            }
        }
        let paired = station && authored.iter().any(|&(_, ordinal)| ordinal == 3);
        for (entity, ordinal) in authored {
            let ordinal = if paired { ordinal / 2 } else { ordinal };
            if let Ok((role, mut current)) = roles.get_mut(entity) {
                current.set_if_neq(TransportConstructionMarkerOrdinal(ordinal));
                if role.0 != owner {
                    commands
                        .entity(entity)
                        .insert(TransportConstructionMarkerOf(owner));
                }
            } else {
                commands.entity(entity).insert((
                    TransportConstructionMarkerOf(owner),
                    TransportConstructionMarkerOrdinal(ordinal),
                ));
            }
        }
    }
}

/// Shows a road's construction marker only where the road stops at a
/// junction no road leaves or starts at one no road leads into, as the original marks an unfinished road
/// ("Click to place road"); every other piece keeps its markers hidden.
pub(super) fn show_construction_markers_at_unconnected_road_ends(
    pieces: Query<
        (
            &TransportTrackConstructionPiece,
            &Transform,
            &TransportConstructionMarkers,
            &TrackProfile,
        ),
        Without<PrefabAuthoredAttachmentIdentifier>,
    >,
    changed_pieces: Query<
        Entity,
        (
            With<TransportTrackConstructionPiece>,
            Or<(
                Changed<TransportConstructionMarkers>,
                Changed<Transform>,
                Changed<TrackProfile>,
                Changed<TransportTrackConstructionPiece>,
            )>,
        ),
    >,
    route_pieces: Query<&TrackPieces>,
    changed_roles: Query<
        &TransportConstructionMarkerOf,
        Or<(
            Changed<TransportConstructionMarkerOf>,
            Changed<TransportConstructionMarkerOrdinal>,
        )>,
    >,
    mut joints: MessageReader<TransportTrackJointChanged>,
    routes: Query<TrackRoute>,
    incidence: TrackIncidence,
    junctions: Query<(), With<TransportTrackJunction>>,
    mut markers: Query<
        (
            &TransportConstructionMarkerOrdinal,
            &mut Visibility,
            &mut Transform,
        ),
        Without<TransportTrackConstructionPiece>,
    >,
) {
    let mut dirty: bevy::ecs::entity::EntityHashSet = changed_pieces.iter().collect();
    dirty.extend(changed_roles.iter().map(|role| role.0));
    dirty.extend(
        joints
            .read()
            .flat_map(|joint| incidence.incident(joint.0))
            .filter_map(|route| route_pieces.get(route).ok())
            .flat_map(|pieces| pieces.iter()),
    );
    for (piece, piece_transform, children, profile) in
        pieces.iter_many(dirty).filter_map(Result::ok)
    {
        // A sky tower carries its own marker on its top.
        let ground = profile.kind == TransportationTrackKind::Ground;
        // A road left after a gap starts at a junction no road leads into,
        // and the original marks that end too.
        let open_end = routes.get(piece.route_segment).ok().and_then(|route| {
            let last = usize::from(piece.piece_index) + 2 == route.path_points.len();
            if !ground {
                None
            } else if last
                && junctions.contains(route.to())
                && incidence.outgoing(route.to()).next().is_none()
            {
                Some(route.to_position)
            } else if piece.piece_index == 0
                && junctions.contains(route.from())
                && incidence.incoming(route.from()).next().is_none()
            {
                Some(route.from_position)
            } else {
                None
            }
        });
        for child in children.iter() {
            let Ok((role, mut visibility, mut transform)) = markers.get_mut(child) else {
                continue;
            };
            let shown = if role.0 == 0 {
                open_end
                    .inspect(|end| {
                        let local = piece_transform
                            .compute_affine()
                            .inverse()
                            .transform_point3(*end);
                        if transform.translation != local {
                            transform.translation = local;
                        }
                    })
                    .is_some()
            } else if role.0 == 1 {
                false
            } else {
                continue;
            };
            let next = if shown {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != next {
                *visibility = next;
            }
        }
    }
}

/// A station shows the marker of each end no road is connected to yet, where
/// the original marks the place a road can start ("Click to place road").
#[allow(clippy::type_complexity)]
pub(super) fn show_construction_markers_at_unconnected_station_ends(
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    stations: Query<(
        Entity,
        &TransportStation,
        &GlobalTransform,
        &TransportConstructionMarkers,
    )>,
    changed_stations: Query<
        Entity,
        (
            With<TransportStation>,
            Or<(
                Changed<TransportStation>,
                Changed<GlobalTransform>,
                Changed<TransportConstructionMarkers>,
            )>,
        ),
    >,
    changed_roles: Query<
        &TransportConstructionMarkerOf,
        Or<(
            Changed<TransportConstructionMarkerOf>,
            Changed<TransportConstructionMarkerOrdinal>,
        )>,
    >,
    mut joints: MessageReader<TransportTrackJointChanged>,
    routes: Query<TrackRoute>,
    incidence: TrackIncidence,
    mut markers: Query<(&TransportConstructionMarkerOrdinal, &mut Visibility)>,
) {
    let definitions_changed = definition_assets.is_changed() || active_definitions.is_changed();
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    let mut dirty: bevy::ecs::entity::EntityHashSet = changed_stations.iter().collect();
    dirty.extend(changed_roles.iter().map(|role| role.0));
    dirty.extend(joints.read().map(|joint| joint.0));
    if definitions_changed {
        dirty.extend(stations.iter().map(|(entity, ..)| entity));
    }
    for (station_entity, station, station_transform, station_markers) in
        stations.iter_many(dirty).filter_map(Result::ok)
    {
        let mut connected = [false; 2];
        for route in incidence
            .incident(station_entity)
            .filter_map(|entity| routes.get(entity).ok())
        {
            if route.from() == station_entity {
                if let Some(connected) = connected.get_mut(usize::from(route.from_endpoint_index)) {
                    *connected = true;
                }
            }
            if route.to() == station_entity {
                if let Some(connected) = connected.get_mut(usize::from(route.to_endpoint_index)) {
                    *connected = true;
                }
            }
        }
        for marker in station_markers.iter() {
            let Ok((role, mut visibility)) = markers.get_mut(marker) else {
                continue;
            };
            let endpoint = role.0;
            let open_end = connected.get(usize::from(endpoint)).filter(|&&used| !used)
                .and_then(|_| {
                    crate::transport_tours::topology::calculate_authored_transport_station_endpoint_world_position(
                        definitions,
                        station,
                        station_transform,
                        endpoint,
                    )
                });
            let next = if open_end.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != next {
                *visibility = next;
            }
        }
    }
}
