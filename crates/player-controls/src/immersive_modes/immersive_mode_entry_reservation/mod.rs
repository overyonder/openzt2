use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;

use super::admission::{reserve_mode, ModeReservation};
use super::immersive_mode_message_types::EnterImmersiveMode;
use super::immersive_mode_message_types::ExitImmersiveMode;
use super::immersive_mode_message_types::ImmersiveModeRejected;
use super::immersive_mode_message_types::ModeEntryFailure;
use super::immersive_mode_message_types::ModeExitReason;
use super::immersive_mode_state_types::ActiveImmersiveMode;
use super::immersive_mode_state_types::ImmersiveMode;

#[derive(Component)]
pub(crate) struct DeferredImmersiveSwitch(EnterImmersiveMode);

/// The camera buttons are a radio set: pressing one while another view is open
/// leaves that view and then enters the new one.
const fn is_camera_button_view(mode: ImmersiveMode) -> bool {
    matches!(
        mode,
        ImmersiveMode::FirstPerson
            | ImmersiveMode::GuestView
            | ImmersiveMode::Photo
            | ImmersiveMode::SuperStaff
            | ImmersiveMode::FossilSearch
            | ImmersiveMode::FossilAssembly
    )
}

pub(super) fn reserve_one_pending_immersive_mode_entry_from_valid_controller_request(
    mut entry_requests: MessageReader<EnterImmersiveMode>,
    active_immersive_modes: Query<(Entity, &ActiveImmersiveMode)>,
    pending_immersive_entries: Query<(), With<ModeReservation>>,
    existing_controller_entities: Query<()>,
    mut deferred_switches: Query<(Entity, &mut DeferredImmersiveSwitch)>,
    mut commands: Commands,
    mut rejected_entry_messages: MessageWriter<ImmersiveModeRejected>,
    mut exit_requests: MessageWriter<ExitImmersiveMode>,
) {
    let mut entry_is_reserved =
        !active_immersive_modes.is_empty() || !pending_immersive_entries.is_empty();
    let queued = if entry_is_reserved {
        None
    } else if let Ok((entity, deferred)) = deferred_switches.single() {
        commands.entity(entity).despawn();
        Some(deferred.0)
    } else {
        None
    };
    let mut next_switch = None;
    for entry_request in queued.into_iter().chain(entry_requests.read().copied()) {
        if let Some((controller, _)) = active_immersive_modes.iter().next().filter(|(_, active)| {
            entry_request.mode != active.mode
                && is_camera_button_view(active.mode)
                && is_camera_button_view(entry_request.mode)
        }) {
            exit_requests.write(ExitImmersiveMode {
                controller,
                reason: ModeExitReason::Cancelled,
            });
            next_switch = Some(entry_request);
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
            reserve_mode(&mut commands, entry_request);
            entry_is_reserved = true;
        }
    }
    if let Some(request) = next_switch {
        if let Ok((_, mut current)) = deferred_switches.single_mut() {
            current.0 = request;
        } else {
            commands.spawn((
                DeferredImmersiveSwitch(request),
                DespawnOnExit(GamePhase::InGame),
            ));
        }
    }
}
