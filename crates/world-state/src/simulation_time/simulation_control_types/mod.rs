use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationControl {
    pub speed_tier: u8,
    pub paused: bool,
    pause_state_before_temporary_ui_hold: Option<bool>,
}

impl SimulationControl {
    pub const fn new(speed_tier: u8, paused: bool) -> Self {
        Self {
            speed_tier,
            paused,
            pause_state_before_temporary_ui_hold: None,
        }
    }

    pub fn requested_pause_state(&self) -> bool {
        self.pause_state_before_temporary_ui_hold
            .unwrap_or(self.paused)
    }

    pub fn temporary_ui_pause_is_held(&self) -> bool {
        self.pause_state_before_temporary_ui_hold.is_some()
    }

    pub fn apply_pause_request(&mut self, request: SimulationPauseRequest) -> bool {
        let paused = match request {
            SimulationPauseRequest::Set(paused) => paused,
            SimulationPauseRequest::ToggleRequested => !self.requested_pause_state(),
        };
        self.set_requested_pause_state(paused)
    }

    fn set_requested_pause_state(&mut self, paused: bool) -> bool {
        let previous = self.paused;
        if let Some(previous_pause) = &mut self.pause_state_before_temporary_ui_hold {
            // Explicit pause requests, including an options overlay's restore,
            // remain authoritative when the final temporary UI hold ends.
            *previous_pause = paused;
        } else {
            self.paused = paused;
        }
        self.paused != previous
    }

    pub fn set_temporary_ui_pause_held(&mut self, held: bool) -> bool {
        let previous = self.paused;
        if held {
            let _ = self
                .pause_state_before_temporary_ui_hold
                .get_or_insert(self.paused);
            self.paused = true;
        } else if let Some(previous_pause) = self.pause_state_before_temporary_ui_hold.take() {
            self.paused = previous_pause;
        }
        self.paused != previous
    }

    pub fn temporary_ui_pause_hold_matches_current_control(&self, held: bool) -> bool {
        self.pause_state_before_temporary_ui_hold.is_some() == held && (!held || self.paused)
    }
}

/// Intent belongs to its authored node or pending receiver. Effective holding
/// also requires its post-command ancestor chain to be shown.
#[derive(Component)]
pub struct AuthoredUiTemporarySimulationPause(pub bool);

impl AuthoredUiTemporarySimulationPause {
    pub const fn holding() -> Self {
        Self(true)
    }

    pub const fn hold_is_requested(&self) -> bool {
        self.0
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetSimulationSpeed {
    pub tier: u8,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationPauseRequest {
    Set(bool),
    ToggleRequested,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationControlChanged {
    pub speed_tier: u8,
    pub paused: bool,
}

/// A UI node shown while the simulation is paused.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiSimulationPausedVisibility;
