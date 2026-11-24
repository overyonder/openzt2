use super::stage_voice_acoustics::{PendingStageVoice, StageVoiceAcoustics};
use bevy::{
    audio::{AudioPlayer, Volume},
    prelude::*,
};

use super::audio_player_spawning::{
    calculate_mixed_audio_gain, AudioCueSelection, AudioSourceGain, DeferredAudioPlayback,
};
use super::audio_stage_playback_projection::EnvironmentVoiceGain;
use super::spatial_cue_distance::{SpatialCueDistance, SpatialCueListener};
use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};
use openzt2_interface_state::settings::audio_settings_types::AudioSettings;

pub(super) fn start_deferred_audio_playback_after_delays_finish(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<AudioSettings>,
    audio_assets: Res<Assets<AudioAsset>>,
    indexes: Res<AudioAssets>,
    listener: SpatialCueListener,
    mut deferred_audio: Query<(
        Entity,
        &AudioCueSelection,
        &AudioSourceGain,
        &mut DeferredAudioPlayback,
        Option<&mut SpatialCueDistance>,
        Option<&ChildOf>,
        Has<StageVoiceAcoustics>,
    )>,
    environment_gains: Query<&EnvironmentVoiceGain>,
) {
    let (listener_pose, _) = listener.snapshot();
    let mixer = indexes.get(&audio_assets).and_then(|view| view.mixer());
    for (
        playback_entity,
        selection,
        source_gain,
        mut deferred_playback,
        distance,
        owner,
        acoustics,
    ) in &mut deferred_audio
    {
        deferred_playback.timer.tick(time.delta());
        if deferred_playback.timer.is_finished() {
            let gain = distance
                .zip(selection.emitter)
                .map_or(1.0, |(mut distance, emitter)| {
                    listener.emitter(emitter).map_or(0.0, |(position, water)| {
                        let mut next = *distance;
                        let gain = listener_pose.gain(&mut next, position, water, mixer);
                        if distance.retained_in_range != next.retained_in_range {
                            distance.retained_in_range = next.retained_in_range;
                        }
                        gain
                    })
                });
            let mut playback = deferred_playback.settings;
            let environment_gain = owner
                .filter(|_| selection.emitter.is_none())
                .and_then(|owner| environment_gains.get(owner.parent()).ok())
                .map_or(1.0, |gain| gain.0);
            playback.volume = Volume::Linear(
                calculate_mixed_audio_gain(
                    &settings,
                    selection.usage,
                    selection.emitter.is_some(),
                    source_gain.0,
                ) * gain
                    * environment_gain,
            );
            let mut entity = commands.entity(playback_entity);
            let clip = deferred_playback.clip.clone();
            if acoustics {
                entity.insert((PendingStageVoice(clip), playback));
            } else {
                entity.insert((AudioPlayer(clip), playback));
            }
            entity.remove::<DeferredAudioPlayback>();
        }
    }
}
