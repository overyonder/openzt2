use bevy::prelude::*;

use super::{audio_playback_message_types::StopAudioCue, audio_player_spawning::AudioCueSelection};

pub(super) fn execute_audio_stop_requests(
    mut commands: Commands,
    mut requests: MessageReader<StopAudioCue>,
    playing_audio: Query<(Entity, &AudioCueSelection)>,
) {
    for request in requests.read() {
        for (playing_entity, selection) in &playing_audio {
            if selection.emitter == request.emitter && selection.selection == request.selection {
                commands.entity(playing_entity).despawn();
            }
        }
    }
}
