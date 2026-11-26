use bevy::prelude::*;

use openzt2_interface_state::camera::camera_runtime_state_types::{CameraMode, ZooCamera};
use openzt2_interface_state::input::input_types::{ActionRequest, GameAction};
use openzt2_zoo_state::maintenance::maintenance_types::{
    AdjustFacilityContainedWaste, FacilityContainedWaste, LooseLitterWaste,
};

/// How far the player can reach from the first-person camera, in metres.
const REACH_METRES: f32 = 4.0;

pub(super) fn clean_nearest_waste_from_first_person_use_action(
    mut commands: Commands,
    mut actions: MessageReader<ActionRequest>,
    cameras: Query<(&GlobalTransform, &CameraMode), With<ZooCamera>>,
    litter: Query<(Entity, &GlobalTransform), With<LooseLitterWaste>>,
    containers: Query<(Entity, &GlobalTransform, &FacilityContainedWaste)>,
    mut container_changes: MessageWriter<AdjustFacilityContainedWaste>,
) {
    let uses = actions
        .read()
        .filter(|request| request.action == GameAction::UseObject)
        .count();
    if uses == 0 {
        return;
    }
    let Some(position) = cameras
        .iter()
        .find(|(_, mode)| matches!(mode, CameraMode::FirstPerson))
        .map(|(transform, _)| transform.translation())
    else {
        return;
    };
    let reach = |transform: &GlobalTransform| {
        let distance = transform.translation().distance(position);
        (distance <= REACH_METRES).then_some(distance)
    };
    let mut targets: Vec<_> = litter
        .iter()
        .filter_map(|(entity, transform)| Some((entity, reach(transform)?, true)))
        .chain(
            containers
                .iter()
                .filter(|(_, _, container)| container.contained_waste_units > 0)
                .filter_map(|(entity, transform, _)| Some((entity, reach(transform)?, false))),
        )
        .collect();
    targets.sort_by(
        |(_, left_distance, left_litter), (_, right_distance, right_litter)| {
            left_distance
                .total_cmp(right_distance)
                .then_with(|| right_litter.cmp(left_litter))
        },
    );
    for (entity, _, is_litter) in targets.into_iter().take(uses) {
        if is_litter {
            commands.entity(entity).despawn();
        } else {
            container_changes.write(AdjustFacilityContainedWaste {
                facility: entity,
                operation: openzt2_game_data::behavior::action::modification::BehaviorFactModificationOperation::Clear,
                delta_q16: 0,
            });
        }
    }
}
