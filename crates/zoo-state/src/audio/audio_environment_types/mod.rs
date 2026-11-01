use bevy::prelude::*;
use openzt2_game_assets::audio::audio_asset_types::AudioAsset;
use openzt2_game_data::AssetId;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct WorldAudioPreset(pub Handle<AudioAsset>);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioStage(pub AssetId);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioSoundscape {
    pub biome: AssetId,
    pub selection: u64,
    pub day_mask: u8,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmbientAudioEmitter {
    pub definition: AssetId,
    pub cue: AssetId,
    pub selection: u64,
}
