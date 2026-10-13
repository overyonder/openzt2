use bevy::prelude::*;

use super::immersive_mode_state_types::ImmersiveMode;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnterImmersiveMode {
    pub mode: ImmersiveMode,
    pub controller: Entity,
    pub subject: Option<Entity>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitImmersiveMode {
    pub controller: Entity,
    pub reason: ModeExitReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeExitReason {
    Cancelled,
    SubjectRemoved,
    GamePhaseChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeEntryFailure {
    Busy,
    InvalidController,
    MissingSubject,
    WrongSubject,
    MissingTool,
    RuleDenied,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImmersiveModeRejected {
    pub mode: ImmersiveMode,
    pub reason: ModeEntryFailure,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImmersiveModeEntered {
    pub mode: ImmersiveMode,
    pub subject: Option<Entity>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImmersiveModeExited {
    pub mode: ImmersiveMode,
    pub reason: ModeExitReason,
}
