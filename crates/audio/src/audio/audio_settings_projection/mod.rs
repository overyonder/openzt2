use bevy::{
    audio::{AudioSink, AudioSinkPlayback, SpatialAudioSink, Volume},
    prelude::*,
};

use openzt2_interface_state::settings::audio_settings_types::AudioSettings;

use super::audio_player_spawning::{
    calculate_mixed_audio_gain, AudioCueSelection, AudioSourceGain,
};
use super::audio_stage_playback_projection::EnvironmentVoiceGain;
use super::spatial_cue_distance::{SpatialCueDistance, SpatialCueListener};

pub(super) fn project_changed_audio_settings_into_live_audio_sinks(
    settings: Res<AudioSettings>,
    listener: SpatialCueListener,
    mut removed_listeners: RemovedComponents<bevy::audio::SpatialListener>,
    mut removed_contacts: RemovedComponents<
        openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterCameraContact,
    >,
    mut sinks: Query<(
        &AudioCueSelection,
        &AudioSourceGain,
        Option<&ChildOf>,
        &mut AudioSink,
    )>,
    changed_environment: Query<&Children, Changed<EnvironmentVoiceGain>>,
    environment_gains: Query<&EnvironmentVoiceGain>,
    changed_spatial: Query<
        Entity,
        (
            With<SpatialAudioSink>,
            Or<(
                Added<SpatialAudioSink>,
                Changed<GlobalTransform>,
                Changed<AudioSourceGain>,
                Changed<SpatialCueDistance>,
            )>,
        ),
    >,
    mut spatial_sinks: Query<(
        &AudioCueSelection,
        &AudioSourceGain,
        Option<&SpatialCueDistance>,
        &mut SpatialAudioSink,
    )>,
) {
    let update_nonspatial = |(selection, source_gain, owner, mut sink): (
        &AudioCueSelection,
        &AudioSourceGain,
        Option<&ChildOf>,
        Mut<AudioSink>,
    )| {
        let environment_gain = owner
            .and_then(|owner| environment_gains.get(owner.parent()).ok())
            .map_or(1.0, |gain| gain.0);
        let gain = calculate_mixed_audio_gain(&settings, selection.usage, false, source_gain.0)
            * environment_gain;
        if sink.volume().to_linear().to_bits() != gain.to_bits() {
            sink.set_volume(Volume::Linear(gain));
        }
    };
    if settings.is_changed() {
        for sink in &mut sinks {
            update_nonspatial(sink);
        }
    } else {
        let voices = changed_environment
            .iter()
            .flat_map(|children| children.iter());
        let mut changed = sinks.iter_many_mut(voices).matched();
        while let Some(sink) = changed.fetch_next() {
            update_nonspatial(sink);
        }
    }
    let (snapshot, listener_changed) = listener.snapshot();
    let update_all = settings.is_changed()
        | listener_changed
        | (removed_listeners.read().count() != 0)
        | (removed_contacts.read().count() != 0);
    let update = |(selection, source_gain, distance, mut sink): (
        &AudioCueSelection,
        &AudioSourceGain,
        Option<&SpatialCueDistance>,
        Mut<SpatialAudioSink>,
    )| {
        let distance_gain =
            distance
                .copied()
                .zip(selection.emitter)
                .map_or(1.0, |(policy, emitter)| {
                    listener.emitter(emitter).map_or(0.0, |(position, water)| {
                        snapshot.gain(policy, position, water)
                    })
                });
        let gain = calculate_mixed_audio_gain(&settings, selection.usage, true, source_gain.0)
            * distance_gain;
        if sink.volume().to_linear().to_bits() != gain.to_bits() {
            sink.set_volume(Volume::Linear(gain));
        }
    };
    if update_all {
        for sink in &mut spatial_sinks {
            update(sink);
        }
    } else {
        let mut changed = spatial_sinks
            .iter_many_mut(changed_spatial.iter())
            .matched();
        while let Some(sink) = changed.fetch_next() {
            update(sink);
        }
    }
}
