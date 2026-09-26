//! Master, music, two-dimensional, and three-dimensional volume actions.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

/// Audio-options operations separated during source lowering.
///
/// Slider values are read from the activated Bevy UI
/// entity; no source widget name or generic setting command reaches runtime.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiAudioSettingActionRecord {
    pub trigger: UiTrigger,
    pub action: UiAudioSettingAction,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAudioSettingAction {
    SetMasterVolume,
    SetMusicVolume,
    SetTwoDimensionalVolume,
    SetThreeDimensionalVolume,
}
