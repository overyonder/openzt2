use bevy::prelude::*;

use super::show_audience_types::GuestShowAttendance;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorWatchShow, RunningBehaviorInstruction,
};
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
    watching: Query<&RunningBehaviorInstruction, With<RunBehaviorWatchShow>>,
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
                    && watching
                        .get(entity)
                        .is_ok_and(|instruction| instruction.is_current(task))
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
