use bevy::prelude::*;

/// Accepted user audio policy in the authored 0..=100 slider range.
#[derive(Resource, serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSettings {
    #[serde(rename = "master")]
    pub master_volume_percent: u8,
    #[serde(rename = "music")]
    pub music_volume_percent: u8,
    #[serde(rename = "effects_2d")]
    pub two_dimensional_effect_volume_percent: u8,
    #[serde(rename = "effects_3d")]
    pub three_dimensional_effect_volume_percent: u8,
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_volume_percent: 100,
            music_volume_percent: 100,
            two_dimensional_effect_volume_percent: 100,
            three_dimensional_effect_volume_percent: 100,
            muted: false,
        }
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplaceAudioSettingsRequest(pub AudioSettings);
