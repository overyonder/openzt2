//! Destination observations have no authority over a replacement activity.

use bevy::{
    ecs::{entity::EntityHashSet, entity_disabling::Disabled, system::SystemParam},
    prelude::*,
};
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestVisitPurpose;

use super::guest_simulation_types::{
    Guest, GuestDestination, GuestNavigationRequest, GuestPhase, GuestViewers, GuestViewingTarget,
    Viewing, ViewingOpportunity,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorViewingProgress, RunBehaviorViewAnimals, RunningBehaviorInstruction,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::economy::service_types::ServiceReservation;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;

pub(super) fn viewing_owned_by_current_view_action(
    task: &BehaviorTaskExecutionState,
    viewing: &Viewing,
    instruction: &RunningBehaviorInstruction,
) -> bool {
    viewing.area.is_some()
        && viewing.task_execution_id == Some(task.execution_id)
        && viewing.area == task.target()
        && instruction.is_current(task)
}

#[derive(Component)]
#[relationship(relationship_target = ObservationAreaViewers, allow_self_referential)]
pub(super) struct ObservationAreaOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = ObservationAreaOf)]
pub(super) struct ObservationAreaViewers(Vec<Entity>);

#[derive(SystemParam)]
pub(super) struct ObservationChanges<'w, 's> {
    changed_guests: Query<
        'w,
        's,
        Entity,
        (
            With<Guest>,
            Or<(
                Changed<Viewing>,
                Changed<GuestDestination>,
                Changed<GuestNavigationRequest>,
                Changed<GuestViewingTarget>,
                Changed<GuestPhase>,
                Changed<WorldMember>,
                Changed<BehaviorTaskExecutionState>,
                Changed<RunningBehaviorInstruction>,
                Added<PendingBehaviorSet>,
                Added<ServiceReservation>,
                Added<Destination>,
                Added<BehaviorViewingProgress>,
                Added<RunBehaviorViewAnimals>,
            )>,
        ),
    >,
    changed_owners: Query<
        'w,
        's,
        Entity,
        (
            Allow<Disabled>,
            Or<(With<GuestViewers>, With<ObservationAreaViewers>)>,
            Or<(Changed<WorldMember>, Added<Disabled>)>,
        ),
    >,
    subjects: Query<'w, 's, &'static GuestViewers, Allow<Disabled>>,
    areas: Query<'w, 's, &'static ObservationAreaViewers, Allow<Disabled>>,
    ended_viewing: RemovedComponents<'w, 's, Viewing>,
    ended_area: RemovedComponents<'w, 's, ObservationAreaOf>,
    ended_task: RemovedComponents<'w, 's, BehaviorTaskExecutionState>,
    ended_view_action: RemovedComponents<'w, 's, RunBehaviorViewAnimals>,
    lost_member: RemovedComponents<'w, 's, WorldMember>,
    lost_animal: RemovedComponents<'w, 's, Animal>,
    lost_opportunity: RemovedComponents<'w, 's, ViewingOpportunity>,
    lost_destination: RemovedComponents<'w, 's, GuestDestination>,
    lost_request: RemovedComponents<'w, 's, GuestNavigationRequest>,
    lost_target: RemovedComponents<'w, 's, GuestViewingTarget>,
    lost_phase: RemovedComponents<'w, 's, GuestPhase>,
    enabled: RemovedComponents<'w, 's, Disabled>,
}

impl ObservationChanges<'_, '_> {
    fn affected(&mut self) -> EntityHashSet {
        let mut affected: EntityHashSet = self
            .changed_guests
            .iter()
            .chain(self.ended_viewing.read())
            .chain(self.ended_area.read())
            .chain(self.ended_task.read())
            .chain(self.ended_view_action.read())
            .chain(self.lost_destination.read())
            .chain(self.lost_request.read())
            .chain(self.lost_target.read())
            .chain(self.lost_phase.read())
            .chain(self.enabled.read())
            .collect();
        for owner in self
            .changed_owners
            .iter()
            .chain(self.lost_member.read())
            .chain(self.lost_animal.read())
            .chain(self.lost_opportunity.read())
        {
            affected.insert(owner);
            if let Ok(viewers) = self.subjects.get(owner) {
                affected.extend(viewers.iter());
            }
            if let Ok(viewers) = self.areas.get(owner) {
                affected.extend(viewers.iter());
            }
        }
        affected
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn revoke_unowned_guest_observations(
    mut commands: Commands,
    mut changes: ObservationChanges,
    guests: Query<
        (
            Entity,
            Option<&Viewing>,
            Option<&GuestDestination>,
            Option<&GuestNavigationRequest>,
            Option<&GuestViewingTarget>,
            Option<&GuestPhase>,
            Option<&WorldMember>,
            Has<BehaviorViewingProgress>,
            Option<&ObservationAreaOf>,
        ),
        (
            With<Guest>,
            Or<(
                With<Viewing>,
                With<GuestDestination>,
                With<BehaviorViewingProgress>,
                With<ObservationAreaOf>,
            )>,
        ),
    >,
    replacement_activities: Query<
        (),
        Or<(
            With<BehaviorTaskExecutionState>,
            With<PendingBehaviorSet>,
            With<ServiceReservation>,
            With<Destination>,
        )>,
    >,
    subjects: Query<&WorldMember, With<Animal>>,
    areas: Query<&WorldMember, With<ViewingOpportunity>>,
    tasks: Query<
        (&BehaviorTaskExecutionState, &RunningBehaviorInstruction),
        With<RunBehaviorViewAnimals>,
    >,
) {
    for (guest, viewing, destination, request, target, phase, world, meter, area_link) in
        guests.iter_many(changes.affected()).matched()
    {
        let navigating = request.is_some_and(|request| request.0.is_some());
        let Some(viewing) = viewing else {
            if area_link.is_some() {
                commands.entity(guest).remove::<ObservationAreaOf>();
            }
            if meter {
                commands.entity(guest).remove::<BehaviorViewingProgress>();
            }
            // Removing the subject removes the relationship through Bevy's hook.
            // Release only an arrived observation, never a replacement route.
            if !navigating
                && target.is_none()
                && destination
                    .is_some_and(|destination| destination.purpose == GuestVisitPurpose::View)
            {
                commands.entity(guest).remove::<GuestDestination>();
            }
            continue;
        };
        // An authored view action owns the observation of the path it stands on.
        let owned_by_view_action = tasks.get(guest).is_ok_and(|(task, instruction)| {
            viewing_owned_by_current_view_action(task, viewing, instruction)
        });
        if owned_by_view_action
            && phase == Some(&GuestPhase::Visiting)
            && world.is_some_and(|world| {
                subjects
                    .get(viewing.subject)
                    .is_ok_and(|subject| subject.root == world.root)
            })
        {
            continue;
        }
        let owns_destination = destination.is_some_and(|destination| {
            destination.purpose == GuestVisitPurpose::View
                && Some(destination.entity) == viewing.area
        });
        let same_world = world.is_some_and(|world| {
            subjects
                .get(viewing.subject)
                .is_ok_and(|subject| subject.root == world.root)
                && viewing
                    .area
                    .is_some_and(|area| areas.get(area).is_ok_and(|area| area.root == world.root))
        });
        if owns_destination
            && viewing.task_execution_id.is_none()
            && !navigating
            && phase == Some(&GuestPhase::Visiting)
            && same_world
            && !replacement_activities.contains(guest)
        {
            if let Some(area) = viewing.area {
                if area_link.is_none_or(|link| link.0 != area) {
                    commands.entity(guest).insert(ObservationAreaOf(area));
                }
            }
            continue;
        }
        commands
            .entity(guest)
            .remove::<(Viewing, BehaviorViewingProgress, ObservationAreaOf)>();
        if owns_destination && !navigating {
            commands
                .entity(guest)
                .remove::<(GuestDestination, GuestViewingTarget)>();
        }
    }
}
