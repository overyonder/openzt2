use bevy::{
    audio::{PlaybackMode, PlaybackSettings},
    prelude::*,
};
use openzt2_game_data::audio::AudioUsage;

use crate::plugins::settings::audio_settings_types::AudioSettings;

use super::{
    audio_playback_message_types::PlayAudioClip,
    audio_player_spawning::{spawn_audio_player_or_deferred_playback, AudioCueSelection},
};

pub(super) fn execute_direct_audio_clip_playback_requests(
    mut commands: Commands,
    mut requests: MessageReader<PlayAudioClip>,
    audio_settings: Res<AudioSettings>,
) {
    for request in requests.read() {
        let playback_settings = PlaybackSettings {
            mode: if request.force_looped {
                PlaybackMode::Loop
            } else {
                PlaybackMode::Despawn
            },
            spatial: request.emitter.is_some(),
            ..default()
        };
        spawn_audio_player_or_deferred_playback(
            &mut commands,
            request.clip.clone(),
            AudioCueSelection {
                emitter: request.emitter,
                selection: request.selection,
                usage: AudioUsage::Ui,
                priority: 128,
            },
            playback_settings,
            &audio_settings,
            0.0,
        );
    }
}
