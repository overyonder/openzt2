use super::audio_request_lanes::{ProducedAudioRequest, WATER_STAGE};
use bevy::prelude::*;

use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};

use super::{
    audio_environment_types::AudioStage,
    audio_playback_message_types::{PlayAudioCue, StopAudioCue},
};

pub(super) fn project_changed_audio_stages_into_playback_requests(
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    stages: Query<(Entity, &AudioStage)>,
    changed: Query<(Entity, &AudioStage), Changed<AudioStage>>,
    mut removed_stages: RemovedComponents<AudioStage>,
    mut requests: MessageWriter<ProducedAudioRequest<WATER_STAGE>>,
) {
    for stage_owner in removed_stages.read() {
        requests.write(ProducedAudioRequest::Stop(StopAudioCue {
            emitter: Some(stage_owner),
            selection: stage_owner.to_bits(),
        }));
    }
    let audio_assets_changed = audio_asset_index.is_changed();
    let Some(audio_asset_view) = audio_asset_index.get(&audio_assets) else {
        return;
    };
    let candidates = audio_assets_changed
        .then(|| stages.iter())
        .into_iter()
        .flatten()
        .chain(
            (!audio_assets_changed)
                .then(|| changed.iter())
                .into_iter()
                .flatten(),
        );
    for (stage_owner, selected_stage) in candidates {
        requests.write(ProducedAudioRequest::Stop(StopAudioCue {
            emitter: Some(stage_owner),
            selection: stage_owner.to_bits(),
        }));
        let Some(stage) = audio_asset_view.stage(selected_stage.0) else {
            continue;
        };
        if let Some(loop_cue) = stage.loop_cue {
            requests.write(ProducedAudioRequest::Play(PlayAudioCue {
                cue: loop_cue,
                emitter: Some(stage_owner),
                selection: stage_owner.to_bits(),
                priority: 64,
                force_looped: true,
            }));
        }
        for ambient_cue in &stage.ambient_cues {
            requests.write(ProducedAudioRequest::Play(PlayAudioCue {
                cue: *ambient_cue,
                emitter: Some(stage_owner),
                selection: stage_owner.to_bits(),
                priority: 32,
                force_looped: false,
            }));
        }
    }
}
