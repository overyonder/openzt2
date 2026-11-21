use bevy::{
    audio::{AudioPlayer, Volume},
    prelude::*,
};

use super::audio_player_spawning::{
    calculate_mixed_audio_gain, AudioCueSelection, AudioSourceGain, DeferredAudioPlayback,
};
use super::spatial_cue_distance::{SpatialCueDistance, SpatialCueListener};
use openzt2_interface_state::settings::audio_settings_types::AudioSettings;

pub(super) fn start_deferred_audio_playback_after_delays_finish(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<AudioSettings>,
    listener: SpatialCueListener,
    mut deferred_audio: Query<(
        Entity,
        &AudioCueSelection,
        &AudioSourceGain,
        &mut DeferredAudioPlayback,
        Option<&SpatialCueDistance>,
    )>,
) {
    for (playback_entity, selection, source_gain, mut deferred_playback, distance) in
        &mut deferred_audio
    {
        deferred_playback.timer.tick(time.delta());
        if deferred_playback.timer.is_finished() {
            let gain = distance
                .copied()
                .zip(selection.emitter)
                .map_or(1.0, |(distance, emitter)| listener.gain(distance, emitter));
            let mut playback = deferred_playback.settings;
            playback.volume = Volume::Linear(
                calculate_mixed_audio_gain(
                    &settings,
                    selection.usage,
                    selection.emitter.is_some(),
                    source_gain.0,
                ) * gain,
            );
            commands
                .entity(playback_entity)
                .insert((AudioPlayer(deferred_playback.clip.clone()), playback))
                .remove::<DeferredAudioPlayback>();
        }
    }
}
