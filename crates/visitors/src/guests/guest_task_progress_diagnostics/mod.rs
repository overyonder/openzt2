//! Debug-level trace of what holds each guest between task selections.
use openzt2_zoo_state::guests::guest_simulation_types::GuestNeedTriggers;

use bevy::{
    ecs::{entity::EntityHashSet, system::SystemParam},
    prelude::*,
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_data::AssetId;

use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::{
    AnimationPresentationOwner, AnimationPresentations,
};
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::ModelAnimationAttachmentResolved;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::model_render::ModelExpanded;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentationHydrated;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorDockingCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorMoveCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::behavior_task_execution_types::SynchronizedBehaviorParticipant;
use openzt2_zoo_state::economy::service_types::ServiceReservation;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::transport_tours::transport_rider_types::TransportRider;
use openzt2_zoo_state::transport_tours::transport_rider_types::WaitingForTransport;

use super::guest_simulation_types::{Guest, GuestDestination, GuestPhase, Viewing};
use super::guest_task_need_facts::GuestTaskNeedFacts;

#[derive(SystemParam)]
pub(super) struct RemovedGuestProgress<'w, 's> {
    tasks: RemovedComponents<'w, 's, BehaviorTaskExecutionState>,
    routes: RemovedComponents<'w, 's, Destination>,
    destinations: RemovedComponents<'w, 's, GuestDestination>,
    services: RemovedComponents<'w, 's, ServiceReservation>,
    viewing: RemovedComponents<'w, 's, Viewing>,
    sets: RemovedComponents<'w, 's, PendingBehaviorSet>,
    transport: RemovedComponents<'w, 's, WaitingForTransport>,
    riders: RemovedComponents<'w, 's, TransportRider>,
    synchronized: RemovedComponents<'w, 's, SynchronizedBehaviorParticipant>,
    clips: RemovedComponents<'w, 's, PendingBehaviorAnimationClipCompletion>,
    docking: RemovedComponents<'w, 's, PendingBehaviorDockingCompletion>,
    moving: RemovedComponents<'w, 's, PendingBehaviorMoveCompletion>,
    failing: RemovedComponents<'w, 's, PendingBehaviorTaskFailure>,
    presentations: RemovedComponents<'w, 's, AnimationPresentations>,
}

impl RemovedGuestProgress<'_, '_> {
    fn read(&mut self) -> impl Iterator<Item = Entity> + '_ {
        self.tasks
            .read()
            .chain(self.routes.read())
            .chain(self.destinations.read())
            .chain(self.services.read())
            .chain(self.viewing.read())
            .chain(self.sets.read())
            .chain(self.transport.read())
            .chain(self.riders.read())
            .chain(self.synchronized.read())
            .chain(self.clips.read())
            .chain(self.docking.read())
            .chain(self.moving.read())
            .chain(self.failing.read())
            .chain(self.presentations.read())
    }
}

pub(super) fn guest_progress_diagnostics_enabled() -> bool {
    bevy::log::tracing::enabled!(bevy::log::Level::DEBUG)
}

/// Logs each guest's phase, current task frame, blocking components and
/// hunger whenever any of them changes, so a guest that never returns to task
/// selection shows what it is still waiting for.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn report_guest_task_progress_changes(
    clock: Single<&ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    changed: Query<
        Entity,
        (
            With<Guest>,
            Or<(
                Changed<GuestPhase>,
                Changed<BehaviorTaskExecutionState>,
                Changed<GuestNeedTriggers>,
                Added<Destination>,
                Added<GuestDestination>,
                Added<ServiceReservation>,
                Added<Viewing>,
                Added<PendingBehaviorSet>,
                Added<WaitingForTransport>,
                Added<TransportRider>,
                Added<SynchronizedBehaviorParticipant>,
                Changed<AnimationPresentations>,
                Or<(
                    Added<PendingBehaviorAnimationClipCompletion>,
                    Added<PendingBehaviorDockingCompletion>,
                    Added<PendingBehaviorMoveCompletion>,
                    Added<PendingBehaviorTaskFailure>,
                    Added<PrefabPresentationHydrated>,
                    Changed<InheritedVisibility>,
                )>,
            )>,
        ),
    >,
    mut removed: RemovedGuestProgress,
    guests: Query<
        (
            Entity,
            &GuestPhase,
            Option<&BehaviorTaskExecutionState>,
            GuestTaskNeedFacts,
            Option<&GuestNeedTriggers>,
            (
                Has<Destination>,
                Has<GuestDestination>,
                Has<ServiceReservation>,
                Has<Viewing>,
                Has<PendingBehaviorSet>,
                Has<WaitingForTransport>,
                Has<TransportRider>,
                Has<SynchronizedBehaviorParticipant>,
            ),
            (
                Has<PendingBehaviorAnimationClipCompletion>,
                Has<PendingBehaviorDockingCompletion>,
                Has<PendingBehaviorMoveCompletion>,
                Has<PendingBehaviorTaskFailure>,
            ),
        ),
        With<Guest>,
    >,
    controllers: Query<
        (),
        (
            With<AnimationPlaybackController>,
            With<AnimationGraphPlaybackState>,
        ),
    >,
    presentations: Query<&AnimationPresentations>,
    changed_models: Query<
        (Entity, Option<&AnimationPresentationOwner>),
        (
            With<PrefabModel>,
            Or<(
                Added<PrefabModel>,
                Added<ModelExpanded>,
                Added<PendingModelAnimationAssets>,
                Added<ModelAnimationAttachmentResolved>,
                Added<AnimationPlaybackController>,
                Added<AnimationGraphPlaybackState>,
            )>,
        ),
    >,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    models: Query<(
        Has<PrefabModel>,
        Has<ModelExpanded>,
        Has<PendingModelAnimationAssets>,
        Has<ModelAnimationAttachmentResolved>,
        Has<AnimationPlaybackController>,
        Has<AnimationGraphPlaybackState>,
    )>,
    hydrated: Query<(Has<PrefabPresentationHydrated>, &InheritedVisibility)>,
) {
    let mut affected: EntityHashSet = changed.iter().chain(removed.read()).collect();
    for (model, owner) in &changed_models {
        if let Some(guest) = owner
            .map(|owner| owner.gameplay_entity)
            .filter(|&guest| guests.contains(guest))
            .or_else(|| {
                parents
                    .iter_ancestors::<ChildOf>(model)
                    .find(|&ancestor| guests.contains(ancestor))
            })
        {
            affected.insert(guest);
        }
    }
    for (guest, phase, task, needs, triggers, blockers, waits) in
        guests.iter_many(&affected).matched()
    {
        let task = task.map(|task| {
            let name = (task.phase != BehaviorTaskExecutionPhase::Set)
                .then(|| {
                    documents
                        .get(&task.document)
                        .and_then(|document| document.behavior_task_at_index(task.declaration))
                        .map(|declaration| declaration.name.clone())
                })
                .flatten()
                .unwrap_or_else(|| format!("{:?}", task.program));
            (name, task.phase, task.action, task.depth)
        });
        let hunger = triggers.and_then(|triggers| {
            needs
                .source_need(AssetId::from_key("hunger"), triggers)
                .map(|(value_q16, triggered)| (value_q16 >> 16, triggered))
        });
        // Counts over the guest's presentation descendants: prefab models,
        // expanded, pending animation, resolved, controller, graph state.
        let mut model_pipeline = [0_u16; 6];
        for descendant in children.iter_descendants(guest) {
            let Ok(flags) = models.get(descendant) else {
                continue;
            };
            if !flags.0 {
                continue;
            }
            let flags = [flags.0, flags.1, flags.2, flags.3, flags.4, flags.5];
            for (count, flag) in model_pipeline.iter_mut().zip(flags) {
                *count += u16::from(flag);
            }
        }
        let presentation = hydrated
            .get(guest)
            .map(|(hydrated, visible)| (hydrated, visible.get()))
            .ok();
        let has_controller =
            AnimationPresentations::for_subject(guest, presentations.get(guest).ok())
                .any(|controller| controllers.contains(controller));
        debug!(
            ?guest,
            tick = clock.tick,
            "phase={phase:?} controller={has_controller} task={task:?} \
             (destination, guest_destination, service, viewing, pending_set, waiting_transport, rider, synchronized)={blockers:?} \
             (clip, docking, move, failure)={waits:?} hunger_percent_triggered={hunger:?} \
             (hydrated, visible)={presentation:?} \
             (models, expanded, pending, resolved, controller, graph)={model_pipeline:?}"
        );
    }
}
