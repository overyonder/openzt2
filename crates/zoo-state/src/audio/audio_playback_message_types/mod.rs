use bevy::{audio::AudioSource, prelude::*};
use openzt2_game_data::AssetId;

#[derive(Message, Clone, Copy, Debug)]
pub struct PlayAudioCue {
    pub cue: AssetId,
    pub emitter: Option<Entity>,
    pub selection: u64,
    pub priority: u8,
    pub force_looped: bool,
}

#[derive(Message, Clone, Debug)]
pub struct PlayAudioClip {
    pub clip: Handle<AudioSource>,
    pub emitter: Option<Entity>,
    pub selection: u64,
    pub force_looped: bool,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StopAudioCue {
    pub emitter: Option<Entity>,
    pub selection: u64,
}
