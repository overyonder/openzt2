use bevy::{
    audio::{AudioSink, AudioSinkPlayback, SpatialAudioSink, Volume},
    prelude::*,
};

use openzt2_interface_state::settings::audio_settings_types::AudioSettings;

use super::audio_player_spawning::{
    calculate_mixed_audio_gain, AudioCueSelection, AudioSourceGain,
};
use super::spatial_cue_distance::{SpatialCueDistance, SpatialCueListener};

pub(super) fn project_changed_audio_settings_into_live_audio_sinks(
    settings: Res<AudioSettings>,
    listener: SpatialCueListener,
    mut sinks: Query<(&AudioCueSelection, &AudioSourceGain, &mut AudioSink)>,
    mut spatial_sinks: Query<(
        &AudioCueSelection,
        &AudioSourceGain,
        Option<&SpatialCueDistance>,
        &mut SpatialAudioSink,
    )>,
) {
    if settings.is_changed() {
        for (selection, source_gain, mut sink) in &mut sinks {
            sink.set_volume(Volume::Linear(calculate_mixed_audio_gain(
                &settings,
                selection.usage,
                false,
                source_gain.0,
            )));
        }
    }
    for (selection, source_gain, distance, mut sink) in &mut spatial_sinks {
        let distance_gain =
            if let Some((distance, emitter)) = distance.copied().zip(selection.emitter) {
                listener.gain(distance, emitter)
            } else if settings.is_changed() {
                1.0
            } else {
                continue;
            };
        let gain = calculate_mixed_audio_gain(&settings, selection.usage, true, source_gain.0)
            * distance_gain;
        if sink.volume().to_linear().to_bits() != gain.to_bits() {
            sink.set_volume(Volume::Linear(gain));
        }
    }
}
