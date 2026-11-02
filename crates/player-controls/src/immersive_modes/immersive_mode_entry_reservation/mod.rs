use bevy::prelude::*;

use super::immersive_mode_message_types::EnterImmersiveMode;
use super::immersive_mode_message_types::ExitImmersiveMode;
use super::immersive_mode_message_types::ImmersiveModeRejected;
use super::immersive_mode_message_types::ModeEntryFailure;
use super::immersive_mode_message_types::ModeExitReason;
use super::immersive_mode_state_types::ActiveImmersiveMode;
use super::immersive_mode_state_types::ImmersiveMode;
use crate::immersive_modes::immersive_control_execution_types::PendingImmersiveEntry;

/// The camera buttons are a radio set: pressing one while another view is open
/// leaves that view and then enters the new one.
const fn is_camera_button_view(mode: ImmersiveMode) -> bool {
    matches!(
        mode,
        ImmersiveMode::FirstPerson
            | ImmersiveMode::GuestView
            | ImmersiveMode::Photo
            | ImmersiveMode::SuperStaff
    )
}

pub(super) fn reserve_one_pending_immersive_mode_entry_from_valid_controller_request(
    mut entry_requests: MessageReader<EnterImmersiveMode>,
    active_immersive_modes: Query<(Entity, &ActiveImmersiveMode)>,
    pending_immersive_entries: Query<(), With<PendingImmersiveEntry>>,
    existing_controller_entities: Query<()>,
    mut queued_switch: Local<Option<EnterImmersiveMode>>,
    mut commands: Commands,
    mut rejected_entry_messages: MessageWriter<ImmersiveModeRejected>,
    mut exit_requests: MessageWriter<ExitImmersiveMode>,
) {
    let mut entry_is_reserved =
        !active_immersive_modes.is_empty() || !pending_immersive_entries.is_empty();
    let queued = (!entry_is_reserved).then(|| queued_switch.take()).flatten();
    for entry_request in queued.into_iter().chain(entry_requests.read().copied()) {
        if let Some((controller, _)) = active_immersive_modes
            .iter()
            .next()
            .filter(|(_, active)| {
                entry_request.mode != active.mode
                    && is_camera_button_view(active.mode)
                    && is_camera_button_view(entry_request.mode)
            })
        {
            exit_requests.write(ExitImmersiveMode {
                controller,
                reason: ModeExitReason::Cancelled,
            });
            *queued_switch = Some(entry_request);
        } else if entry_is_reserved {
            rejected_entry_messages.write(ImmersiveModeRejected {
                mode: entry_request.mode,
                reason: ModeEntryFailure::Busy,
            });
        } else if existing_controller_entities
            .get(entry_request.controller)
            .is_err()
        {
            rejected_entry_messages.write(ImmersiveModeRejected {
                mode: entry_request.mode,
                reason: ModeEntryFailure::InvalidController,
            });
        } else {
            commands
                .entity(entry_request.controller)
                .insert(PendingImmersiveEntry {
                    mode: entry_request.mode,
                    subject: entry_request.subject,
                    failure: None,
                });
            entry_is_reserved = true;
        }
    }
}

pub(super) fn reject_and_remove_uncommitted_pending_immersive_mode_entries(
    pending_immersive_entries: Query<(Entity, &PendingImmersiveEntry)>,
    mut commands: Commands,
    mut rejected_entry_messages: MessageWriter<ImmersiveModeRejected>,
) {
    for (controller_entity, pending_entry) in &pending_immersive_entries {
        rejected_entry_messages.write(ImmersiveModeRejected {
            mode: pending_entry.mode,
            reason: pending_entry
                .failure
                .unwrap_or(ModeEntryFailure::RuleDenied),
        });
        commands
            .entity(controller_entity)
            .remove::<PendingImmersiveEntry>();
    }
}
