//! Destination observations have no authority over a replacement activity.

use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestVisitPurpose;

use super::guest_simulation_types::{
    Guest, GuestDestination, GuestNavigationRequest, GuestPhase, GuestViewingTarget, Viewing,
    ViewingOpportunity,
};
use crate::plugins::behavior_task_execution_types::find_current_behavior_task_action;
use crate::plugins::{
    animal_behavior::behavior_set_start_request_types::PendingBehaviorSet,
    animal_lifecycle::types::Animal, behavior_task_execution_types::BehaviorTaskExecutionState,
    economy::service_types::ServiceReservation, locomotion::locomotion_types::Destination,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_data::behavior::action_record::BehaviorAction;

pub(super) fn revoke_unowned_guest_observations(
    mut commands: Commands,
    guests: Query<
        (
            Entity,
            Option<&Viewing>,
            Option<&GuestDestination>,
            Option<&GuestNavigationRequest>,
            Option<&GuestViewingTarget>,
            Option<&GuestPhase>,
            Option<&WorldMember>,
        ),
        (With<Guest>, Or<(With<Viewing>, With<GuestDestination>)>),
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
    tasks: Query<&BehaviorTaskExecutionState>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
) {
    for (guest, viewing, destination, request, target, phase, world) in &guests {
        let navigating = request.is_some_and(|request| request.0.is_some());
        let Some(viewing) = viewing else {
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
        let owned_by_view_action = tasks.get(guest).is_ok_and(|task| {
            viewing.area.is_some()
                && viewing.area == task.target()
                && documents
                    .get(&task.document)
                    .and_then(|document| find_current_behavior_task_action(document, task))
                    .is_some_and(|action| matches!(action, BehaviorAction::ViewAnimals(_)))
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
            && !navigating
            && phase == Some(&GuestPhase::Visiting)
            && same_world
            && !replacement_activities.contains(guest)
        {
            continue;
        }
        commands.entity(guest).remove::<Viewing>();
        if owns_destination && !navigating {
            commands
                .entity(guest)
                .remove::<(GuestDestination, GuestViewingTarget)>();
        }
    }
}
