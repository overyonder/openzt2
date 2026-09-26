use bevy::prelude::*;

use super::immersive_mode_state_types::ImmersiveMode;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EnterImmersiveMode {
    pub(crate) mode: ImmersiveMode,
    pub(crate) controller: Entity,
    pub(crate) subject: Option<Entity>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExitImmersiveMode {
    pub(crate) controller: Entity,
    pub(crate) reason: ModeExitReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModeExitReason {
    Cancelled,
    SubjectRemoved,
    GamePhaseChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModeEntryFailure {
    Busy,
    InvalidController,
    MissingSubject,
    WrongSubject,
    MissingTool,
    RuleDenied,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImmersiveModeRejected {
    pub(crate) mode: ImmersiveMode,
    pub(crate) reason: ModeEntryFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImmersiveModeEntered {
    pub(crate) mode: ImmersiveMode,
    pub(crate) subject: Option<Entity>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImmersiveModeExited {
    pub(crate) mode: ImmersiveMode,
    pub(crate) reason: ModeExitReason,
}
