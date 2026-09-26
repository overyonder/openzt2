use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AudioStage(pub(crate) AssetId);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AudioSoundscape {
    pub(crate) biome: AssetId,
    pub(crate) selection: u64,
    pub(crate) day_mask: u8,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AmbientAudioEmitter {
    pub(crate) definition: AssetId,
    pub(crate) cue: AssetId,
    pub(crate) selection: u64,
}
