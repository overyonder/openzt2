use bevy::{audio::AudioSource, prelude::*};
use openzt2_game_data::AssetId;

#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct PlayAudioCue {
    pub(crate) cue: AssetId,
    pub(crate) emitter: Option<Entity>,
    pub(crate) selection: u64,
    pub(crate) priority: u8,
    pub(crate) force_looped: bool,
}

#[derive(Message, Clone, Debug)]
pub(crate) struct PlayAudioClip {
    pub(crate) clip: Handle<AudioSource>,
    pub(crate) emitter: Option<Entity>,
    pub(crate) selection: u64,
    pub(crate) force_looped: bool,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StopAudioCue {
    pub(crate) emitter: Option<Entity>,
    pub(crate) selection: u64,
}
