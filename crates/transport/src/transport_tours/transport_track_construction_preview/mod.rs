use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionPiece;
use openzt2_world_state::world_spawn::prefab_model_tint::PrefabModelTint;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabRenderable;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_game_data::AssetId;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackProfile;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackSegment;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackJunction;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionPreview;
use crate::transport_tours::transport_track_piece_presentation::PaintedTrackHighlight;
use crate::transport_tours::transport_track_endpoint_queries::find_nearest_compatible_transport_station_endpoint;
use crate::transport_tours::transport_track_endpoint_queries::find_nearest_compatible_transport_track_junction;
use crate::transport_tours::transport_track_endpoint_queries::nearer_transport_track_construction_endpoint;
use crate::transport_tours::transport_track_piece_chain_geometry::build_authored_axis_and_diagonal_transport_track_piece_chain;
use crate::transport_tours::transport_track_piece_chain_geometry::build_projected_authored_axis_or_diagonal_transport_track_piece_chain;
use crate::transport_tours::transport_track_piece_chain_geometry::track_piece_chain_turns_are_gentle;

fn selected_transport_track_definition(
    tool: ConstructionTool,
) -> Option<(AssetId, TransportationTrackKind)> {
    match tool {
        ConstructionTool::GroundTrackPlacement(Some(definition)) => {
            Some((definition, TransportationTrackKind::Ground))
        }
        ConstructionTool::SkyTrackPlacement(Some(definition)) => {
            Some((definition, TransportationTrackKind::Sky))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn begin_or_commit_transport_track_construction_from_primary_pointer_action(
    mut commands: Commands,
    primary_pointer: Res<openzt2_interface_state::input::input_types::PrimaryPointerInputState>,
    ui_capture: Res<UiPointerCapture>,
    tool: Res<ConstructionTool>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    cursors: Query<(&ConstructionCursor, &WorldMember)>,
    stations: Query<(
        Entity,
        &TransportStation,
        &CircuitMember,
        &GlobalTransform,
        &WorldMember,
    )>,
    junctions: Query<(
        Entity,
        &TransportTrackJunction,
        &CircuitMember,
        &GlobalTransform,
        &WorldMember,
    )>,
    segments: Query<(Entity, &CircuitMember, &TrackSegment, &TrackProfile)>,
    previews: Query<(
        Entity,
        &ConstructionPreview,
        &TransportTrackConstructionPreview,
    )>,
    mut commits: MessageWriter<CommitConstruction>,
    mut continue_from: Local<Option<Vec3>>,
    pointer_ray: Res<openzt2_interface_state::camera::world_pointer_ray_types::WorldPointerRay>,
) {
    let Some((definition, kind)) = selected_transport_track_definition(*tool) else {
        *continue_from = None;
        return;
    };
    let (Ok((cursor, world_member)), Some(definitions)) =
        (cursors.single(), active_definitions.get(&definitions))
    else {
        return;
    };
    // As in the original, placing a piece that doesn't end at a station keeps
    // building from its end. The end junction is spawned by the commit, so
    // wait for it before starting the next preview.
    if previews.is_empty() {
        if let Some(end) = *continue_from {
            if let Some(junction) = find_nearest_compatible_transport_track_junction(
                end,
                kind,
                world_member.root,
                None,
                None,
                &junctions,
                &segments,
            ) {
                *continue_from = None;
                spawn_transport_track_construction_preview(
                    &mut commands,
                    definition,
                    junction,
                    *world_member,
                );
            }
            return;
        }
    }
    if !primary_pointer.just_pressed || ui_capture.over_ui {
        return;
    }
    if let Ok((preview_entity, preview, track_preview)) = previews.single() {
        if preview.definition == definition
            && matches!(preview.validity, PlacementValidity::Valid { .. })
        {
            commits.write(CommitConstruction {
                preview: preview_entity,
            });
            *continue_from = track_preview
                .to_station
                .is_none()
                .then_some(track_preview.to_position);
        }
        return;
    }
    let station_endpoint = find_nearest_compatible_transport_station_endpoint(
        cursor.world,
        kind,
        world_member.root,
        None,
        true,
        None,
        definitions,
        &stations,
        &segments,
    );
    let junction_endpoint = find_nearest_compatible_transport_track_junction(
        cursor.world,
        kind,
        world_member.root,
        None,
        pointer_ray.0,
        &junctions,
        &segments,
    );
    let Some(endpoint) = nearer_transport_track_construction_endpoint(
        cursor.world,
        station_endpoint,
        junction_endpoint,
    ) else {
        return;
    };
    spawn_transport_track_construction_preview(&mut commands, definition, endpoint, *world_member);
}

fn spawn_transport_track_construction_preview(
    commands: &mut Commands,
    definition: AssetId,
    (from_endpoint, circuit, from_endpoint_index, from_position): (Entity, Entity, u8, Vec3),
    world_member: WorldMember,
) {
    commands.spawn((
        ConstructionPreview {
            definition,
            transform: Transform::from_translation(from_position),
            validity: PlacementValidity::Pending,
        },
        TransportTrackConstructionPreview {
            circuit,
            from_endpoint,
            from_endpoint_index,
            from_position,
            to_station: None,
            to_endpoint_index: 0,
            to_position: from_position,
            path_points: vec![from_position],
        },
        Transform::from_translation(from_position),
        Visibility::Inherited,
        world_member,
    ));
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update_transport_track_construction_preview_from_pointer_and_station_endpoints(
    tool: Res<ConstructionTool>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    cursors: Query<(&ConstructionCursor, &WorldMember)>,
    stations: Query<(
        Entity,
        &TransportStation,
        &CircuitMember,
        &GlobalTransform,
        &WorldMember,
    )>,
    segments: Query<(Entity, &CircuitMember, &TrackSegment, &TrackProfile)>,
    mut previews: Query<(
        &mut ConstructionPreview,
        &mut TransportTrackConstructionPreview,
        &mut Transform,
    )>,
    terrain: TrackTerrain,
) {
    let Some((selected_definition, selected_kind)) = selected_transport_track_definition(*tool)
    else {
        return;
    };
    let (
        Ok((cursor, world_member)),
        Ok((mut preview, mut track_preview, mut transform)),
        Some(definitions),
    ) = (
        cursors.single(),
        previews.single_mut(),
        active_definitions.get(&definitions),
    )
    else {
        return;
    };
    if preview.definition != selected_definition {
        preview.validity = PlacementValidity::Invalid(PlacementFailure::InvalidTopology);
        return;
    }
    let Some(track_definition) = definitions.find_track(selected_definition) else {
        preview.validity =
            PlacementValidity::Invalid(PlacementFailure::AuthoredRule(selected_definition));
        return;
    };
    let from_position = track_preview.from_position;
    // A road leaves a station along its axis and a road end the way its last
    // piece runs.
    let from_heading = if let Ok((_, _, _, station, _)) = stations.get(track_preview.from_endpoint)
    {
        (from_position - station.translation()).xz().try_normalize()
    } else {
        segments
            .iter()
            .find(|(_, _, segment, _)| segment.to == track_preview.from_endpoint)
            .and_then(|(_, _, segment, _)| {
                let points = &segment.path_points;
                let last = points.len().checked_sub(1)?;
                (points[last] - points[last.checked_sub(1)?])
                    .xz()
                    .try_normalize()
            })
    }
    .filter(|_| selected_kind == TransportationTrackKind::Ground);
    let snapped_station_endpoint = find_nearest_compatible_transport_station_endpoint(
        cursor.world,
        selected_kind,
        world_member.root,
        Some(track_preview.circuit),
        false,
        Some(track_preview.from_endpoint),
        definitions,
        &stations,
        &segments,
    );
    // A road left after a gap starts at a junction nothing leads into; a road
    // can be built back into it, along the way that road leaves.
    let gap_start = segments
        .iter()
        .filter(|(_, member, segment, profile)| {
            member.0 == track_preview.circuit
                && profile.kind == TransportationTrackKind::Ground
                && segment.from != track_preview.from_endpoint
                && !stations.contains(segment.from)
                && !segments
                    .iter()
                    .any(|(_, _, other, _)| other.to == segment.from)
                && segment.from_position.distance(cursor.world) <= GAP_START_SNAP_DISTANCE_METRES
        })
        .min_by(|(_, _, left, _), (_, _, right, _)| {
            left.from_position
                .distance_squared(cursor.world)
                .total_cmp(&right.from_position.distance_squared(cursor.world))
        })
        .and_then(|(_, member, segment, _)| {
            let heading = (*segment.path_points.get(1)? - *segment.path_points.first()?)
                .xz()
                .try_normalize()?;
            Some(((segment.from, member.0, 0, segment.from_position), heading))
        });
    let snapped_station_endpoint = nearer_transport_track_construction_endpoint(
        cursor.world,
        snapped_station_endpoint,
        gap_start.map(|(endpoint, _)| endpoint),
    );
    let snapped_station_chain = snapped_station_endpoint.and_then(|endpoint| {
        build_authored_axis_and_diagonal_transport_track_piece_chain(
            track_definition,
            from_position,
            endpoint.3,
        )
        .map(|chain| (endpoint, chain))
    });
    // ...and enters a station along its axis, never from the side.
    let to_heading = snapped_station_chain
        .as_ref()
        .and_then(|((station, _, _, position), _)| {
            let Ok((_, _, _, transform, _)) = stations.get(*station) else {
                return gap_start.map(|(_, heading)| heading);
            };
            (transform.translation() - *position).xz().try_normalize()
        })
        .filter(|_| selected_kind == TransportationTrackKind::Ground);
    let (to_station, to_endpoint_index, to_position, path_points_and_cost) =
        if let Some(((station, _, endpoint_index, position), chain)) = snapped_station_chain {
            (Some(station), endpoint_index, position, Some(chain))
        } else {
            let chain = build_projected_authored_axis_or_diagonal_transport_track_piece_chain(
                track_definition,
                from_position,
                from_heading,
                cursor.world,
            );
            let projected_position = chain
                .as_ref()
                .and_then(|(points, _)| points.last().copied())
                .unwrap_or(cursor.world);
            (None, 0, projected_position, chain)
        };
    track_preview.to_station = to_station;
    track_preview.to_endpoint_index = to_endpoint_index;
    track_preview.to_position = to_position;
    let midpoint = from_position.midpoint(to_position);
    let direction = to_position - from_position;
    preview.transform = Transform::from_translation(midpoint);
    if direction.xz().length_squared() > f32::EPSILON {
        preview.transform.rotation = Quat::from_rotation_y(direction.x.atan2(direction.z));
    }
    *transform = preview.transform;
    preview.validity = if let Some((mut path_points, cost)) = path_points_and_cost {
        if selected_kind == TransportationTrackKind::Ground {
            drape_ground_level_track_points_on_terrain(
                &mut path_points,
                to_station.is_none(),
                &terrain,
            );
            if let Some(&last) = path_points.last() {
                track_preview.to_position = last;
            }
        }
        // Laying the road on the terrain can make a piece steeper than the
        // track allows; the connection would then be rejected after the
        // click and the piece would vanish, so refuse it here, as the
        // original's `too_steep` placement colour does.
        let valid = crate::transport_tours::transport_fact_validation::authored_transport_track_piece_chain_is_valid(
            track_definition,
            &path_points,
        );
        let gentle = track_piece_chain_turns_are_gentle(from_heading, &path_points, to_heading);
        track_preview.path_points = path_points;
        if !gentle {
            PlacementValidity::Invalid(PlacementFailure::InvalidTopology)
        } else if valid {
            PlacementValidity::Valid { cost }
        } else {
            PlacementValidity::Invalid(PlacementFailure::TooSteep)
        }
    } else {
        track_preview.path_points.clear();
        track_preview.path_points.push(from_position);
        track_preview.path_points.push(to_position);
        PlacementValidity::Invalid(PlacementFailure::InvalidTopology)
    };
}

const GAP_START_SNAP_DISTANCE_METRES: f32 = 8.0;

/// A highlighted road decal's own material, put back afterwards.
#[derive(Component)]
pub(super) struct HighlightedTrackDecalOriginal(Handle<EffectPassMaterial>);

// `ZTGroundTrackMode` in `UI/modes/modes.xml`.
const STATION_START: Color = Color::srgb(0.0, 0.0, 1.0);
const STATION_TARGET: Color = Color::srgb(0.0, 1.0, 0.0);
const PLACE_NO: Color = Color::srgb(200.0 / 255.0, 20.0 / 255.0, 10.0 / 255.0);
const ROAD_END: Color = Color::srgb(0.0, 0.0, 1.0);
const COLOR_ANIMATION_CYCLE_SECONDS: f32 = 0.5;

/// `ZTGroundTrackMode` in `UI/modes/modes.xml` colours the station a road
/// would start from `station_highlight1` (blue) and the station it would
/// connect to `station_highlight2` (green), and blinks the road already
/// joined to the start `endpoint_highlight2` (blue) every `colorAnimationCycle`.
/// With the delete tool it colours the road leg under the cursor red.
// ponytail: the colours and cycle are copied from modes.xml, not lowered from it.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn highlight_transport_track_connection_stations(
    mut commands: Commands,
    time: Res<Time<Real>>,
    tool: Res<ConstructionTool>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    cursors: Query<(
        &ConstructionCursor,
        &WorldMember,
        Option<&openzt2_zoo_state::construction::construction_interaction_types::DeleteHoverTarget>,
    )>,
    stations: Query<(
        Entity,
        &TransportStation,
        &CircuitMember,
        &GlobalTransform,
        &WorldMember,
    )>,
    junctions: Query<(
        Entity,
        &TransportTrackJunction,
        &CircuitMember,
        &GlobalTransform,
        &WorldMember,
    )>,
    segments: Query<(Entity, &CircuitMember, &TrackSegment, &TrackProfile)>,
    previews: Query<(
        Entity,
        &TransportTrackConstructionPreview,
        &ConstructionPreview,
    )>,
    pieces: Query<(Entity, &TransportTrackConstructionPiece)>,
    children: Query<&Children>,
    decals: Query<(
        &MeshMaterial3d<EffectPassMaterial>,
        Option<&HighlightedTrackDecalOriginal>,
        Has<PaintedTrackHighlight>,
    )>,
    mut renderables: Query<Option<&mut PrefabModelTint>, With<PrefabRenderable>>,
    (mut effect_materials, mut shader_buffers, pointer_ray, ropes): (
        ResMut<Assets<EffectPassMaterial>>,
        ResMut<Assets<bevy::render::storage::ShaderBuffer>>,
        Res<openzt2_interface_state::camera::world_pointer_ray_types::WorldPointerRay>,
        Query<(), With<crate::transport_tours::transport_track_piece_presentation::SkyTrackRope>>,
    ),
    mut lit: Local<HashMap<Entity, Color>>,
) {
    let mut wanted = HashMap::new();
    let on = (time.elapsed_secs() / COLOR_ANIMATION_CYCLE_SECONDS).rem_euclid(2.0) < 1.0;
    if let (ConstructionTool::Delete, Ok((_, _, Some(hovered)))) = (*tool, cursors.single()) {
        light_deleted_track_leg(
            hovered.0,
            on,
            &pieces,
            &children,
            &decals,
            &ropes,
            &mut wanted,
        );
    }
    if let (Some((_, kind)), Ok((cursor, world_member, _)), Some(definitions)) = (
        selected_transport_track_definition(*tool),
        cursors.single(),
        active_definitions.get(&definitions),
    ) {
        let preview = previews.single().ok();
        let valid = preview
            .is_none_or(|(_, _, state)| matches!(state.validity, PlacementValidity::Valid { .. }));
        let (start, target) = preview.map_or_else(
            || {
                (
                    endpoint_under_pointer(
                        cursor.world,
                        kind,
                        world_member.root,
                        pointer_ray.0,
                        definitions,
                        &stations,
                        &junctions,
                        &segments,
                    ),
                    None,
                )
            },
            |(_, preview, _)| (Some(preview.from_endpoint), preview.to_station),
        );
        // Station models carry vertex colours the model tint multiplies; the
        // sky tower parts don't, so their materials are tinted instead.
        // A sky station's model has no vertex colours either.
        let by_material = kind == TransportationTrackKind::Sky;
        let mut light_station = |station: Entity, color: Color| {
            for entity in children.iter_descendants(station) {
                if !ropes.contains(entity)
                    && if by_material {
                        decals.contains(entity)
                    } else {
                        renderables.contains(entity)
                    }
                {
                    wanted.insert(entity, color);
                }
            }
        };
        // A sky line's start is its station or its last tower.
        if let Some(start) = start.filter(|start| {
            stations.contains(*start)
                || (kind == TransportationTrackKind::Sky && junctions.contains(*start))
        }) {
            light_station(start, STATION_START);
        }
        let placeable = if valid { STATION_TARGET } else { PLACE_NO };
        if let Some(target) = target {
            light_station(target, placeable);
        }
        // The sky tower being placed shows whether it can stand there.
        if let Some((preview_entity, _, _)) =
            preview.filter(|_| kind == TransportationTrackKind::Sky)
        {
            light_station(preview_entity, placeable);
        }
        // Only a road blinks; a sky line's tower is lit instead.
        if let Some(start) = start.filter(|_| kind == TransportationTrackKind::Ground) {
            let color = if on { ROAD_END } else { Color::WHITE };
            for (piece_entity, piece) in &pieces {
                if segments
                    .get(piece.route_segment)
                    .is_ok_and(|(_, _, route, _)| route.to == start || route.from == start)
                {
                    light_track_decals(piece_entity, color, &children, &decals, &mut wanted);
                }
            }
        }
    }
    for (&entity, &color) in wanted.iter().chain(
        lit.iter()
            .filter(|(entity, _)| !wanted.contains_key(*entity))
            .map(|(entity, _)| (entity, &Color::WHITE)),
    ) {
        if lit.get(&entity) == Some(&color) && wanted.contains_key(&entity) {
            continue;
        }
        apply_highlight_color(
            &mut commands,
            entity,
            color,
            &mut renderables,
            &decals,
            &mut effect_materials,
            &mut shader_buffers,
        );
    }
    *lit = wanted;
}

/// Tints a station model or lights a road decal or tower part `color`.
fn apply_highlight_color(
    commands: &mut Commands,
    entity: Entity,
    color: Color,
    renderables: &mut Query<Option<&mut PrefabModelTint>, With<PrefabRenderable>>,
    decals: &Query<(
        &MeshMaterial3d<EffectPassMaterial>,
        Option<&HighlightedTrackDecalOriginal>,
        Has<PaintedTrackHighlight>,
    )>,
    effect_materials: &mut Assets<EffectPassMaterial>,
    shader_buffers: &mut Assets<bevy::render::storage::ShaderBuffer>,
) {
    if let Ok(current) = renderables.get_mut(entity) {
        match current {
            Some(mut current) => current.0 = color,
            None => {
                commands.entity(entity).try_insert(PrefabModelTint(color));
            }
        }
    } else if let Ok((material, original, painted)) = decals.get(entity) {
        // A painted road shows its decal only while lit.
        if painted {
            commands
                .entity(entity)
                .try_insert(if color == Color::WHITE {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                });
        }
        let material = original.map_or_else(|| material.0.clone(), |original| original.0.clone());
        tint_track_decal(
            commands.entity(entity),
            material,
            color,
            painted,
            effect_materials,
            shader_buffers,
        );
    }
}

/// Lights the road decals of a track piece `color`.
fn light_track_decals(
    piece: Entity,
    color: Color,
    children: &Query<&Children>,
    decals: &Query<(
        &MeshMaterial3d<EffectPassMaterial>,
        Option<&HighlightedTrackDecalOriginal>,
        Has<PaintedTrackHighlight>,
    )>,
    wanted: &mut HashMap<Entity, Color>,
) {
    for child in children.get(piece).into_iter().flatten().copied() {
        if decals.contains(child) {
            wanted.insert(child, color);
        }
    }
}

/// The station end or line end a new line would start from at the pointer.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn endpoint_under_pointer(
    pointer: Vec3,
    kind: TransportationTrackKind,
    world: Entity,
    pointer_ray: Option<Ray3d>,
    definitions: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    stations: &Query<(
        Entity,
        &TransportStation,
        &CircuitMember,
        &GlobalTransform,
        &WorldMember,
    )>,
    junctions: &Query<(
        Entity,
        &TransportTrackJunction,
        &CircuitMember,
        &GlobalTransform,
        &WorldMember,
    )>,
    segments: &Query<(Entity, &CircuitMember, &TrackSegment, &TrackProfile)>,
) -> Option<Entity> {
    let station = find_nearest_compatible_transport_station_endpoint(
        pointer,
        kind,
        world,
        None,
        true,
        None,
        definitions,
        stations,
        segments,
    );
    let junction = find_nearest_compatible_transport_track_junction(
        pointer,
        kind,
        world,
        None,
        pointer_ray,
        junctions,
        segments,
    );
    nearer_transport_track_construction_endpoint(pointer, station, junction)
        .map(|endpoint| endpoint.0)
}

/// `ZTDeleteMode` colours what the delete tool would remove `mouseover`
/// (red), alternating with `alternate` (orange): here the whole road leg the
/// hovered piece belongs to, which is what the sale removes.
fn light_deleted_track_leg(
    hovered: Entity,
    on: bool,
    pieces: &Query<(Entity, &TransportTrackConstructionPiece)>,
    children: &Query<&Children>,
    decals: &Query<(
        &MeshMaterial3d<EffectPassMaterial>,
        Option<&HighlightedTrackDecalOriginal>,
        Has<PaintedTrackHighlight>,
    )>,
    ropes: &Query<
        (),
        With<crate::transport_tours::transport_track_piece_presentation::SkyTrackRope>,
    >,
    wanted: &mut HashMap<Entity, Color>,
) {
    let color = if on {
        Color::srgb(1.0, 0.0, 0.0)
    } else {
        Color::srgb(1.0, 128.0 / 255.0, 0.0)
    };
    // A sky tower: its own parts, not its ropes.
    // A road piece: just that piece, as the delete tool takes one piece out.
    if pieces.contains(hovered) {
        light_track_decals(hovered, color, children, decals, wanted);
        return;
    }
    for part in children.iter_descendants(hovered) {
        if decals.contains(part) && !ropes.contains(part) {
            wanted.insert(part, color);
        }
    }
}

/// Draws a road decal as its texture times `color` (or, `flat`, as `color` in
/// its shape), or as its own material again for white.
fn tint_track_decal(
    mut decal: EntityCommands,
    material: Handle<EffectPassMaterial>,
    color: Color,
    flat: bool,
    effect_materials: &mut Assets<EffectPassMaterial>,
    shader_buffers: &mut Assets<bevy::render::storage::ShaderBuffer>,
) {
    if color == Color::WHITE {
        decal
            .try_insert(MeshMaterial3d(material))
            .try_remove::<HighlightedTrackDecalOriginal>();
        return;
    }
    let rgba = color.to_srgba().to_vec4();
    let Some(tinted) = effect_materials.get(&material).and_then(|source| {
        if flat {
            source.clone_flat_coloured(rgba, shader_buffers)
        } else {
            source.clone_tinted(rgba, shader_buffers)
        }
    }) else {
        return;
    };
    decal.try_insert((
        MeshMaterial3d(effect_materials.add(tinted)),
        HighlightedTrackDecalOriginal(material),
    ));
}

type TrackTerrain<'w, 's> = (
    Res<'w, openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex>,
    Res<
        'w,
        Assets<
            openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
        >,
    >,
    Query<
        'w,
        's,
        (
            &'static openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk,
            Option<&'static openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples>,
        ),
    >,
);

/// A ground road whose ends both sit on the terrain follows the ground
/// between them, as the original paints it into the terrain. Interpolating a
/// straight line instead lifted every piece over a dip above the terrain, so
/// it was drawn as an elevated strip.
fn drape_ground_level_track_points_on_terrain(
    path_points: &mut [Vec3],
    free_end: bool,
    (index, assets, chunks): &TrackTerrain,
) {
    use openzt2_zoo_state::terrain::terrain_world_sampling::{sample_terrain, terrain_chunk_at};
    let ground = |point: Vec3| {
        let (chunk, edited) = chunks.get(terrain_chunk_at(index, point.xz())?).ok()?;
        Some(sample_terrain(chunk, assets.get(&chunk.asset)?, edited, point.xz())?.height_m)
    };
    // An end that isn't snapped to a station is the pointer projected onto
    // the road's axis, so it carries the pointer's height rather than the
    // ground's at its own position.
    if free_end {
        if let Some(height) = path_points.last().and_then(|last| ground(*last)) {
            if let Some(last) = path_points.last_mut() {
                last.y = height;
            }
        }
    }
    drape_points_between_ends(path_points, &ground);
}

fn drape_points_between_ends(path_points: &mut [Vec3], ground: &impl Fn(Vec3) -> Option<f32>) {
    // The original's road tool has no height control: a ground road is always
    // painted onto the terrain between its two ends.
    let inner = path_points.len().saturating_sub(1);
    for point in path_points.iter_mut().take(inner).skip(1) {
        if let Some(height) = ground(*point) {
            point.y = height;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::drape_points_between_ends;
    use bevy::prelude::*;

    #[test]
    fn a_road_follows_the_ground_between_its_ends() {
        // Ground at 8 m with a 2 m dip; the station end sits 1 m higher.
        let ground = |point: Vec3| Some(if point.x.abs() < 1.0 { 6.0 } else { 8.0 });
        let mut points = [-3.0, 0.0, 3.0, 6.0].map(|x| Vec3::new(x, 9.0, 0.0));
        drape_points_between_ends(&mut points, &ground);
        let heights = points.map(|point| point.y);
        assert!(heights
            .iter()
            .zip([9.0, 6.0, 8.0, 9.0])
            .all(|(y, want)| (y - want).abs() < 1e-5));
    }
}
