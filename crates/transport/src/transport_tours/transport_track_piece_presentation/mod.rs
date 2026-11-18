use std::collections::HashSet;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_data::world_definitions::transportation_and_tours::GroundTransportTrackPiecePresentationDefinition;
use openzt2_game_data::world_definitions::transportation_and_tours::SkyTowerTrackPresentationDefinition;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;

use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
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
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionPiece;

#[derive(Component)]
pub(super) struct AuthoredTransportTrackPiecePresentationProjected;

#[derive(Component)]
pub(super) struct AuthoredLinkedSkyTowerPresentationProjected {
    source_route: Entity,
    root: Entity,
    top_kind: u8,
}

pub(super) const AUTHORED_TERRAIN_DECAL_MATERIAL_PATH: &str = "Materials/terraindecal.bfmat";

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
        // A diagonal curving at both ends.
        (3, [Some(_), Some(_)]) => 9,
        (4, [Some(_), Some(_)]) => 10,
        (_, [Some(first), _] | [None, Some(first)]) => first,
    };
    definitions.iter().find(|definition| {
        definition.piece_type == piece_type && definition.appearance == appearance
    })
}

/// How far a straight piece turning at one end is painted towards that end:
/// its turn texture's strip stops short of the joint, where the diagonal's
/// curve texture meets it.
// ponytail: 0.75 m was fitted against the shipped textures, not read from zt.exe.
const AUTHORED_GROUND_TRACK_TURN_SHIFT_METRES: f32 = 0.75;

fn authored_ground_track_paint_shift(route: &[Vec3], index: usize) -> Vec3 {
    let direction = (route[index + 1] - route[index])
        .with_y(0.0)
        .normalize_or_zero();
    match authored_ground_track_turns(route, index) {
        (heading, _) if heading % 2 == 1 => Vec3::ZERO,
        (_, [Some(_), _]) => -direction * AUTHORED_GROUND_TRACK_TURN_SHIFT_METRES,
        (_, [None, Some(_)]) => direction * AUTHORED_GROUND_TRACK_TURN_SHIFT_METRES,
        _ => Vec3::ZERO,
    }
}

/// A straight's turn texture is painted towards its turn, which leaves the
/// other end bare: that end is painted a second time towards it, with the end's
/// own turn texture, or with the plain road where a straight carries on.
fn authored_ground_track_second_paint<'a>(
    definitions: &'a [GroundTransportTrackPiecePresentationDefinition],
    route: &[Vec3],
    index: usize,
) -> Option<(&'a GroundTransportTrackPiecePresentationDefinition, Vec3)> {
    let (heading, turns) = authored_ground_track_turns(route, index);
    if heading % 2 == 1 {
        return None;
    }
    let (appearance, towards_end) = match turns {
        [Some(_), Some(end)] => (end, true),
        [Some(_), None] if route.get(index + 2).is_some() => (0, true),
        [None, Some(_)] if index > 0 => (0, false),
        _ => return None,
    };
    let piece_type = authored_ground_track_piece_type(heading);
    let definition = definitions.iter().find(|definition| {
        definition.piece_type == piece_type && definition.appearance == appearance
    })?;
    let direction = (route[index + 1] - route[index])
        .with_y(0.0)
        .normalize_or_zero();
    let shift = direction * AUTHORED_GROUND_TRACK_TURN_SHIFT_METRES;
    Some((definition, if towards_end { shift } else { -shift }))
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

/// A ground route's points with, at each end where exactly one other route
/// joins it, that route's next point, so a piece at a joint turns towards
/// the road it meets; and the index of the route's first point in them.
fn ground_route_points_with_joined_neighbours(
    route: &TrackSegment,
    routes: &Query<&TrackSegment>,
) -> (Vec<Vec3>, usize) {
    // A station's two ends share its entity, so an end is the pair.
    let neighbour = |junction: (Entity, u8)| {
        let mut joined = routes.iter().filter_map(|candidate| {
            if core::ptr::eq(candidate, route) {
                return None;
            }
            let points = &candidate.path_points;
            if (candidate.from, candidate.from_endpoint_index) == junction {
                points.get(1).copied()
            } else if (candidate.to, candidate.to_endpoint_index) == junction {
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
    let before = neighbour((route.from, route.from_endpoint_index));
    let points = before
        .into_iter()
        .chain(route.path_points.iter().copied())
        .chain(neighbour((route.to, route.to_endpoint_index)))
        .collect();
    (points, usize::from(before.is_some()))
}

/// The decal or elevated strip a ground piece draws, redrawn with it.
#[derive(Component)]
pub(super) struct GroundTrackPieceDecal;

/// Redraws the road pieces next to a joint a route was added to or removed
/// from, so they turn towards (or no longer towards) the road there.
pub(super) fn invalidate_ground_track_pieces_at_changed_joints(
    mut commands: Commands,
    added: Query<(Entity, &TrackSegment), Added<TrackSegment>>,
    mut removed: RemovedComponents<TrackSegment>,
    mut joints: Local<bevy::platform::collections::HashMap<Entity, [Entity; 2]>>,
    routes: Query<(Entity, &TrackSegment)>,
    pieces: Query<(Entity, &TransportTrackConstructionPiece, &Children)>,
    decals: Query<(), With<GroundTrackPieceDecal>>,
) {
    let mut changed = removed
        .read()
        .filter_map(|route| joints.remove(&route))
        .flatten()
        .collect::<HashSet<_>>();
    for (entity, route) in &added {
        joints.insert(entity, [route.from, route.to]);
        changed.extend([route.from, route.to]);
    }
    if changed.is_empty() {
        return;
    }
    let neighbours = routes
        .iter()
        .filter(|(entity, route)| {
            !added.contains(*entity)
                && (changed.contains(&route.from) || changed.contains(&route.to))
        })
        .map(|(entity, _)| entity)
        .collect::<HashSet<_>>();
    for (piece, construction, children) in &pieces {
        if !neighbours.contains(&construction.route_segment) {
            continue;
        }
        for child in children.iter().filter(|child| decals.contains(*child)) {
            commands.entity(child).despawn();
        }
        commands
            .entity(piece)
            .remove::<AuthoredTransportTrackPiecePresentationProjected>();
    }
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
    // The fixed-function shader always reads a second texture coordinate set.
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, texture_coordinates.clone());
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
            // A road end at a station on a slope sits a little off the
            // ground; only a real gap gets the elevated strip.
            // ponytail: fixed 1 m; read a clearance from the track if one exists.
            .is_some_and(|terrain| point.y > terrain.height_m + 1.0)
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_authored_sky_track_rope(
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
    towers: Query<(Entity, &AuthoredLinkedSkyTowerPresentationProjected)>,
    routes: Query<&TrackSegment>,
    mut markers: Query<(&ChildOf, &mut Visibility), With<SkyTowerConstructionMarker>>,
) {
    for (tower, projected) in &towers {
        let open = !routes.iter().any(|route| route.from == tower);
        for (parent, mut visibility) in &mut markers {
            if parent.parent() != projected.root {
                continue;
            }
            let next = if open {
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

/// A sky track's rope, which keeps its colour while its towers and stations
/// are highlighted.
#[derive(Component)]
pub(super) struct SkyTrackRope;

/// A sky station's rope wheels (`link_wheel`, `link_wheel2`), where its ropes
/// turn.
#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct SkyRopeAnchors<'w, 's> {
    stations: Query<'w, 's, (), With<TransportStation>>,
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

/// A road piece's decal over its painted road, shown only while the piece is
/// highlighted.
#[derive(Component)]
pub(super) struct PaintedTrackHighlight;

/// The road a ground piece paints into the terrain (`paintTexture`), and the
/// size of a decal of the whole texture laid over it. Paint textures are
/// authored at the terrain's texel size with their copy window centred, so a
/// 43-texel window sits in 64 texels and an 86-texel one in 128.
fn authored_ground_track_paint(
    presentation: &GroundTransportTrackPiecePresentationDefinition,
    centre: Vec2,
    texture: Handle<Image>,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> Option<(TerrainPaintedTexture, f32)> {
    let start = Vec2::from(presentation.paint_sample_start_texels);
    let copy = Vec2::from(presentation.paint_copy_dimension_texels);
    if copy.cmple(Vec2::ZERO).any() {
        return None;
    }
    let (chunk, _) = terrain_chunks.iter().next()?;
    let presentation_grid = &terrain_assets
        .get(&chunk.asset)?
        .canonical_terrain_grid()
        .presentation;
    // A chunk's surface image also covers its margin cells on each side.
    let texel_metres = f32::from(presentation_grid.surface_margin_cells)
        .mul_add(2.0, f32::from(chunk.side.checked_sub(1)?))
        * chunk.spacing_m
        / f32::from(presentation_grid.surface_resolution);
    let reference = start.x.mul_add(2.0, copy.x).max(1.0).log2().ceil().exp2();
    Some((
        TerrainPaintedTexture {
            centre,
            size_m: copy.x * texel_metres,
            texture,
            uv_min: start / reference,
            uv_max: (start + copy) / reference,
        },
        reference * texel_metres,
    ))
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
    (endpoint_transforms, rope_anchors): (
        Query<&GlobalTransform, With<TransportTrackJunction>>,
        SkyRopeAnchors,
    ),
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
                let tower_world_transform = endpoint_transforms
                    .get(route.to)
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
                let promoted_height_metres = presentation.initial_height_metres.max(
                    route.from_position.y - route.to_position.y
                        + track.sky_rope_clearance_metres.unwrap_or_default(),
                );
                if let Some(tower_world_transform) = tower_world_transform.filter(|_| {
                    existing_sky_towers.get(route.to).is_err()
                        && tower_targets_projected_this_run.insert(route.to)
                }) {
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
                // The rope runs between tower tops, and at a station over the
                // wheel (link_wheel) next to the end it leaves from.
                let start = rope_anchors
                    .station_wheel(route.from, route.path_points[0])
                    .unwrap_or(route.path_points[0] + Vec3::Y * source_height_metres);
                let end = rope_anchors
                    .station_wheel(route.to, route.path_points[1])
                    .unwrap_or(route.path_points[1] + Vec3::Y * promoted_height_metres);
                spawn_authored_sky_track_rope(
                    &mut commands,
                    &mut meshes,
                    material,
                    entity,
                    transform,
                    start,
                    end,
                    presentation,
                );
            }
        } else {
            let (Some(template), Ok(route)) =
                (terrain_decal_pass.as_ref(), routes.get(piece.route_segment))
            else {
                continue;
            };
            let (joined_points, first) = ground_route_points_with_joined_neighbours(route, &routes);
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
            )) + authored_ground_track_paint_shift(
                &joined_points,
                usize::from(piece.piece_index) + first,
            );
            let paint = definitions
                .texture_image(presentation.paint_texture)
                .and_then(|paint_texture| {
                    authored_ground_track_paint(
                        presentation,
                        decal_centre.xz(),
                        paint_texture,
                        &terrain_assets,
                        &terrain_chunks,
                    )
                });
            let decal_size = paint
                .as_ref()
                .map_or(presentation.ground_decal_size_metres[0], |(_, size)| *size);
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
            if let Some((paint, _)) = paint {
                commands.entity(entity).insert(paint);
                let end_paint = authored_ground_track_second_paint(
                    &track.ground_piece_presentations,
                    &joined_points,
                    usize::from(piece.piece_index) + first,
                )
                .and_then(|(end, shift)| {
                    let centre = transform.transform_point(Vec3::new(
                        end.ground_decal_offset_metres[0],
                        0.0,
                        end.ground_decal_offset_metres[1],
                    )) + shift;
                    authored_ground_track_paint(
                        end,
                        centre.xz(),
                        definitions.texture_image(end.paint_texture)?,
                        &terrain_assets,
                        &terrain_chunks,
                    )
                });
                if let Some((end_paint, _)) = end_paint {
                    commands.spawn((
                        end_paint,
                        Transform::IDENTITY,
                        Visibility::Inherited,
                        ChildOf(entity),
                        GroundTrackPieceDecal,
                    ));
                }
                commands
                    .entity(decal)
                    .insert((PaintedTrackHighlight, Visibility::Hidden));
            }
        }
        commands
            .entity(entity)
            .insert(AuthoredTransportTrackPiecePresentationProjected);
    }
}

#[cfg(test)]
mod tests;

/// Shows a road's construction marker only where the road stops at a
/// junction no road leaves or starts at one no road leads into, as the original marks an unfinished road
/// ("Click to place road"); every other piece keeps its markers hidden.
pub(super) fn show_construction_markers_at_unconnected_road_ends(
    pieces: Query<
        (
            &TransportTrackConstructionPiece,
            &Transform,
            &Children,
            &TrackProfile,
        ),
        Without<PrefabAuthoredAttachmentIdentifier>,
    >,
    routes: Query<&TrackSegment>,
    junctions: Query<(), With<TransportTrackJunction>>,
    mut markers: Query<
        (
            &PrefabAuthoredAttachmentIdentifier,
            &mut Visibility,
            &mut Transform,
        ),
        Without<TransportTrackConstructionPiece>,
    >,
) {
    let marker = openzt2_game_data::AssetId::from_key("constructionmarker1");
    let hidden_marker = openzt2_game_data::AssetId::from_key("constructionmarker2");
    for (piece, piece_transform, children, profile) in &pieces {
        // A sky tower carries its own marker on its top.
        let ground = profile.kind == TransportationTrackKind::Ground;
        // A road left after a gap starts at a junction no road leads into,
        // and the original marks that end too.
        let open_end = routes.get(piece.route_segment).ok().and_then(|route| {
            let last = usize::from(piece.piece_index) + 2 == route.path_points.len();
            if !ground {
                None
            } else if last
                && junctions.contains(route.to)
                && !routes.iter().any(|other| other.from == route.to)
            {
                Some(route.to_position)
            } else if piece.piece_index == 0
                && junctions.contains(route.from)
                && !routes.iter().any(|other| other.to == route.from)
            {
                Some(route.from_position)
            } else {
                None
            }
        });
        for child in children {
            let Ok((name, mut visibility, mut transform)) = markers.get_mut(*child) else {
                continue;
            };
            let shown = if name.0 == marker {
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
            } else if name.0 == hidden_marker {
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

/// Whether a station carries the four markers of a sky station.
fn names_in_pairs(
    children: &Query<&Children>,
    markers: &Query<(
        &PrefabAuthoredAttachmentIdentifier,
        &ChildOf,
        &mut Visibility,
        &mut Transform,
    )>,
    station: Entity,
    names: [openzt2_game_data::AssetId; 4],
) -> bool {
    children.iter_descendants(station).any(|entity| {
        markers
            .get(entity)
            .is_ok_and(|(name, ..)| name.0 == names[3])
    })
}

/// A station shows the marker of each end no road is connected to yet, where
/// the original marks the place a road can start ("Click to place road").
#[allow(clippy::type_complexity)]
pub(super) fn show_construction_markers_at_unconnected_station_ends(
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    stations: Query<(Entity, &TransportStation, &GlobalTransform)>,
    routes: Query<&TrackSegment>,
    children: Query<&Children>,
    parents: Query<&GlobalTransform>,
    mut markers: Query<(
        &PrefabAuthoredAttachmentIdentifier,
        &ChildOf,
        &mut Visibility,
        &mut Transform,
    )>,
    rope_anchors: SkyRopeAnchors,
) {
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    // A ground station marks each end with one marker; a sky station marks
    // each rope end with a pair (markers 1-2 and 3-4).
    let names = [
        "constructionmarker1",
        "constructionmarker2",
        "constructionmarker3",
        "constructionmarker4",
    ]
    .map(openzt2_game_data::AssetId::from_key);
    for (station_entity, station, station_transform) in &stations {
        let pair = names_in_pairs(&children, &markers, station_entity, names);
        for descendant in children.iter_descendants(station_entity) {
            let Ok((name, parent, mut visibility, mut transform)) = markers.get_mut(descendant)
            else {
                continue;
            };
            let Some(marker) = (0_u8..)
                .zip(names)
                .find_map(|(index, marker)| (marker == name.0).then_some(index))
            else {
                continue;
            };
            let endpoint = if pair { marker / 2 } else { marker };
            let connected = routes.iter().any(|route| {
                (route.from == station_entity && route.from_endpoint_index == endpoint)
                    || (route.to == station_entity && route.to_endpoint_index == endpoint)
            });
            let open_end = (!connected)
                .then(|| {
                    crate::transport_tours::topology::calculate_authored_transport_station_endpoint_world_position(
                        definitions,
                        station,
                        station_transform,
                        endpoint,
                    )
                })
                .flatten();
            if let (Some(end), Ok(parent_transform)) = (open_end, parents.get(parent.parent())) {
                // ponytail: fixed 1.5 m step out of the building, tune if a station hides it
                let outward = (end - station_transform.translation())
                    .with_y(0.0)
                    .normalize_or_zero();
                // A sky station's pair hangs over its rope wheel, one each side.
                let position = if pair {
                    let side = Vec3::Y.cross(outward) * if marker % 2 == 0 { 1.0 } else { -1.0 };
                    rope_anchors
                        .station_wheel(station_entity, end)
                        .unwrap_or(end)
                        + side
                        // ponytail: fixed lift above the roof, match zt.exe if a station differs
                        + Vec3::Y * 3.0
                } else {
                    end + outward * 1.5
                };
                let local = parent_transform
                    .affine()
                    .inverse()
                    .transform_point3(position);
                if transform.translation != local {
                    transform.translation = local;
                }
            }
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
