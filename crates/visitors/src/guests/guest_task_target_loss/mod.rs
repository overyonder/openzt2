//! Routes target teardown to the guest tasks currently using that entity.

use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow, relationship::RelationshipTarget},
    prelude::*,
};
use openzt2_zoo_state::behavior_task_execution_types::{
    running_actions::mark_behavior_task_for_failure_and_stop_navigation,
    BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
};

use super::guest_simulation_types::Guest;

#[derive(Component)]
#[relationship(relationship_target = GuestTaskTargetUsers)]
pub(super) struct GuestTaskTarget(Entity);

#[derive(Component)]
#[relationship_target(relationship = GuestTaskTarget)]
pub(super) struct GuestTaskTargetUsers(Vec<Entity>);

#[derive(Message)]
pub(super) struct GuestTaskTargetLost {
    guest: Entity,
    target: Entity,
}

pub(super) fn observe_guest_task_target_removal(
    discarded: On<Despawn<GuestTaskTargetUsers>>,
    users: Query<&GuestTaskTargetUsers, Allow<Disabled>>,
    mut lost: MessageWriter<GuestTaskTargetLost>,
) {
    if let Ok(users) = users.get(discarded.entity) {
        for guest in users.iter() {
            lost.write(GuestTaskTargetLost {
                guest,
                target: discarded.entity,
            });
        }
    }
}

pub(super) fn observe_guest_task_target_disabling(
    disabled: On<Add<Disabled>>,
    users: Query<&GuestTaskTargetUsers, Allow<Disabled>>,
    mut lost: MessageWriter<GuestTaskTargetLost>,
) {
    if let Ok(users) = users.get(disabled.entity) {
        for guest in users.iter() {
            lost.write(GuestTaskTargetLost {
                guest,
                target: disabled.entity,
            });
        }
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn reconcile_guest_task_targets(
    mut commands: Commands,
    changed: Query<
        Entity,
        (
            With<Guest>,
            Or<(Added<Guest>, Changed<BehaviorTaskExecutionState>)>,
        ),
    >,
    tasks: Query<
        (
            &BehaviorTaskExecutionState,
            Option<&GuestTaskTarget>,
            Has<PendingBehaviorTaskFailure>,
        ),
        With<Guest>,
    >,
    entities: Query<()>,
    mut lost: MessageReader<GuestTaskTargetLost>,
    mut removed_tasks: RemovedComponents<BehaviorTaskExecutionState>,
    mut enabled_guests: RemovedComponents<Disabled>,
) {
    let mut visited = bevy::ecs::entity::EntityHashSet::default();
    for guest in changed
        .iter()
        .chain(removed_tasks.read())
        .chain(enabled_guests.read())
        .chain(
            lost.read()
                .filter_map(|event| (!entities.contains(event.target)).then_some(event.guest)),
        )
        .filter(|guest| visited.insert(*guest))
    {
        let Ok((task, current, failing)) = tasks.get(guest) else {
            if entities.contains(guest) {
                commands.entity(guest).remove::<GuestTaskTarget>();
            }
            continue;
        };
        let target = task.target().filter(|target| *target != guest);
        match target {
            Some(target) if !entities.contains(target) => {
                if !failing {
                    mark_behavior_task_for_failure_and_stop_navigation(guest, &mut commands);
                }
            }
            Some(target) if current.is_none_or(|current| current.0 != target) => {
                commands.entity(guest).insert(GuestTaskTarget(target));
            }
            None if current.is_some() => {
                commands.entity(guest).remove::<GuestTaskTarget>();
            }
            _ => {}
        }
    }
}
