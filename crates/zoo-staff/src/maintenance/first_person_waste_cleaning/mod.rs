use bevy::prelude::*;

use openzt2_interface_state::camera::camera_runtime_state_types::{CameraMode, ZooCamera};
use openzt2_interface_state::input::input_types::{ActionRequest, GameAction};
use openzt2_zoo_state::maintenance::maintenance_types::{
    AdjustFacilityContainedWaste, FacilityContainedWaste, IncrementalZooCleanlinessTotals,
    LooseLitterWaste,
};

/// How far the player can reach from the first-person camera, in metres.
const REACH_METRES: f32 = 4.0;

/// `ZT_USE_OBJECT`: on foot, the player sweeps the nearest litter or empties the
/// nearest trash can, as a maintenance worker would.
pub(super) fn clean_nearest_waste_from_first_person_use_action(
    mut commands: Commands,
    mut actions: MessageReader<ActionRequest>,
    keys: Res<ButtonInput<KeyCode>>,
    cameras: Query<(&GlobalTransform, &CameraMode), With<ZooCamera>>,
    mut litter: Query<(Entity, &GlobalTransform, &mut LooseLitterWaste)>,
    containers: Query<(Entity, &GlobalTransform, &FacilityContainedWaste)>,
    mut cleanliness_totals: ResMut<IncrementalZooCleanlinessTotals>,
    mut container_changes: MessageWriter<AdjustFacilityContainedWaste>,
) {
    // `hotkeys.xml` binds both keys to `ZT_USE_OBJECT` in this mode.
    let pressed = keys.any_just_pressed([KeyCode::Space, KeyCode::KeyE]);
    if !actions
        .read()
        .any(|request| request.action == GameAction::UseObject)
        && !pressed
    {
        return;
    }
    let Some(position) = cameras
        .iter()
        .find(|(_, mode)| matches!(mode, CameraMode::FirstPerson(_)))
        .map(|(transform, _)| transform.translation())
    else {
        return;
    };
    let reach = |transform: &GlobalTransform| {
        let distance = transform.translation().distance(position);
        (distance <= REACH_METRES).then_some(distance)
    };
    let nearest_litter = litter
        .iter()
        .filter_map(|(entity, transform, _)| Some((entity, reach(transform)?)))
        .min_by(|left, right| left.1.total_cmp(&right.1));
    let nearest_container = containers
        .iter()
        .filter(|(_, _, container)| container.contained_waste_units > 0)
        .filter_map(|(entity, transform, _)| Some((entity, reach(transform)?)))
        .min_by(|left, right| left.1.total_cmp(&right.1));
    match (nearest_litter, nearest_container) {
        (Some((entity, litter_distance)), container)
            if container.is_none_or(|(_, distance)| litter_distance <= distance) =>
        {
            if let Ok((_, _, mut waste)) = litter.get_mut(entity) {
                cleanliness_totals
                    .replace_uncontained_waste_units(waste.uncontained_waste_units, 0);
                waste.uncontained_waste_units = 0;
                commands.entity(entity).despawn();
            }
        }
        (_, Some((facility, _))) => {
            container_changes.write(AdjustFacilityContainedWaste {
                facility,
                operation: openzt2_game_data::behavior::action::modification::BehaviorFactModificationOperation::Clear,
                delta_q16: 0,
            });
        }
        _ => {}
    }
}
