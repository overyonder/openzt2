use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SimulationSpeedTier(pub u8);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SimulationPaused(pub bool);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationControl {
    requested_pause: bool,
    temporary_ui_pause: bool,
    immersive_pause: bool,
}

impl SimulationControl {
    #[must_use]
    pub const fn new(requested_pause: bool) -> Self {
        Self {
            requested_pause,
            temporary_ui_pause: false,
            immersive_pause: false,
        }
    }

    pub const fn requested_pause_state(&self) -> bool {
        self.requested_pause
    }

    pub const fn temporary_ui_pause_is_held(&self) -> bool {
        self.temporary_ui_pause
    }

    pub const fn apply_pause_request(&mut self, request: SimulationPauseRequest) {
        self.requested_pause = match request {
            SimulationPauseRequest::Set(paused) => paused,
            SimulationPauseRequest::ToggleRequested => !self.requested_pause,
        };
    }

    pub const fn project_pause_claims(&mut self, temporary_ui_pause: bool, immersive_pause: bool) {
        self.temporary_ui_pause = temporary_ui_pause;
        self.immersive_pause = immersive_pause;
    }

    pub const fn effective_pause(&self) -> SimulationPaused {
        SimulationPaused(self.requested_pause || self.temporary_ui_pause || self.immersive_pause)
    }
}

#[derive(Component)]
pub struct ImmersiveSimulationPauseClaim;

/// A live authored node or pending receiver that requests a pause while shown.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct AuthoredUiTemporarySimulationPause(pub bool);

impl AuthoredUiTemporarySimulationPause {
    pub const fn holding() -> Self {
        Self(true)
    }

    pub const fn hold_is_requested(&self) -> bool {
        self.0
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct UiDocumentPauseTransfer(pub bool);

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

/// An authored toggle whose selected state follows effective simulation pause.
#[derive(Component)]
pub struct UiSimulationPauseToggle;
