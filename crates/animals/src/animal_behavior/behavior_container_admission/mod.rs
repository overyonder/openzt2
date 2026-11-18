use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_zoo_state::animal_behavior::interaction_container_membership::requests::{
    InteractionContainerAdmission, InteractionContainerAdmissionOutcome,
    InteractionContainerAdmissionRecipient, InteractionContainerAdmissionRequest,
    InteractionContainerRequest,
};

#[derive(SystemParam)]
pub(super) struct BehaviorContainerAdmissions<'w, 's> {
    pub(super) requests: MessageWriter<'w, InteractionContainerRequest>,
    pub(super) outcomes: MessageReader<'w, 's, InteractionContainerAdmission>,
}

pub(super) fn request_behavior_container_admission(
    actor: Entity,
    target: Entity,
    slots: Vec<(usize, u8)>,
    queue_service_slots: Vec<(usize, u8)>,
    reply_to: InteractionContainerAdmissionRecipient,
    requests: &mut MessageWriter<InteractionContainerRequest>,
) {
    requests.write(InteractionContainerRequest::Admit {
        admission: InteractionContainerAdmissionRequest::Admit {
            container: target,
            member: actor,
            slots,
            despawn_on_refusal: false,
            queue_service_slots,
        },
        reply_to,
    });
}

pub(super) struct SelectedBehaviorContainerSlot {
    pub(super) index: usize,
    pub(super) queue_service_available: bool,
}

pub(super) fn behavior_container_admission_selection(
    outcome: &InteractionContainerAdmission,
) -> Option<SelectedBehaviorContainerSlot> {
    match outcome.outcome {
        InteractionContainerAdmissionOutcome::Granted {
            slot,
            queue_service_available,
            ..
        } => Some(SelectedBehaviorContainerSlot {
            index: slot,
            queue_service_available,
        }),
        InteractionContainerAdmissionOutcome::Refused
        | InteractionContainerAdmissionOutcome::IdentityUnavailable => None,
    }
}
