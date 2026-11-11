use bevy::prelude::*;
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
use crate::transport_tours::transport_track_endpoint_queries::find_nearest_compatible_transport_station_endpoint;
use crate::transport_tours::transport_track_endpoint_queries::find_nearest_compatible_transport_track_junction;
use crate::transport_tours::transport_track_endpoint_queries::nearer_transport_track_construction_endpoint;
use crate::transport_tours::transport_track_piece_chain_geometry::build_authored_axis_and_diagonal_transport_track_piece_chain;
use crate::transport_tours::transport_track_piece_chain_geometry::build_projected_authored_axis_or_diagonal_transport_track_piece_chain;

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
) {
    if !primary_pointer.just_pressed || ui_capture.over_ui {
        return;
    }
    let Some((definition, kind)) = selected_transport_track_definition(*tool) else {
        return;
    };
    if let Ok((preview_entity, preview, _)) = previews.single() {
        if preview.definition == definition
            && matches!(preview.validity, PlacementValidity::Valid { .. })
        {
            commits.write(CommitConstruction {
                preview: preview_entity,
            });
        }
        return;
    }
    let (Ok((cursor, world_member)), Some(definitions)) =
        (cursors.single(), active_definitions.get(&definitions))
    else {
        return;
    };
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
        &junctions,
        &segments,
    );
    let Some((from_endpoint, circuit, from_endpoint_index, from_position)) =
        nearer_transport_track_construction_endpoint(
            cursor.world,
            station_endpoint,
            junction_endpoint,
        )
    else {
        return;
    };
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
        *world_member,
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
    let snapped_station_chain = snapped_station_endpoint.and_then(|endpoint| {
        build_authored_axis_and_diagonal_transport_track_piece_chain(
            track_definition,
            from_position,
            endpoint.3,
        )
        .map(|chain| (endpoint, chain))
    });
    let (to_station, to_endpoint_index, to_position, path_points_and_cost) =
        if let Some(((station, _, endpoint_index, position), chain)) = snapped_station_chain {
            (Some(station), endpoint_index, position, Some(chain))
        } else {
            let chain = build_projected_authored_axis_or_diagonal_transport_track_piece_chain(
                track_definition,
                from_position,
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
            drape_ground_level_track_points_on_terrain(&mut path_points, &terrain);
        }
        track_preview.path_points = path_points;
        PlacementValidity::Valid { cost }
    } else {
        track_preview.path_points.clear();
        track_preview.path_points.push(from_position);
        track_preview.path_points.push(to_position);
        PlacementValidity::Invalid(PlacementFailure::InvalidTopology)
    };
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
    (index, assets, chunks): &TrackTerrain,
) {
    use openzt2_zoo_state::terrain::terrain_world_sampling::{sample_terrain, terrain_chunk_at};
    let ground = |point: Vec3| {
        let (chunk, edited) = chunks.get(terrain_chunk_at(index, point.xz())?).ok()?;
        Some(sample_terrain(chunk, assets.get(&chunk.asset)?, edited, point.xz())?.height_m)
    };
    drape_points_between_grounded_ends(path_points, &ground);
}

fn drape_points_between_grounded_ends(
    path_points: &mut [Vec3],
    ground: &impl Fn(Vec3) -> Option<f32>,
) {
    const GROUNDED_TOLERANCE_METRES: f32 = 0.05;
    let grounded = |point: Vec3| {
        ground(point).is_some_and(|height| (point.y - height).abs() <= GROUNDED_TOLERANCE_METRES)
    };
    let (Some(&first), Some(&last)) = (path_points.first(), path_points.last()) else {
        return;
    };
    if path_points.len() < 3 || !grounded(first) || !grounded(last) {
        return;
    }
    let inner = path_points.len() - 1;
    for point in &mut path_points[1..inner] {
        if let Some(height) = ground(*point) {
            point.y = height;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::drape_points_between_grounded_ends;
    use bevy::prelude::*;

    #[test]
    fn a_grounded_road_follows_a_dip_but_a_raised_one_keeps_its_line() {
        // Ground at 8 m with a 2 m dip in the middle.
        let ground = |point: Vec3| Some(if point.x.abs() < 1.0 { 6.0 } else { 8.0 });
        let line = |end_y: f32| {
            [-3.0, 0.0, 3.0].map(|x| Vec3::new(x, 8.0 + (end_y - 8.0) * (x + 3.0) / 6.0, 0.0))
        };
        let mut grounded = line(8.0);
        drape_points_between_grounded_ends(&mut grounded, &ground);
        assert!(grounded
            .iter()
            .zip([8.0, 6.0, 8.0])
            .all(|(point, y)| (point.y - y).abs() < 1e-5));
        let mut raised = line(11.0);
        drape_points_between_grounded_ends(&mut raised, &ground);
        assert!(raised
            .iter()
            .zip([8.0, 9.5, 11.0])
            .all(|(point, y)| (point.y - y).abs() < 1e-5));
    }
}
