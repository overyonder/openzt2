//! Authored audio cue, mixing, and environmental policy.

use serde::{Deserialize, Serialize};

use crate::AssetId;

/// Audio facts contributed by one winning source document.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AudioDocument {
    pub mixer: Option<AudioMixerPolicy>,
    pub ambient: Option<AudioAmbientPolicy>,
    pub water: Option<AudioWaterPolicy>,
    pub crowd: Option<AudioCrowdPolicy>,
    pub cues: Vec<AudioCue>,
    pub stages: Vec<AudioStage>,
    pub soundscape_biomes: Vec<AudioSoundscapeBiome>,
    pub soundscape_contents: Vec<AudioSoundscapeContent>,
    pub soundscape_allowed_entities: Vec<AudioSoundscapeAllowedEntities>,
}

pub const VARIANT_LOOPED: u16 = 1 << 0;
pub const VARIANT_STREAMED: u16 = 1 << 1;
pub const VARIANT_RANDOM_VOLUME: u16 = 1 << 2;
pub const VARIANT_RANDOM_PITCH: u16 = 1 << 3;
pub const VARIANT_RANDOM_PAN: u16 = 1 << 4;
pub const VARIANT_KEEP_RESIDENT: u16 = 1 << 5;
pub const VARIANT_CUE_REFERENCE: u16 = 1 << 6;
pub const STAGE_INDOOR_OCCLUSION: u8 = 1 << 0;
pub const STAGE_KEEP_AMBIENCE: u8 = 1 << 1;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct AudioMixerPolicy {
    pub output_sample_rate: u32,
    pub output_channels: u8,
    pub voice_limit: u16,
    pub spatial_voice_limit: u16,
    pub spatial_in_range: f32,
    pub spatial_out_range: f32,
    pub max_world_distance: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct AudioAmbientPolicy {
    pub voice_limit: u16,
    pub delay_seconds: [f32; 2],
    pub probability: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AudioWaterPolicy {
    pub area_thresholds: [f32; 2],
    pub natural_stages: [Option<AssetId>; 3],
    pub tank_stages: [Option<AssetId>; 3],
    pub environment_transition_seconds: f32,
    pub lapping_max_distance: f32,
    pub lapping_gain_scale: f32,
    pub lapping_minimum_area: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AudioCrowdPolicy {
    pub thresholds: [f32; 8],
    pub cues: [Option<AssetId>; 5],
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum AudioUsage {
    Ui,
    Effect,
    Voice,
    Ambient,
    Music,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AudioCue {
    pub id: AssetId,
    pub name: String,
    pub usage: AudioUsage,
    pub variants: Vec<AudioVariant>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AudioVariant {
    /// An ordinary audio asset path, or a cue name when `VARIANT_CUE_REFERENCE` is set.
    pub source: String,
    pub source_id: AssetId,
    pub weight: f32,
    pub gain: [f32; 2],
    pub pitch: [f32; 2],
    pub delay_seconds: [f32; 2],
    /// Explicit authored spatial range. `None` leaves the range to the mixer policy.
    pub distance: Option<[f32; 2]>,
    pub soften_gain: f32,
    pub filter_layer: String,
    pub flags: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AudioStage {
    pub id: AssetId,
    pub name: String,
    pub room_type: String,
    pub filter_layer: String,
    pub room_effect: f32,
    pub occlusion: f32,
    pub exclusion: f32,
    pub obstruction: f32,
    pub loop_cue: Option<AssetId>,
    pub ambient_cues: Vec<AssetId>,
    pub flags: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AudioSoundscapeContent {
    pub biome: AssetId,
    pub name: String,
    pub loop_cue: Option<AssetId>,
    pub cues: Vec<AudioSoundscapeCue>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AudioSoundscapeAllowedEntities {
    pub biome: AssetId,
    pub name: String,
    pub allowed_entities: Vec<AssetId>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AudioSoundscapeBiome {
    pub biome: AssetId,
    pub name: String,
    pub directory: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct AudioSoundscapeCue {
    pub cue: AssetId,
    pub weight: f32,
    pub day_mask: u8,
}
