use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::camera::camera_runtime_state_types::ZooCamera;

use super::types::{
    AimTranquilizer, FireTranquilizer, TranquilizeRequest, TranquilizerFireOutcome,
    TranquilizerFired, TranquilizerHud, TranquilizerMisfireFeedback, TranquilizerReticle,
    TranquilizerTool,
};

fn calculate_optional_tranquilizer_target_distance_and_range(
    source_position: Option<[f32; 3]>,
    target_position: Option<[f32; 3]>,
    maximum_range_m: f32,
) -> (f32, bool) {
    let Some((source_position, target_position)) = source_position.zip(target_position) else {
        return (0.0, true);
    };
    let distance_m = target_position
        .into_iter()
        .zip(source_position)
        .map(|(target_coordinate, source_coordinate)| {
            (target_coordinate - source_coordinate).powi(2)
        })
        .sum::<f32>()
        .sqrt();
    (distance_m, distance_m <= maximum_range_m)
}

fn select_tranquilizer_fire_outcome(
    tool: &TranquilizerTool,
    target_is_in_range: bool,
) -> TranquilizerFireOutcome {
    if tool.charge_points < tool.required_charge_points {
        TranquilizerFireOutcome::Misfire
    } else if tool.target.is_none() {
        TranquilizerFireOutcome::NoTarget
    } else if !target_is_in_range {
        TranquilizerFireOutcome::OutOfRange
    } else {
        TranquilizerFireOutcome::Shot
    }
}

pub(super) fn project_tranquilizer_target_distance_charge_and_reticle_state(
    mut commands: Commands,
    mut aim_requests: MessageReader<AimTranquilizer>,
    time: Res<Time>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    transforms: Query<&GlobalTransform>,
    cameras: Query<&GlobalTransform, With<ZooCamera>>,
    mut tools: Query<(&TranquilizerTool, Option<&mut TranquilizerMisfireFeedback>)>,
) {
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    let Some(mode) = definitions.tranquilizer_mode() else {
        return;
    };
    for aim_request in aim_requests.read() {
        let Ok((tool, misfire_feedback)) = tools.get_mut(aim_request.controller) else {
            continue;
        };
        if definitions.find_tranquilizer(tool.tranquilizer).is_none() {
            continue;
        }
        let camera_position = cameras
            .single()
            .ok()
            .map(|transform| transform.translation().to_array());
        let target_position = tool
            .target
            .and_then(|entity| transforms.get(entity).ok())
            .map(|transform| transform.translation().to_array());
        let (distance_m, measured_in_range) =
            calculate_optional_tranquilizer_target_distance_and_range(
                camera_position,
                target_position,
                mode.range_cm as f32 / 100.0,
            );
        let target = tool.target.filter(|_| target_position.is_some());
        let in_range = camera_position.is_some() && target.is_some() && measured_in_range;
        let ready = tool.charge_points >= tool.required_charge_points;
        let reticle = match (target.is_some(), in_range, ready) {
            (false, _, _) => TranquilizerReticle::NoTarget,
            (true, false, _) => TranquilizerReticle::OutOfRange,
            (true, true, false) => TranquilizerReticle::Charging,
            (true, true, true) => TranquilizerReticle::Ready,
        };
        let mut misfire_finished = false;
        let misfire_visible = if let Some(mut feedback) = misfire_feedback {
            feedback.0.tick(time.delta());
            misfire_finished = feedback.0.is_finished();
            !misfire_finished
        } else {
            false
        };
        if misfire_finished {
            commands
                .entity(aim_request.controller)
                .remove::<TranquilizerMisfireFeedback>();
        }
        commands
            .entity(aim_request.controller)
            .insert(TranquilizerHud {
                distance_m,
                in_range,
                reticle,
                misfire_visible,
            });
    }
}

pub(super) fn resolve_tranquilizer_fire_requests_to_shot_or_misfire_outcomes(
    mut commands: Commands,
    mut fire_requests: MessageReader<FireTranquilizer>,
    mut huds: Query<&mut TranquilizerHud>,
    mut tools: Query<&mut TranquilizerTool>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    transforms: Query<&GlobalTransform>,
    cameras: Query<&GlobalTransform, With<ZooCamera>>,
    mut tranquilize_requests: MessageWriter<TranquilizeRequest>,
    mut fired_messages: MessageWriter<TranquilizerFired>,
) {
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    let Some(mode) = definitions.tranquilizer_mode() else {
        return;
    };
    for fire_request in fire_requests.read() {
        let Ok(mut tool) = tools.get_mut(fire_request.controller) else {
            continue;
        };
        if definitions.find_tranquilizer(tool.tranquilizer).is_none() {
            continue;
        }
        let source_position = cameras
            .single()
            .ok()
            .map(|transform| transform.translation().to_array());
        let target_position = tool
            .target
            .and_then(|entity| transforms.get(entity).ok())
            .map(|transform| transform.translation().to_array());
        let (distance_m, measured_in_range) =
            calculate_optional_tranquilizer_target_distance_and_range(
                source_position,
                target_position,
                mode.range_cm as f32 / 100.0,
            );
        let in_range = source_position.is_some()
            && tool.target.is_some()
            && target_position.is_some()
            && measured_in_range;
        let outcome = select_tranquilizer_fire_outcome(&tool, in_range);
        if outcome == TranquilizerFireOutcome::Shot {
            tranquilize_requests.write(TranquilizeRequest {
                animal: tool.target.expect("target checked above"),
                tranquilizer: tool.tranquilizer,
                source: fire_request.controller,
            });
        }
        fired_messages.write(TranquilizerFired {
            controller: fire_request.controller,
            tranquilizer: tool.tranquilizer,
            outcome,
        });
        tool.charge_points = 0.0;
        if let Ok(mut hud) = huds.get_mut(fire_request.controller) {
            hud.distance_m = distance_m;
            hud.in_range = in_range;
            hud.reticle = tool.target.filter(|_| target_position.is_some()).map_or(
                TranquilizerReticle::NoTarget,
                |_| {
                    if in_range {
                        TranquilizerReticle::Charging
                    } else {
                        TranquilizerReticle::OutOfRange
                    }
                },
            );
            hud.misfire_visible = outcome == TranquilizerFireOutcome::Misfire;
        }
        if outcome == TranquilizerFireOutcome::Misfire {
            commands
                .entity(fire_request.controller)
                .insert(TranquilizerMisfireFeedback(Timer::from_seconds(
                    mode.misfire_duration_seconds,
                    TimerMode::Once,
                )));
        }
    }
}

#[cfg(test)]
mod tests {

    use super::calculate_optional_tranquilizer_target_distance_and_range;

    #[test]
    fn tranquilizer_range_is_inclusive_and_missing_view_is_in_range() {
        assert_eq!(
            calculate_optional_tranquilizer_target_distance_and_range(
                None,
                Some([20.0, 0.0, 0.0]),
                10.0,
            ),
            (0.0, true),
        );
        assert_eq!(
            calculate_optional_tranquilizer_target_distance_and_range(
                Some([0.0; 3]),
                Some([3.0, 4.0, 0.0]),
                5.0,
            ),
            (5.0, true),
        );
        assert_eq!(
            calculate_optional_tranquilizer_target_distance_and_range(
                Some([0.0; 3]),
                Some([6.0, 8.0, 0.0]),
                5.0,
            ),
            (10.0, false),
        );
    }
}
