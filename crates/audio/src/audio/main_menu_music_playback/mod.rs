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
pub(super) struct PendingMenuMusic;

pub(super) fn begin_menu_music_visit(
    mut commands: Commands,
    assets: Res<Assets<AudioAsset>>,
    index: Res<AudioAssets>,
    mut requests: MessageWriter<ProducedAudioRequest<MENU_MUSIC>>,
) {
    let mut owner = commands.spawn(DespawnOnExit(MenuMusicState));
    if let Some(request) = resolve_menu_music_request(&assets, &index) {
        requests.write(ProducedAudioRequest::PlayOwned(request, owner.id()));
    } else {
        owner.insert(PendingMenuMusic);
    }
}

pub(super) fn admit_pending_menu_music(
    mut commands: Commands,
    owner: Single<Entity, With<PendingMenuMusic>>,
    assets: Res<Assets<AudioAsset>>,
    index: Res<AudioAssets>,
    mut requests: MessageWriter<ProducedAudioRequest<MENU_MUSIC>>,
) {
    if let Some(request) = resolve_menu_music_request(&assets, &index) {
        requests.write(ProducedAudioRequest::PlayOwned(request, *owner));
        commands.entity(*owner).remove::<PendingMenuMusic>();
    }
}

fn resolve_menu_music_request(
    assets: &Assets<AudioAsset>,
    index: &AudioAssets,
) -> Option<PlayAudioCue> {
    let cue = AssetId::from_key("audio-cue:mainmenu_music");
    index.get(assets)?.cue(cue)?;
    let selection = u64::from_le_bytes(cue.0[..8].try_into().expect("asset identifier prefix"));
    Some(PlayAudioCue {
        cue,
        emitter: None,
        selection,
        priority: u8::MAX,
        force_looped: false,
    })
}
