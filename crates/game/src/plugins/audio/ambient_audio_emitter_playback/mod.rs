use bevy::prelude::*;

use crate::{
    assets::audio::audio_asset_types::{AudioAsset, AudioAssets},
    plugins::world_spawn::world_membership_types::WorldMember,
};

use super::{
    audio_environment_types::{AmbientAudioEmitter, AudioSoundscape},
    audio_playback_message_types::PlayAudioCue,
};

pub(super) fn play_changed_allowed_ambient_audio_emitters(
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    emitters: Query<(Entity, Ref<AmbientAudioEmitter>, &WorldMember)>,
    soundscapes: Query<(&AudioSoundscape, &WorldMember)>,
    mut play_requests: MessageWriter<PlayAudioCue>,
) {
    let audio_assets_changed = audio_asset_index.is_changed();
    let Some(audio_asset_view) = audio_asset_index.get(&audio_assets) else {
        return;
    };
    for (emitter_entity, emitter, emitter_world) in &emitters {
        if !audio_assets_changed && !emitter.is_added() && !emitter.is_changed() {
            continue;
        }
        let selected_soundscape = soundscapes
            .iter()
            .find_map(|(soundscape, soundscape_world)| {
                (soundscape_world.root == emitter_world.root).then_some(soundscape)
            });
        let Some(allowed_entities) = selected_soundscape
            .and_then(|soundscape| audio_asset_view.soundscape_allowed_entities(soundscape.biome))
        else {
            continue;
        };
        if allowed_entities
            .allowed_entities
            .contains(&emitter.definition)
        {
            play_requests.write(PlayAudioCue {
                cue: emitter.cue,
                emitter: Some(emitter_entity),
                selection: emitter.selection,
                priority: 16,
                force_looped: false,
            });
        }
    }
}
