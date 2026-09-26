use bevy::{prelude::*, window::PresentMode};

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Windowed,
    BorderlessFullscreen,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePacing {
    VSync,
    Immediate,
    Mailbox,
}

impl FramePacing {
    pub const fn bevy_present_mode(self) -> PresentMode {
        match self {
            Self::VSync => PresentMode::Fifo,
            Self::Immediate => PresentMode::Immediate,
            Self::Mailbox => PresentMode::Mailbox,
        }
    }
}

#[derive(Resource, serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplaySettings {
    pub width: u32,
    pub height: u32,
    pub mode: DisplayMode,
    pub pacing: FramePacing,
    pub ui_scale_permille: u16,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            width: 1600,
            height: 900,
            mode: DisplayMode::BorderlessFullscreen,
            pacing: FramePacing::VSync,
            ui_scale_permille: 1000,
        }
    }
}

/// A renderer-advertised window size. Colour depth was part of the original
/// `BFModeDesc`; the Bevy renderer selects a surface format independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DisplayResolution {
    pub width: u32,
    pub height: u32,
}

/// The renderer-advertised mode represented by one live options-screen row.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayResolutionChoice(pub DisplayResolution);

/// Screen modes exposed by the active renderer and monitor integration.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SupportedDisplayResolutions(pub Vec<DisplayResolution>);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplaceDisplaySettingsRequest(pub DisplaySettings);

/// Select a renderer-advertised resolution while preserving the other live
/// display policy fields.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectScreenResolutionRequest(pub DisplayResolution);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplaySettingsRejectionReason {
    InvalidResolution,
    InvalidScale,
}
