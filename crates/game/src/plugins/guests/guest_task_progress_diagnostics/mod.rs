//! Debug-level trace of what holds each guest between task selections.

use bevy::{ecs::entity::EntityHashMap, prelude::*};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_data::AssetId;

use crate::plugins::animal_behavior::behavior_move_execution::PendingBehaviorMoveCompletion;
use crate::plugins::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use crate::plugins::animal_behavior::behavior_synchronized_set_execution::SynchronizedBehaviorParticipant;
use crate::plugins::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use crate::plugins::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use crate::plugins::animation_graph::model_animation_asset_binding_types::{
    ModelAnimationAttachmentResolved, PendingModelAnimationAssets,
};
use crate::plugins::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use crate::plugins::behavior_task_execution_types::{
    BehaviorTaskExecutionPhase, BehaviorTaskExecutionState, PendingBehaviorAnimationClipCompletion,
    PendingBehaviorDockingCompletion, PendingBehaviorTaskFailure,
};
use crate::plugins::economy::service_types::ServiceReservation;
use crate::plugins::locomotion::locomotion_types::Destination;
use crate::plugins::model_render::ModelExpanded;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::transport_tours::transport_rider_types::{TransportRider, WaitingForTransport};
use crate::plugins::world_spawn::prefab_presentation_types::{
    PrefabModel, PrefabPresentationHydrated,
};

use super::guest_simulation_types::{Guest, GuestDestination, GuestPhase, Viewing};
use super::guest_task_need_facts::{GuestNeedTriggers, GuestTaskNeedFacts};

/// Logs each guest's phase, current task frame, blocking components and
/// hunger whenever any of them changes, so a guest that never returns to task
/// selection shows what it is still waiting for.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn report_guest_task_progress_changes(
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
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
        &AnimationPresentationOwner,
        (
            With<AnimationPlaybackController>,
            With<AnimationGraphPlaybackState>,
        ),
    >,
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
    mut reported: Local<EntityHashMap<String>>,
) {
    if !bevy::log::tracing::enabled!(bevy::log::Level::DEBUG) {
        return;
    }
    reported.retain(|guest, _| guests.contains(*guest));
    for (guest, phase, task, needs, triggers, blockers, waits) in &guests {
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
            (name, task.phase, task.action, task.stack.len())
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
        let has_controller = controllers
            .iter()
            .any(|owner| owner.gameplay_entity == guest);
        let signature = format!(
            "phase={phase:?} controller={has_controller} task={task:?} \
             (destination, guest_destination, service, viewing, pending_set, waiting_transport, rider, synchronized)={blockers:?} \
             (clip, docking, move, failure)={waits:?} hunger_percent_triggered={hunger:?} \
             (hydrated, visible)={presentation:?} \
             (models, expanded, pending, resolved, controller, graph)={model_pipeline:?}"
        );
        if reported.get(&guest) != Some(&signature) {
            debug!(
                ?guest,
                tick = clock.tick,
                "guest task progress: {signature}"
            );
            reported.insert(guest, signature);
        }
    }
}
