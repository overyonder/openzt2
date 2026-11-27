use openzt2_zoo_state::locomotion::locomotion_types::SpatialCell;
use openzt2_zoo_state::transport_tours::transport_topology_types::{TrackStart, TrackEnd, TrackSegment};
use openzt2_construction::construction::transport_spatial_picking::TransportSpatialPicking;
use super::ground_track_policy::TrackTerrain;
use openzt2_zoo_state::transport_tours::transport_highlight_style::TransportHighlightStyle;
use openzt2_zoo_state::transport_tours::transport_pick_bounds::TransportPickBounds;
use openzt2_zoo_state::transport_tours::transport_topology_types::{TrackRoute, TrackIncidence};
use bevy::picking::pointer::{PointerButton, PointerInput};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::{TransportTrackConstructionPiece, TrackPieces};
use openzt2_world_state::world_spawn::prefab_model_tint::PrefabModelTint;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabRenderable;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_game_data::AssetId;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitted;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackProfile;
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

#[derive(Component)]
pub(super) struct ContinueTransportTrackFrom(pub(super) Entity);

pub(super) fn continue_track_preview_from_committed_junction(
    mut committed: MessageReader<ConstructionCommitted>,
    mut commands: Commands,
    tool: Res<ConstructionTool>,
    continuations: Query<&ContinueTransportTrackFrom>,
    tracks: Query<(TrackRoute, &TrackProfile, &CircuitMember, &WorldMember)>,
    junctions: Query<(&GlobalTransform, &WorldMember), With<TransportTrackJunction>>,
    previews: Query<(), With<TransportTrackConstructionPreview>>,
    cursors: Query<&WorldMember, With<ConstructionCursor>>,
) {
    let mut resume = previews.is_empty();
    for commit in committed.read() {
        let Ok(continuation) = continuations.get(commit.transaction) else {
            continue;
        };
        commands
            .entity(commit.transaction)
            .remove::<ContinueTransportTrackFrom>();
        let (true, Some((definition, kind)), Ok(world)) = (
            resume,
            selected_transport_track_definition(*tool),
            cursors.single(),
        ) else {
            continue;
        };
        let Ok((track, profile, circuit, track_world)) = tracks.get(continuation.0) else {
            continue;
        };
        let Ok((junction, junction_world)) = junctions.get(track.to()) else {
            continue;
        };
        if track_world.root != world.root
            || junction_world.root != world.root
            || track.definition != definition
            || profile.kind != kind
        {
            continue;
        }
        spawn_transport_track_construction_preview(
            &mut commands,
            definition,
            (track.to(), circuit.0, 0, junction.translation()),
            *world,
        );
        resume = false;
    }
}

type TrackSocket = (Entity, Entity, u8, Vec3);

#[derive(Component, Default, PartialEq)]
pub(super) struct TransportPointerEndpoints {
    start: Option<TrackSocket>,
    destination: Option<TrackSocket>,
    gap_heading: Option<Vec2>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn resolve_transport_pointer_endpoints(
    mut commands: Commands,
    tool: Res<ConstructionTool>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    mut cursor: Query<(
        Entity,
        Ref<ConstructionCursor>,
        &WorldMember,
        Option<&mut TransportPointerEndpoints>,
    )>,
    (pointer, bounds, spatial): (
        Res<openzt2_interface_state::camera::world_pointer_ray_types::WorldPointerRay>,
        TransportPickBounds,
        TransportSpatialPicking,
    ),
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
    segments: Query<(Entity, &CircuitMember, TrackRoute, &TrackProfile)>,
    incidence: TrackIncidence,
    previews: Query<&TransportTrackConstructionPreview>,
    previous: Query<(), Added<TransportTrackConstructionPreview>>,
    mut removed: RemovedComponents<TransportTrackConstructionPreview>,
    (changed_routes, changed_endpoints): (
        Query<
            (),
            Or<(
                Changed<TrackStart>,
                Changed<TrackEnd>,
                Changed<TrackSegment>,
            )>,
        >,
        Query<
            (),
            (
                Or<(With<TransportStation>, With<TransportTrackJunction>)>,
                Or<(
                    Added<CircuitMember>,
                    Changed<GlobalTransform>,
                    Changed<SpatialCell>,
                )>,
            ),
        >,
    ),
    mut discarded: MessageReader<
        super::transport_track_piece_presentation::DiscardedTrackJointEndpoints,
    >,
) {
    let route_removed = discarded.read().count() != 0;
    let preview_removed = removed.read().count() != 0;
    let Ok((cursor_entity, cursor, world, current)) = cursor.single_mut() else {
        return;
    };
    let Some((_, kind)) = selected_transport_track_definition(*tool) else {
        if current.is_some() {
            commands
                .entity(cursor_entity)
                .remove::<TransportPointerEndpoints>();
        }
        return;
    };
    if current.is_some()
        && !cursor.is_changed()
        && !pointer.is_changed()
        && !tool.is_changed()
        && !active.is_changed()
        && !definitions.is_changed()
        && previous.is_empty()
        && changed_routes.is_empty()
        && changed_endpoints.is_empty()
        && !route_removed
        && !preview_removed
    {
        return;
    }
    let Some(definitions) = active.get(&definitions) else {
        return;
    };
    let preview = previews.single().ok();
    let mut candidates = spatial.nearby(world.root, cursor.world.xz(), 8.0);
    if let Some(ray) = pointer.0 {
        spatial.extend_along_ray(world.root, ray, 8.0, &mut candidates);
    }
    let station = find_nearest_compatible_transport_station_endpoint(
        cursor.world,
        kind,
        world.root,
        None,
        preview.is_none(),
        preview.map(|preview| preview.from_endpoint),
        definitions,
        &candidates,
        &stations,
        &segments,
        &incidence,
        pointer.0,
        &bounds,
    );
    let next = if let Some(preview) = preview {
        let gap = (kind == TransportationTrackKind::Ground)
            .then(|| {
                junctions
                    .iter_many(candidates.iter().copied())
                    .filter_map(Result::ok)
                    .filter(|(entity, _, _, _, _)| {
                        *entity != preview.from_endpoint
                            && incidence.incoming(*entity).next().is_none()
                    })
                    .flat_map(|(entity, _, _, _, _)| incidence.outgoing(entity))
                    .filter_map(|route| segments.get(route).ok())
                    .filter(|(_, _, segment, profile)| {
                        profile.kind == kind
                            && segment.from_position.distance(cursor.world)
                                <= GAP_START_SNAP_DISTANCE_METRES
                    })
                    .min_by(|(_, _, left, _), (_, _, right, _)| {
                        left.from_position
                            .distance_squared(cursor.world)
                            .total_cmp(&right.from_position.distance_squared(cursor.world))
                    })
                    .and_then(|(_, member, segment, _)| {
                        let heading = (*segment.path_points.get(1)?
                            - *segment.path_points.first()?)
                        .xz()
                        .try_normalize()?;
                        Some((
                            (segment.from(), member.0, 0, segment.from_position),
                            heading,
                        ))
                    })
            })
            .flatten();
        let destination = nearer_transport_track_construction_endpoint(
            cursor.world,
            station,
            gap.map(|(socket, _)| socket),
        );
        let gap_heading = gap
            .filter(|(socket, _)| Some(*socket) == destination)
            .map(|(_, heading)| heading);
        TransportPointerEndpoints {
            start: None,
            destination,
            gap_heading,
        }
    } else {
        let junction = find_nearest_compatible_transport_track_junction(
            cursor.world,
            kind,
            world.root,
            None,
            pointer.0,
            &candidates,
            &junctions,
            &segments,
            &incidence,
            &bounds,
        );
        TransportPointerEndpoints {
            start: nearer_transport_track_construction_endpoint(cursor.world, station, junction),
            ..default()
        }
    };
    if let Some(mut current) = current {
        current.set_if_neq(next);
    } else {
        commands.entity(cursor_entity).insert(next);
    }
}

pub(super) fn begin_or_commit_transport_track_construction_from_primary_pointer_action(
    mut commands: Commands,
    mut pointer_inputs: MessageReader<PointerInput>,
    ui_capture: Res<UiPointerCapture>,
    tool: Res<ConstructionTool>,
    cursors: Query<(&TransportPointerEndpoints, &WorldMember), With<ConstructionCursor>>,
    previews: Query<(Entity, &ConstructionPreview), With<TransportTrackConstructionPreview>>,
    mut commits: MessageWriter<CommitConstruction>,
) {
    let primary = pointer_inputs.read().any(|input| {
        input.pointer_id.is_mouse() && input.button_just_pressed(PointerButton::Primary)
    });
    pointer_inputs.clear();
    let Some((definition, _)) = selected_transport_track_definition(*tool) else {
        return;
    };
    if !primary || ui_capture.over_ui {
        return;
    }
    if let Ok((preview_entity, preview)) = previews.single() {
        if preview.definition == definition
            && matches!(preview.validity, PlacementValidity::Valid { .. })
        {
            commits.write(CommitConstruction {
                preview: preview_entity,
            });
        }
        return;
    }
    if let Ok((hover, world)) = cursors.single() {
        if let Some(endpoint) = hover.start {
            spawn_transport_track_construction_preview(&mut commands, definition, endpoint, *world);
        }
    }
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
            tower_height_m: None,
        },
        Transform::from_translation(from_position),
        Visibility::Inherited,
        world_member,
    ));
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update_transport_track_construction_preview_from_pointer_and_station_endpoints(
    tool: Res<ConstructionTool>,
    policy: Res<openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    cursors: Query<(
        Ref<ConstructionCursor>,
        &WorldMember,
        Ref<TransportPointerEndpoints>,
    )>,
    stations: Query<(
        Entity,
        &TransportStation,
        &CircuitMember,
        &GlobalTransform,
        &WorldMember,
    )>,
    segments: Query<(Entity, &CircuitMember, TrackRoute, &TrackProfile)>,
    incidence: TrackIncidence,
    mut previews: Query<(
        &mut ConstructionPreview,
        &mut TransportTrackConstructionPreview,
        &mut Transform,
    )>,
    terrain: TrackTerrain,
    mut terrain_changed: MessageReader<
        openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged,
    >,
) {
    let Some((selected_definition, selected_kind)) = selected_transport_track_definition(*tool)
    else {
        return;
    };
    let grade_changed = terrain_changed.read().next().is_some();
    terrain_changed.clear();
    let changed = tool.is_changed()
        || policy.is_changed()
        || definitions.is_changed()
        || active_definitions.is_changed()
        || grade_changed;
    let (
        Ok((cursor, world_member, hover)),
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
    if !changed && !cursor.is_changed() && !hover.is_changed() && !track_preview.is_added() {
        return;
    }
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
        incidence
            .incoming(track_preview.from_endpoint)
            .find_map(|entity| segments.get(entity).ok())
            .and_then(|(_, _, segment, _)| {
                let points = &segment.path_points;
                let last = points.len().checked_sub(1)?;
                (points[last] - points[last.checked_sub(1)?])
                    .xz()
                    .try_normalize()
            })
    }
    .filter(|_| selected_kind == TransportationTrackKind::Ground);
    let snapped_station_endpoint = hover.destination;
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
                return hover.gap_heading;
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
    let midpoint = from_position.midpoint(to_position);
    let direction = to_position - from_position;
    let mut preview_transform = Transform::from_translation(midpoint);
    if direction.xz().length_squared() > f32::EPSILON {
        preview_transform.rotation = Quat::from_rotation_y(direction.x.atan2(direction.z));
    }
    let (path_points, to_position, validity) = if let Some((mut path_points, cost)) =
        path_points_and_cost
    {
        let mut endpoint_position = to_position;
        if selected_kind == TransportationTrackKind::Ground {
            drape_ground_level_track_points_on_terrain(
                &mut path_points,
                world_member.root,
                to_station.is_none(),
                &terrain,
            );
            if let Some(&last) = path_points.last() {
                endpoint_position = last;
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
        let across = selected_kind != TransportationTrackKind::Ground
            || track_definition
                .ground_track_physics
                .as_ref()
                .is_none_or(|ground| {
                    terrain.across_grade_allowed(
                        world_member.root,
                        &path_points,
                        [
                            stations.contains(track_preview.from_endpoint),
                            to_station.is_some_and(|to| stations.contains(to)),
                        ],
                        ground,
                    )
                });
        let gentle = track_piece_chain_turns_are_gentle(from_heading, &path_points, to_heading);
        let validity = if !gentle {
            PlacementValidity::Invalid(PlacementFailure::InvalidTopology)
        } else if valid && across {
            PlacementValidity::Valid { cost }
        } else {
            PlacementValidity::Invalid(PlacementFailure::TooSteep)
        };
        (path_points, endpoint_position, validity)
    } else {
        (
            vec![from_position, to_position],
            to_position,
            PlacementValidity::Invalid(PlacementFailure::InvalidTopology),
        )
    };
    let tower_height_m = (selected_kind == TransportationTrackKind::Sky && to_station.is_none())
        .then(|| {
            super::sky_tower_navigation::authored_sky_tower_height(
                track_definition,
                from_position,
                to_position,
                policy.sky_tower_height_steps,
            )
        })
        .flatten();
    if track_preview.to_station != to_station
        || track_preview.to_endpoint_index != to_endpoint_index
        || track_preview.to_position != to_position
        || track_preview.path_points != path_points
        || track_preview.tower_height_m != tower_height_m
    {
        track_preview.to_station = to_station;
        track_preview.to_endpoint_index = to_endpoint_index;
        track_preview.to_position = to_position;
        track_preview.path_points = path_points;
        track_preview.tower_height_m = tower_height_m;
    }
    if preview.transform != preview_transform || preview.validity != validity {
        preview.transform = preview_transform;
        preview.validity = validity;
    }
    transform.set_if_neq(preview_transform);
}

const GAP_START_SNAP_DISTANCE_METRES: f32 = 8.0;

/// A highlighted road decal's own material, put back afterwards.
#[derive(Component)]
pub(super) struct HighlightedTrackDecalOriginal(Handle<EffectPassMaterial>);

#[derive(Component)]
pub(super) struct TrackHighlight(Option<Color>);

#[derive(Component)]
pub(super) struct OriginalModelTint(Option<Color>);

#[derive(Resource, Default)]
pub(super) struct TransportHighlightBlink([Timer; 2]);

pub(super) fn tick_transport_highlight_blink(
    time: Res<Time<Real>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    mut style: ResMut<TransportHighlightStyle>,
    mut blink: ResMut<TransportHighlightBlink>,
) {
    if active.is_changed() || definitions.is_changed() {
        let mut next = TransportHighlightStyle::default();
        if let Some(policy) = active
            .get(&definitions)
            .and_then(|definitions| definitions.transport_highlight())
        {
            let rgba = |[r, g, b, a]: [u8; 4]| Color::srgba_u8(r, g, b, a);
            if let Some(track) = policy.track {
                next.start = track.station.map(rgba);
                next.target = rgba(track.place[0]);
                next.invalid = rgba(track.place[1]);
                next.steep = rgba(track.steep);
                next.open_end = track.endpoint.map(rgba);
                if let Ok(period) =
                    std::time::Duration::try_from_secs_f32(track.color_interval_seconds * 2.0)
                {
                    next.blink_period = period;
                }
            }
            if let Some(deletion) = policy.deletion {
                next.delete = deletion.colors.map(rgba);
                if let Ok(period) =
                    std::time::Duration::try_from_secs_f32(deletion.color_interval_seconds * 2.0)
                {
                    next.deletion_blink_period = period;
                }
            }
        }
        style.set_if_neq(next);
    }
    let changed = style.is_changed();
    for (timer, period) in blink
        .0
        .iter_mut()
        .zip([style.blink_period, style.deletion_blink_period])
    {
        if changed {
            *timer = Timer::new(period, TimerMode::Repeating);
            timer.tick(time.elapsed());
        } else {
            timer.tick(time.delta());
        }
    }
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn highlight_transport_track_connection_stations(
    mut commands: Commands,
    (blink, style): (Res<TransportHighlightBlink>, Res<TransportHighlightStyle>),
    tool: Res<ConstructionTool>,
    cursors: Query<
        (
            Option<&TransportPointerEndpoints>,
            Option<
                &openzt2_zoo_state::construction::construction_interaction_types::DeleteHoverTarget,
            >,
        ),
        With<ConstructionCursor>,
    >,
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
    incidence: TrackIncidence,
    previews: Query<(
        Entity,
        &TransportTrackConstructionPreview,
        &ConstructionPreview,
    )>,
    (pieces, route_pieces, children, decals, renderables, ropes): (
        Query<(), With<TransportTrackConstructionPiece>>,
        Query<&TrackPieces>,
        Query<&Children>,
        Query<(), With<MeshMaterial3d<EffectPassMaterial>>>,
        Query<(), With<PrefabRenderable>>,
        Query<(), With<crate::transport_tours::transport_track_piece_presentation::SkyTrackRope>>,
    ),
    mut highlights: Query<(Entity, &mut TrackHighlight)>,
) {
    let mut wanted = HashMap::new();
    let track_color = usize::from(blink.0[0].fraction() >= 0.5);
    let deletion_color = usize::from(blink.0[1].fraction() >= 0.5);
    if let (ConstructionTool::Delete, Ok((_, Some(hovered)))) = (*tool, cursors.single()) {
        light_deleted_track_leg(
            hovered.0,
            style.delete[deletion_color],
            &pieces,
            &children,
            &decals,
            &ropes,
            &mut wanted,
        );
    }
    if let (Some((_, kind)), Ok((hover, _))) =
        (selected_transport_track_definition(*tool), cursors.single())
    {
        let preview = previews.single().ok();
        let valid = preview
            .is_none_or(|(_, _, state)| matches!(state.validity, PlacementValidity::Valid { .. }));
        let (start, target) = preview.map_or_else(
            || {
                (
                    hover.and_then(|hover| hover.start.map(|socket| socket.0)),
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
            light_station(start, style.start[track_color]);
        }
        let placeable = if valid { style.target } else { style.invalid };
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
            let color = style.open_end[track_color];
            for piece in incidence
                .incident(start)
                .filter_map(|route| route_pieces.get(route).ok())
                .flat_map(|pieces| pieces.iter())
            {
                light_track_decals(piece, color, &children, &decals, &mut wanted);
            }
        }
    }
    for (entity, mut highlight) in &mut highlights {
        let color = wanted.remove(&entity);
        if highlight.0 != color {
            highlight.0 = color;
        }
    }
    for (entity, color) in wanted {
        commands
            .entity(entity)
            .try_insert(TrackHighlight(Some(color)));
    }
}

pub(super) fn apply_changed_track_highlights(
    mut commands: Commands,
    mut renderables: Query<
        (
            Entity,
            &TrackHighlight,
            Option<&mut PrefabModelTint>,
            Option<&OriginalModelTint>,
        ),
        (With<PrefabRenderable>, Changed<TrackHighlight>),
    >,
    mut decals: Query<
        (
            Entity,
            &TrackHighlight,
            &mut MeshMaterial3d<EffectPassMaterial>,
            Option<&HighlightedTrackDecalOriginal>,
            Has<PaintedTrackHighlight>,
            Option<&mut Visibility>,
        ),
        (Without<PrefabRenderable>, Changed<TrackHighlight>),
    >,
    mut effect_materials: ResMut<Assets<EffectPassMaterial>>,
    mut shader_buffers: ResMut<Assets<bevy::render::storage::ShaderBuffer>>,
) {
    for (entity, highlight, tint, original) in &mut renderables {
        if let Some(color) = highlight.0 {
            if original.is_none() {
                commands
                    .entity(entity)
                    .insert(OriginalModelTint(tint.as_deref().map(|tint| tint.0)));
            }
            if let Some(mut tint) = tint {
                if tint.0 != color {
                    tint.0 = color;
                }
            } else {
                commands.entity(entity).insert(PrefabModelTint(color));
            }
        } else {
            if let Some((mut tint, color)) = tint.zip(original.and_then(|original| original.0)) {
                tint.0 = color;
            } else {
                commands.entity(entity).remove::<PrefabModelTint>();
            }
            commands
                .entity(entity)
                .remove::<(TrackHighlight, OriginalModelTint)>();
        }
    }
    for (entity, highlight, mut material, original, painted, visibility) in &mut decals {
        let color = highlight.0.filter(|&color| color != Color::WHITE);
        if let Some(color) = color {
            let source = original.map_or_else(|| material.0.clone(), |original| original.0.clone());
            let rgba = color.to_srgba().to_vec4();
            let tinted = effect_materials.get(&source).and_then(|asset| {
                if painted {
                    asset.clone_flat_coloured(rgba, &mut shader_buffers)
                } else {
                    asset.clone_tinted(rgba, &mut shader_buffers)
                }
            });
            if let Some(tinted) = tinted {
                material.0 = effect_materials.add(tinted);
                if original.is_none() {
                    commands
                        .entity(entity)
                        .insert(HighlightedTrackDecalOriginal(source));
                }
            }
        } else if let Some(original) = original {
            material.0 = original.0.clone();
            commands
                .entity(entity)
                .remove::<HighlightedTrackDecalOriginal>();
        }
        if painted {
            if let Some(mut visibility) = visibility {
                visibility.set_if_neq(if color.is_some() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
        if highlight.0.is_none() {
            commands.entity(entity).remove::<TrackHighlight>();
        }
    }
}

/// Lights the road decals of a track piece `color`.
fn light_track_decals(
    piece: Entity,
    color: Color,
    children: &Query<&Children>,
    decals: &Query<(), With<MeshMaterial3d<EffectPassMaterial>>>,
    wanted: &mut HashMap<Entity, Color>,
) {
    for child in children.get(piece).into_iter().flatten().copied() {
        if decals.contains(child) {
            wanted.insert(child, color);
        }
    }
}

/// `ZTDeleteMode` colours what the delete tool would remove `mouseover`
/// (red), alternating with `alternate` (orange): here the whole road leg the
/// hovered piece belongs to, which is what the sale removes.
fn light_deleted_track_leg(
    hovered: Entity,
    color: Color,
    pieces: &Query<(), With<TransportTrackConstructionPiece>>,
    children: &Query<&Children>,
    decals: &Query<(), With<MeshMaterial3d<EffectPassMaterial>>>,
    ropes: &Query<
        (),
        With<crate::transport_tours::transport_track_piece_presentation::SkyTrackRope>,
    >,
    wanted: &mut HashMap<Entity, Color>,
) {
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

/// A ground road whose ends both sit on the terrain follows the ground
/// between them, as the original paints it into the terrain. Interpolating a
/// straight line instead lifted every piece over a dip above the terrain, so
/// it was drawn as an elevated strip.
fn drape_ground_level_track_points_on_terrain(
    path_points: &mut [Vec3],
    world_root: Entity,
    free_end: bool,
    terrain: &TrackTerrain,
) {
    let ground = |point: Vec3| terrain.height_at(world_root, point.xz());
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
