use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::{
    application_lifecycle::GamePhase,
    assets::audio::audio_asset_types::{AudioAsset, AudioAssets},
};

use super::audio_playback_message_types::{PlayAudioCue, StopAudioCue};

pub(super) fn play_main_menu_music_when_audio_is_ready(
    phase: Res<State<GamePhase>>,
    assets: Res<Assets<AudioAsset>>,
    index: Res<AudioAssets>,
    mut requested: Local<bool>,
    mut play: MessageWriter<PlayAudioCue>,
    mut stop: MessageWriter<StopAudioCue>,
) {
    let cue = AssetId::from_key("audio-cue:mainmenu_music");
    let selection = u64::from_le_bytes(cue.0[..8].try_into().expect("asset identifier prefix"));
    let in_menu = matches!(phase.get(), GamePhase::MainMenu | GamePhase::MapSelection);
    if !in_menu {
        if *requested {
            stop.write(StopAudioCue {
                emitter: None,
                selection,
            });
            *requested = false;
        }
        return;
    }
    if *requested || index.get(&assets).and_then(|view| view.cue(cue)).is_none() {
        return;
    }
    play.write(PlayAudioCue {
        cue,
        emitter: None,
        selection,
        priority: u8::MAX,
        force_looped: false,
    });
    *requested = true;
}
