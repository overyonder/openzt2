use bevy::prelude::*;

use crate::assets::audio::audio_asset_types::{AudioAsset, AudioAssets};

use super::{
    audio_environment_types::AudioStage,
    audio_playback_message_types::{PlayAudioCue, StopAudioCue},
};

pub(super) fn project_changed_audio_stages_into_playback_requests(
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    stages: Query<(Entity, Ref<AudioStage>)>,
    mut removed_stages: RemovedComponents<AudioStage>,
    mut play_requests: MessageWriter<PlayAudioCue>,
    mut stop_requests: MessageWriter<StopAudioCue>,
) {
    for stage_owner in removed_stages.read() {
        stop_requests.write(StopAudioCue {
            emitter: Some(stage_owner),
            selection: stage_owner.to_bits(),
        });
    }
    let audio_assets_changed = audio_asset_index.is_changed();
    let Some(audio_asset_view) = audio_asset_index.get(&audio_assets) else {
        return;
    };
    for (stage_owner, selected_stage) in &stages {
        if !audio_assets_changed && !selected_stage.is_added() && !selected_stage.is_changed() {
            continue;
        }
        stop_requests.write(StopAudioCue {
            emitter: Some(stage_owner),
            selection: stage_owner.to_bits(),
        });
        let Some(stage) = audio_asset_view.stage(selected_stage.0) else {
            continue;
        };
        if let Some(loop_cue) = stage.loop_cue {
            play_requests.write(PlayAudioCue {
                cue: loop_cue,
                emitter: Some(stage_owner),
                selection: stage_owner.to_bits(),
                priority: 64,
                force_looped: true,
            });
        }
        for ambient_cue in &stage.ambient_cues {
            play_requests.write(PlayAudioCue {
                cue: *ambient_cue,
                emitter: Some(stage_owner),
                selection: stage_owner.to_bits(),
                priority: 32,
                force_looped: false,
            });
        }
    }
}
