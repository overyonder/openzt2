use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use super::show_audience_types::GuestShowAttendance;
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::economy::service_types::ServiceReservation;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::guests::guest_simulation_types::GuestPhase;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;

#[allow(
    clippy::type_complexity,
    reason = "cleanup also visits guests that lost required owner components"
)]
pub(super) fn synchronize_authored_show_attendance(
    mut commands: Commands,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    clock: Res<ZooClock>,
    guests: Query<
        (
            Entity,
            Option<&Guest>,
            Option<&GuestPhase>,
            Option<&WorldMember>,
            Option<&BehaviorTaskExecutionState>,
            Option<&GuestShowAttendance>,
        ),
        Or<(With<Guest>, With<GuestShowAttendance>)>,
    >,
    interruptions: Query<
        (),
        Or<(
            With<PendingBehaviorTaskFailure>,
            With<PendingBehaviorSet>,
            With<Destination>,
            With<ServiceReservation>,
        )>,
    >,
    targets: Query<&WorldMember>,
) {
    for (entity, guest, phase, world, task, attendance) in &guests {
        let current = task
            .filter(|task| {
                guest.is_some()
                    && phase == Some(&GuestPhase::Visiting)
                    && !interruptions.contains(entity)
                    && task.next_action_tick <= clock.tick
                    && matches!(
                        assets
                            .get(&task.document)
                            .and_then(|document| find_current_behavior_task_action(document, task)),
                        Some(BehaviorAction::WatchShow(_))
                    )
            })
            .and_then(|task| {
                let target = task.target?;
                let world = world?;
                (target != entity
                    && targets
                        .get(target)
                        .is_ok_and(|target| target.root == world.root))
                .then_some(GuestShowAttendance {
                    target,
                    execution_id: task.execution_id,
                })
            });
        if current.as_ref() == attendance {
            continue;
        }
        if let Some(current) = current {
            commands.entity(entity).insert(current);
        } else {
            commands.entity(entity).remove::<GuestShowAttendance>();
        }
        // Registration cannot complete the action. Event identity, scoring and
        // nested reactions require the running show's event producer.
    }
}
