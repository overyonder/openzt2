use bevy::ecs::relationship::RelationshipTarget;
use bevy::prelude::*;

use super::{
    audio_playback_message_types::StopAudioCue,
    audio_player_spawning::{AudioCueSelection, NonspatialCueOwner, NonspatialCueVoices},
};

pub(super) fn execute_audio_stop_requests(
    mut commands: Commands,
    mut requests: MessageReader<StopAudioCue>,
    emitter_voices: Query<&Children>,
    nonspatial_voices: Query<&NonspatialCueVoices, With<NonspatialCueOwner>>,
    playing: Query<&AudioCueSelection>,
) {
    for request in requests.read() {
        let voices = match request.emitter {
            Some(emitter) => {
                let Ok(children) = emitter_voices.get(emitter) else {
                    continue;
                };
                children.collection().as_slice()
            }
            None => {
                let Ok(voices) = nonspatial_voices.single() else {
                    continue;
                };
                voices.collection().as_slice()
            }
        };
        for &voice in voices {
            if playing.get(voice).is_ok_and(|selection| {
                selection.emitter == request.emitter && selection.selection == request.selection
            }) {
                commands.entity(voice).try_despawn();
            }
        }
    }
}
