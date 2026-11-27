use bevy::prelude::*;
use openzt2_game_assets::{
    application_lifecycle::GamePhase, behavior::behavior_asset_types::BehaviorDocumentAsset,
};
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_world_state::{
    application_schedule::FixedGameSet, simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_zoo_state::behavior_task_execution_types::{
    advance_behavior_task_to_next_action, find_current_behavior_task_action,
    BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
};
use openzt2_zoo_state::locomotion::locomotion_types::TurnToHeading;

pub(super) fn register_face_target_execution(application: &mut App) {
    application.add_systems(
        FixedUpdate,
        face_current_behavior_targets
            .after(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
            .in_set(FixedGameSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
}

/// Turns the subject to its target's position when it starts, plus the
/// authored offset, unless its heading is already within the authored error.
/// An animal turns on the spot with its turning clips, as `zt.exe` hands the
/// heading to locomotion (`BFBehFaceTarget`, `FUN_0061ded7`), and the action
/// waits for it; anyone else turns at once.
fn face_current_behavior_targets(
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &mut Transform,
            Has<openzt2_zoo_state::animal_lifecycle::types::Animal>,
            Has<TurnToHeading>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    targets: Query<&GlobalTransform>,
) {
    for (actor, mut task, mut transform, animal, turning) in &mut actors {
        if turning || task.next_action_tick > clock.tick {
            continue;
        }
        let Some(&BehaviorAction::FaceTarget(action)) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        if let Some(toward) = task
            .target()
            .and_then(|target| targets.get(target).ok())
            .and_then(|target| {
                (target.translation() - transform.translation)
                    .xz()
                    .try_normalize()
            })
        {
            // Actor models face local +Z.
            let wanted = toward.x.atan2(toward.y) + action.rotation_offset_radians;
            let current = transform.rotation * Vec3::Z;
            let current = current.x.atan2(current.z);
            let error = (wanted - current + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            if error.abs() > action.rotation_tolerance_radians {
                if animal {
                    commands.entity(actor).insert(TurnToHeading {
                        heading_radians: wanted,
                        tolerance_radians: action.rotation_tolerance_radians,
                    });
                    continue;
                }
                transform.rotation = Quat::from_rotation_y(wanted);
            }
        }
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}
