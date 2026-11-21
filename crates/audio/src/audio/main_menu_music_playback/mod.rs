use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    audio::audio_asset_types::{AudioAsset, AudioAssets},
};

use super::{
    audio_playback_message_types::PlayAudioCue,
    audio_request_lanes::{ProducedAudioRequest, MENU_MUSIC},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct MenuMusicState;

impl ComputedStates for MenuMusicState {
    type SourceStates = GamePhase;

    fn compute(phase: GamePhase) -> Option<Self> {
        matches!(phase, GamePhase::MainMenu | GamePhase::MapSelection).then_some(Self)
    }
}

#[derive(Component)]
pub(super) struct MenuMusicVisit;

#[derive(Component)]
pub(super) struct MenuMusicRequested;

pub(super) fn begin_menu_music_visit(mut commands: Commands) {
    commands.spawn((MenuMusicVisit, DespawnOnExit(MenuMusicState)));
}

pub(super) fn play_main_menu_music_when_audio_is_ready(
    mut commands: Commands,
    visits: Query<Entity, (With<MenuMusicVisit>, Without<MenuMusicRequested>)>,
    assets: Res<Assets<AudioAsset>>,
    index: Res<AudioAssets>,
    mut requests: MessageWriter<ProducedAudioRequest<MENU_MUSIC>>,
) {
    let Ok(owner) = visits.single() else {
        return;
    };
    let cue = AssetId::from_key("audio-cue:mainmenu_music");
    if index.get(&assets).and_then(|view| view.cue(cue)).is_none() {
        return;
    }
    let selection = u64::from_le_bytes(cue.0[..8].try_into().expect("asset identifier prefix"));
    requests.write(ProducedAudioRequest::PlayOwned(
        PlayAudioCue {
            cue,
            emitter: None,
            selection,
            priority: u8::MAX,
            force_looped: false,
        },
        owner,
    ));
    commands.entity(owner).insert(MenuMusicRequested);
}
