use bevy::{ecs::relationship::RelationshipTarget, prelude::*};

use super::show_audience_types::{GuestShowExperience, GuestWatchingStage, StageShowAudience};
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_set_start_request_types::PendingBehaviorSet,
    behavior_task_execution_types::{
        running_actions::{RunBehaviorWatchShow, RunningBehaviorInstruction},
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    economy::service_types::ServiceReservation,
    guests::guest_simulation_types::{Guest, GuestPhase},
    locomotion::locomotion_types::Destination,
    shows::{
        running_show_types::{ShowRunFinished, ShowViewingLocationOf},
        show_stage_types::ShowStage,
    },
};

pub(super) fn bind_ready_show_watch_actions(
    mut commands: Commands,
    clock: Res<ZooClock>,
    watching: Query<
        (
            Entity,
            &GuestPhase,
            &WorldMember,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            Has<GuestShowExperience>,
        ),
        (
            With<Guest>,
            With<RunBehaviorWatchShow>,
            Without<GuestWatchingStage>,
        ),
    >,
    blocked: Query<
        (),
        Or<(
            With<PendingBehaviorTaskFailure>,
            With<PendingBehaviorSet>,
            With<Destination>,
            With<ServiceReservation>,
        )>,
    >,
    stages: Query<&WorldMember, With<ShowStage>>,
    viewing_locations: Query<&ShowViewingLocationOf>,
    parents: Query<&ChildOf>,
) {
    for (guest, phase, world, task, instruction, has_history) in &watching {
        if *phase != GuestPhase::Visiting
            || task.next_action_tick > clock.tick
            || blocked.contains(guest)
            || !instruction.is_current(task)
        {
            continue;
        }
        let Some(target) = task.target else { continue };
        let stage = viewing_locations
            .get(target)
            .ok()
            .map(|location| location.0)
            .or_else(|| {
                std::iter::once(target)
                    .chain(parents.iter_ancestors::<ChildOf>(target))
                    .find(|&entity| stages.contains(entity))
            });
        let Some(stage) = stage else { continue };
        if let Ok(member) = stages.get(stage) {
            if world.root != member.root {
                continue;
            }
            let mut guest_commands = commands.entity(guest);
            guest_commands.insert(GuestWatchingStage {
                stage,
                instruction: Some(instruction.id()),
            });
            if !has_history {
                guest_commands.insert(GuestShowExperience::default());
            }
        }
    }
}

pub(super) fn retire_interrupted_show_watch_actions(
    watches: Query<
        (
            Entity,
            &GuestWatchingStage,
            &GuestPhase,
            &RunningBehaviorInstruction,
        ),
        (With<RunBehaviorWatchShow>, With<Guest>),
    >,
    blocked: Query<
        (),
        Or<(
            With<PendingBehaviorTaskFailure>,
            With<PendingBehaviorSet>,
            With<Destination>,
            With<ServiceReservation>,
        )>,
    >,
    mut commands: Commands,
) {
    for (guest, watch, phase, instruction) in &watches {
        if *phase != GuestPhase::Visiting
            || watch.instruction != Some(instruction.id())
            || blocked.contains(guest)
        {
            commands.entity(guest).try_remove::<GuestWatchingStage>();
        }
    }
}

pub(super) fn clear_completed_show_watch_action(
    removed: On<
        Remove<(
            RunBehaviorWatchShow,
            Guest,
            GuestPhase,
            RunningBehaviorInstruction,
        )>,
    >,
    linked: Query<(), With<GuestWatchingStage>>,
    mut commands: Commands,
) {
    if linked.contains(removed.entity) {
        commands
            .entity(removed.entity)
            .try_remove::<GuestWatchingStage>();
    }
}

pub(super) fn complete_watch_actions_after_show(
    mut finished: MessageReader<ShowRunFinished>,
    audiences: Query<&StageShowAudience>,
    guests: Query<(&GuestWatchingStage, &RunningBehaviorInstruction), With<RunBehaviorWatchShow>>,
    clock: Res<ZooClock>,
    mut commands: Commands,
) {
    for show in finished.read() {
        let Ok(audience) = audiences.get(show.stage) else {
            continue;
        };
        for guest in audience.iter() {
            let Ok((watch, instruction)) = guests.get(guest) else {
                continue;
            };
            if watch.instruction == Some(instruction.id()) {
                instruction.complete(guest, clock.tick, &mut commands);
            }
        }
    }
}
