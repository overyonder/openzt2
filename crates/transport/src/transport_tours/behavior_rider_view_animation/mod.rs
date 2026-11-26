use super::transport_route_position_calculations::TrackTravelDistances;
use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::{
    animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest,
    animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy,
};
use openzt2_zoo_state::{
    behavior_task_execution_types::{
        running_actions::{
            mark_behavior_task_for_failure_and_stop_navigation,
            RunBehaviorAnimateForViewFromTransport, RunningBehaviorInstruction, WhenBehaviorAction,
        },
        BehaviorTaskExecutionState, PendingBehaviorAnimationClipCompletion,
    },
    transport_tours::{
        transport_rider_types::{TransportRider, TransportSeat, TransportSeatRider},
        transport_topology_types::TrackSegment,
        transport_vehicle_types::{RoutePosition, TransportVehicle, VehicleOnTrack},
    },
};

pub(super) fn animate_rider_for_authored_view(
    activation: On<WhenBehaviorAction<RunBehaviorAnimateForViewFromTransport>>,
    riders: Query<(
        &BehaviorTaskExecutionState,
        &RunningBehaviorInstruction,
        &RunBehaviorAnimateForViewFromTransport,
        Option<(&TransportRider, &TransportSeatRider)>,
    )>,
    vehicles: Query<(
        &TransportVehicle,
        &GlobalTransform,
        Option<(&VehicleOnTrack, &RoutePosition)>,
    )>,
    tracks: Query<(&TrackSegment, &TrackTravelDistances)>,
    seats: Query<&TransportSeat>,
    targets: Query<&GlobalTransform>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut playback: MessageWriter<AnimationClipPlaybackRequest>,
    mut commands: Commands,
) {
    let rider = activation.entity;
    let Ok((task, instruction, action, seat_rider)) = riders.get(rider) else {
        return;
    };
    if !instruction.is_current(task) {
        return;
    }
    let clip_key = (|| {
        let target = targets.get(task.target?).ok()?;
        let (ride, seat) = seat_rider?;
        let (vehicle, vehicle_pose, track) = vehicles.get(ride.vehicle).ok()?;
        let seat = seats.get(seat.0).ok()?;
        let definitions = definitions.get(&assets)?;
        let definition = definitions.find_vehicle_seat_by_index(vehicle.definition, seat.index)?;
        let prefix = track
            .and_then(|(track, route)| {
                let (segment, distances) = tracks.get(track.0).ok()?;
                let (position, tangent) =
                    distances.translation_and_tangent(segment, route.distance);
                let vehicle_forward = vehicle_pose.rotation() * Vec3::Z;
                let forward = if tangent.dot(vehicle_forward) < 0.0 {
                    -tangent
                } else {
                    tangent
                };
                let heading = forward.xz().try_normalize()?;
                let offset = target.translation() - position;
                let right = Vec2::new(heading.y, -heading.x);
                let side = offset.xz().dot(right);
                let angle = side.abs().atan2(offset.xz().dot(heading)).to_degrees();
                let vertical_angle = offset.xz().length().atan2(offset.y).to_degrees();
                let first = if side > 0.0 { 2 } else { 0 };
                definition.animation_states.iter().find(|state| {
                    let limit = |index, default| {
                        if state.present_limits & (1_u8 << index) == 0_u8 {
                            default
                        } else {
                            f32::from(state.angle_limits_degrees[index])
                        }
                    };
                    limit(first, 180.0) < angle
                        && angle <= limit(first + 1, 0.0)
                        && state.up_degrees.unwrap_or(0.0) < vertical_angle
                        && vertical_angle <= state.down_degrees.unwrap_or(180.0)
                })
            })
            .map_or("", |state| state.clip_prefix.as_str());
        Some(format!("{prefix}{}", action.0))
    })();
    let Some(clip_key) = clip_key else {
        mark_behavior_task_for_failure_and_stop_navigation(rider, &mut commands);
        return;
    };
    let request_id = playback
        .write(AnimationClipPlaybackRequest {
            animation_subject_entity: rider,
            animation_clip_asset_key: clip_key.clone(),
            blend_duration_milliseconds: 0,
            playback_speed_permille: 1000,
            playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
            action_settings: None,
        })
        .id;
    commands
        .entity(rider)
        .insert(PendingBehaviorAnimationClipCompletion {
            instruction: instruction.id(),
            request_id,
            clip_key,
            target: task.target,
            repeats: false,
        });
}
