use bevy::{
    audio::{AudioPlayer, AudioSource, PlaybackSettings, Volume},
    prelude::*,
};
use openzt2_game_data::audio::AudioUsage;

use crate::plugins::settings::audio_settings_types::AudioSettings;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AudioCueSelection {
    pub(super) emitter: Option<Entity>,
    pub(super) selection: u64,
    pub(super) usage: AudioUsage,
    pub(super) priority: u8,
}

#[derive(Component, Clone, Copy, Debug)]
pub(super) struct AudioSourceGain(pub(super) f32);

#[derive(Component)]
pub(super) struct DeferredAudioPlayback {
    pub(super) timer: Timer,
    pub(super) clip: Handle<AudioSource>,
    pub(super) settings: PlaybackSettings,
}

pub(super) fn calculate_mixed_audio_gain(
    settings: &AudioSettings,
    usage: AudioUsage,
    spatial: bool,
    source_gain: f32,
) -> f32 {
    if settings.muted {
        return 0.0;
    }
    let category_volume_percent = if usage == AudioUsage::Music {
        settings.music_volume_percent
    } else if spatial {
        settings.three_dimensional_effect_volume_percent
    } else {
        settings.two_dimensional_effect_volume_percent
    };
    source_gain * f32::from(settings.master_volume_percent) * f32::from(category_volume_percent)
        / 10_000.0
}

pub(super) fn spawn_audio_player_or_deferred_playback(
    commands: &mut Commands,
    clip: Handle<AudioSource>,
    request: AudioCueSelection,
    playback_settings: PlaybackSettings,
    audio_settings: &AudioSettings,
    delay_seconds: f32,
) -> Entity {
    let source_gain = playback_settings.volume.to_linear();
    let playback_settings = PlaybackSettings {
        volume: Volume::Linear(calculate_mixed_audio_gain(
            audio_settings,
            request.usage,
            request.emitter.is_some(),
            source_gain,
        )),
        ..playback_settings
    };
    let mut playback_entity = commands.spawn((request, AudioSourceGain(source_gain)));
    if delay_seconds > 0.0 {
        playback_entity.insert(DeferredAudioPlayback {
            timer: Timer::from_seconds(delay_seconds, TimerMode::Once),
            clip,
            settings: playback_settings,
        });
    } else {
        playback_entity.insert((AudioPlayer(clip), playback_settings));
    }
    let playback_entity = playback_entity.id();
    if let Some(emitter) = request.emitter {
        commands.entity(emitter).add_child(playback_entity);
    }
    playback_entity
}
