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
        transport_vehicle_types::TransportVehicle,
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
    vehicles: Query<(&TransportVehicle, &GlobalTransform)>,
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
        let (vehicle, vehicle_pose) = vehicles.get(ride.vehicle).ok()?;
        let seat = seats.get(seat.0).ok()?;
        let definitions = definitions.get(&assets)?;
        let definition = definitions.find_vehicle_seat_by_index(vehicle.definition, seat.index)?;
        let offset =
            vehicle_pose.rotation().inverse() * (target.translation() - vehicle_pose.translation());
        let offset = if seat.back_facing { -offset } else { offset };
        let degrees = offset.x.atan2(offset.z).to_degrees();
        let first = if degrees > 0.0 { 2 } else { 0 };
        let angle = degrees.abs();
        let prefix = definition
            .animation_states
            .iter()
            .find(|state| {
                state.present_limits & (0b11 << first) == 0b11 << first
                    && f32::from(state.angle_limits_degrees[first]) <= angle
                    && angle < f32::from(state.angle_limits_degrees[first + 1])
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
