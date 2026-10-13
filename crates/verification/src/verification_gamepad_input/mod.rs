use bevy::{
    ecs::system::SystemParam,
    input::gamepad::{
        GamepadConnection, GamepadConnectionEvent, RawGamepadButtonChangedEvent, RawGamepadEvent,
    },
    prelude::*,
};
use openzt2_game_data::ui_document::action::UiTrigger;

use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::ActionSource;
use openzt2_interface_state::input::input_types::ActiveInputDevice;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::input::input_types::PrimaryPointerInputState;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
    VerificationSessionStage,
};

/// Evidence belongs to the injected device; Bevy's Gamepad owns its input state.
#[derive(Component, Default)]
pub(super) struct VerificationGamepadEvidence {
    confirm_requests: u32,
    cancel_requests: u32,
    primary_pointer_requests: u32,
    other_requests: u32,
    press_activations: u32,
}

#[derive(SystemParam)]
pub(crate) struct VerificationGamepadInput<'w, 's> {
    commands: Commands<'w, 's>,
    devices: Query<'w, 's, (Entity, Option<&'static Gamepad>), With<VerificationGamepadEvidence>>,
    connections: MessageWriter<'w, GamepadConnectionEvent>,
    raw_events: MessageWriter<'w, RawGamepadEvent>,
}

impl VerificationGamepadInput<'_, '_> {
    pub(super) fn connect(&mut self) -> Result<(), String> {
        if !self.devices.is_empty() {
            return Err("the journey gamepad is already connected".to_owned());
        }
        let entity = self
            .commands
            .spawn(VerificationGamepadEvidence::default())
            .id();
        let event = GamepadConnectionEvent::new(
            entity,
            GamepadConnection::Connected {
                name: "verification controller".to_owned(),
                vendor_id: None,
                product_id: None,
            },
        );
        self.raw_events
            .write(RawGamepadEvent::Connection(event.clone()));
        self.connections.write(event);
        Ok(())
    }

    pub(super) fn is_connected(&self) -> bool {
        self.devices
            .single()
            .is_ok_and(|(_, gamepad)| gamepad.is_some())
    }

    pub(super) fn press_or_release_button(
        &mut self,
        button: GamepadButton,
        pressed: bool,
    ) -> Result<(), String> {
        let (entity, gamepad) = self.devices.single().map_err(|error| error.to_string())?;
        if gamepad.is_none() {
            return Err("the journey gamepad has no connected Bevy input owner".to_owned());
        }
        self.raw_events
            .write(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(
                entity,
                button,
                if pressed { 1.0 } else { 0.0 },
            )));
        Ok(())
    }

    pub(super) fn disconnect(&mut self) -> bool {
        let empty = self.devices.is_empty();
        for (entity, gamepad) in &self.devices {
            if let Some(gamepad) = gamepad {
                for button in gamepad.get_pressed() {
                    self.raw_events.write(RawGamepadEvent::Button(
                        RawGamepadButtonChangedEvent::new(entity, *button, 0.0),
                    ));
                }
                let event = GamepadConnectionEvent::new(entity, GamepadConnection::Disconnected);
                self.raw_events
                    .write(RawGamepadEvent::Connection(event.clone()));
                self.connections.write(event);
            } else {
                // Bevy removes Gamepad first. Retire only this journey's device
                // after normal disconnect processing has released its pointer.
                self.commands.entity(entity).despawn();
            }
        }
        empty
    }
}

pub(super) fn disconnect_journey_gamepads_after_completion_or_failure(
    session: Res<VerificationJourneySession>,
    report: Res<VerificationJourneyReport>,
    mut input: VerificationGamepadInput,
) {
    if session.finished
        || session.stage != VerificationSessionStage::Journey
        || !report.failures.is_empty()
    {
        input.disconnect();
    }
}

pub(super) fn observe_journey_controller_requests_and_activations(
    mut actions: MessageReader<ActionRequest>,
    mut activations: MessageReader<UiNodeActivated>,
    mut devices: Query<&mut VerificationGamepadEvidence>,
) {
    for request in actions.read() {
        let ActionSource::Controller(entity) = request.source else {
            continue;
        };
        let Ok(mut evidence) = devices.get_mut(entity) else {
            continue;
        };
        let count = match request.action {
            GameAction::Confirm => &mut evidence.confirm_requests,
            GameAction::Cancel => &mut evidence.cancel_requests,
            GameAction::PrimaryPointer => &mut evidence.primary_pointer_requests,
            _ => &mut evidence.other_requests,
        };
        *count += 1;
    }
    for activation in activations.read() {
        let ActionSource::Controller(entity) = activation.source else {
            continue;
        };
        if matches!(activation.trigger, UiTrigger::Press | UiTrigger::Submit) {
            // A momentary Confirm also emits its intentional On/Off pair.
            // Count activation requests, not those presentation transitions.
            if let Ok(mut evidence) = devices.get_mut(entity) {
                evidence.press_activations += 1;
            }
        }
    }
}

pub(super) fn collect_journey_controller_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    devices: Query<(&VerificationGamepadEvidence, Option<&Gamepad>)>,
    active: Res<ActiveInputDevice>,
    pointer: Res<PrimaryPointerInputState>,
) {
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let mut counts = [0_u32; 7];
    for (evidence, gamepad) in &devices {
        for (count, value) in counts.iter_mut().zip([
            evidence.confirm_requests,
            evidence.cancel_requests,
            evidence.primary_pointer_requests,
            evidence.other_requests,
            evidence.press_activations,
            u32::from(gamepad.is_some()),
            gamepad.map_or(0, |gamepad| {
                u32::try_from(gamepad.get_pressed().count()).unwrap_or(u32::MAX)
            }),
        ]) {
            *count += value;
        }
    }
    for (name, count) in [
        "controller_confirm_requests",
        "controller_cancel_requests",
        "controller_primary_pointer_requests",
        "controller_other_requests",
        "controller_ui_press_activations",
        "controller_connected_devices",
        "controller_held_buttons",
    ]
    .into_iter()
    .zip(counts)
    {
        report
            .current_facts
            .insert(name.to_owned(), f64::from(count));
    }
    report.current_facts.insert(
        "controller_active_device".to_owned(),
        f64::from(u8::from(matches!(
            active.source,
            ActionSource::Controller(_)
        ))),
    );
    report.current_facts.insert(
        "controller_primary_pointer_held".to_owned(),
        f64::from(u8::from(pointer.pressed)),
    );
}
