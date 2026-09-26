use bevy::prelude::*;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_game_data::AssetId;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_interaction_types::CommitConstruction;
use crate::plugins::construction::construction_interaction_types::ConstructionCursor;
use crate::plugins::construction::construction_interaction_types::ConstructionPreview;
use crate::plugins::construction::construction_interaction_types::PlacementFailure;
use crate::plugins::construction::construction_interaction_types::PlacementValidity;
use crate::plugins::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use crate::plugins::ui::picking::UiPointerCapture;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use super::transport_circuit_types::CircuitMember;
use super::transport_topology_types::TrackProfile;
use super::transport_topology_types::TrackSegment;
use super::transport_topology_types::TransportStation;
use super::transport_topology_types::TransportTrackJunction;
use super::transport_track_construction_types::TransportTrackConstructionPreview;
use super::transport_track_endpoint_queries::find_nearest_compatible_transport_station_endpoint;
use super::transport_track_endpoint_queries::find_nearest_compatible_transport_track_junction;
use super::transport_track_endpoint_queries::nearer_transport_track_construction_endpoint;
use super::transport_track_piece_chain_geometry::build_authored_axis_and_diagonal_transport_track_piece_chain;
use super::transport_track_piece_chain_geometry::build_projected_authored_axis_or_diagonal_transport_track_piece_chain;

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
    primary_pointer: Res<crate::plugins::input::input_types::PrimaryPointerInputState>,
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
    )>,
) {
    let Some((selected_definition, selected_kind)) = selected_transport_track_definition(*tool)
    else {
        return;
    };
    let (Ok((cursor, world_member)), Ok((mut preview, mut track_preview)), Some(definitions)) = (
        cursors.single(),
        previews.single_mut(),
        active_definitions.get(&definitions),
    ) else {
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
    preview.validity = if let Some((path_points, cost)) = path_points_and_cost {
        track_preview.path_points = path_points;
        PlacementValidity::Valid { cost }
    } else {
        track_preview.path_points.clear();
        track_preview.path_points.push(from_position);
        track_preview.path_points.push(to_position);
        PlacementValidity::Invalid(PlacementFailure::InvalidTopology)
    };
}
