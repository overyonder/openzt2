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

#[derive(Message, Clone, Copy, Debug)]
pub struct PlayPositionedAudioCue {
    pub cue: AssetId,
    pub world_position: Vec3,
    pub selection: u64,
    pub priority: u8,
    pub force_looped: bool,
    pub listener_above_water: bool,
    pub listener_below_water: bool,
    pub admission: Option<PositionedAudioAdmission>,
}

#[derive(Clone, Copy, Debug)]
pub struct PositionedAudioAdmission {
    pub group: AssetId,
    pub maximum_voices: u32,
    pub maximum_distance_squared: f32,
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
