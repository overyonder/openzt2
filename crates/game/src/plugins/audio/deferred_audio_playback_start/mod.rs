use bevy::{
    audio::{AudioPlayer, Volume},
    prelude::*,
};

use super::audio_player_spawning::{
    calculate_mixed_audio_gain, AudioCueSelection, AudioSourceGain, DeferredAudioPlayback,
};
use crate::plugins::settings::audio_settings_types::AudioSettings;

pub(super) fn start_deferred_audio_playback_after_delays_finish(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<AudioSettings>,
    mut deferred_audio: Query<(
        Entity,
        &AudioCueSelection,
        &AudioSourceGain,
        &mut DeferredAudioPlayback,
    )>,
) {
    for (playback_entity, selection, source_gain, mut deferred_playback) in &mut deferred_audio {
        deferred_playback.timer.tick(time.delta());
        if deferred_playback.timer.is_finished() {
            deferred_playback.settings.volume = Volume::Linear(calculate_mixed_audio_gain(
                &settings,
                selection.usage,
                selection.emitter.is_some(),
                source_gain.0,
            ));
            commands
                .entity(playback_entity)
                .insert((
                    AudioPlayer(deferred_playback.clip.clone()),
                    deferred_playback.settings,
                ))
                .remove::<DeferredAudioPlayback>();
        }
    }
}
