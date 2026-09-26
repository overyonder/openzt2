use bevy::{
    audio::{AudioSink, AudioSinkPlayback, SpatialAudioSink, Volume},
    prelude::*,
};

use crate::plugins::settings::audio_settings_types::AudioSettings;

use super::audio_player_spawning::{
    calculate_mixed_audio_gain, AudioCueSelection, AudioSourceGain,
};

pub(super) fn project_changed_audio_settings_into_live_audio_sinks(
    settings: Res<AudioSettings>,
    mut sinks: Query<(&AudioCueSelection, &AudioSourceGain, &mut AudioSink)>,
    mut spatial_sinks: Query<(&AudioCueSelection, &AudioSourceGain, &mut SpatialAudioSink)>,
) {
    if !settings.is_changed() {
        return;
    }
    for (selection, source_gain, mut sink) in &mut sinks {
        sink.set_volume(Volume::Linear(calculate_mixed_audio_gain(
            &settings,
            selection.usage,
            false,
            source_gain.0,
        )));
    }
    for (selection, source_gain, mut sink) in &mut spatial_sinks {
        sink.set_volume(Volume::Linear(calculate_mixed_audio_gain(
            &settings,
            selection.usage,
            true,
            source_gain.0,
        )));
    }
}
