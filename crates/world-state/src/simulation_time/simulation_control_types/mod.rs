use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationControl {
    pub speed_tier: u8,
    pub paused: bool,
    requested_pause: bool,
    temporary_ui_pause: bool,
    immersive_pause: bool,
}

impl SimulationControl {
    pub const fn new(speed_tier: u8, paused: bool) -> Self {
        Self {
            speed_tier,
            paused,
            requested_pause: paused,
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

    pub fn apply_pause_request(&mut self, request: SimulationPauseRequest) -> bool {
        self.requested_pause = match request {
            SimulationPauseRequest::Set(paused) => paused,
            SimulationPauseRequest::ToggleRequested => !self.requested_pause,
        };
        self.update_effective_pause()
    }

    pub fn project_pause_claims(
        &mut self,
        temporary_ui_pause: bool,
        immersive_pause: bool,
    ) -> bool {
        self.temporary_ui_pause = temporary_ui_pause;
        self.immersive_pause = immersive_pause;
        self.update_effective_pause()
    }

    pub const fn pause_claims_match(
        &self,
        temporary_ui_pause: bool,
        immersive_pause: bool,
    ) -> bool {
        self.temporary_ui_pause == temporary_ui_pause && self.immersive_pause == immersive_pause
    }

    fn update_effective_pause(&mut self) -> bool {
        let previous = self.paused;
        self.paused = self.requested_pause || self.temporary_ui_pause || self.immersive_pause;
        self.paused != previous
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
