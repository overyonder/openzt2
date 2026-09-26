use bevy::prelude::*;

use super::{
    immersive_mode_message_types::{EnterImmersiveMode, ImmersiveModeRejected, ModeEntryFailure},
    immersive_mode_state_types::{ActiveImmersiveMode, PendingImmersiveEntry},
};

pub(super) fn reserve_one_pending_immersive_mode_entry_from_valid_controller_request(
    mut entry_requests: MessageReader<EnterImmersiveMode>,
    active_immersive_modes: Query<(), With<ActiveImmersiveMode>>,
    pending_immersive_entries: Query<(), With<PendingImmersiveEntry>>,
    existing_controller_entities: Query<()>,
    mut commands: Commands,
    mut rejected_entry_messages: MessageWriter<ImmersiveModeRejected>,
) {
    let mut entry_is_reserved =
        !active_immersive_modes.is_empty() || !pending_immersive_entries.is_empty();
    for entry_request in entry_requests.read().copied() {
        if entry_is_reserved {
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
