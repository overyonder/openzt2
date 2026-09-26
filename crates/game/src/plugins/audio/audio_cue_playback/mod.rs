use bevy::{
    audio::{PlaybackMode, PlaybackSettings, Volume},
    prelude::*,
};
use openzt2_game_data::audio::{AudioUsage, VARIANT_LOOPED};

use crate::{
    assets::audio::audio_asset_types::{AudioAsset, AudioAssets},
    plugins::settings::audio_settings_types::AudioSettings,
};

use super::{
    audio_cue_resolution::resolve_weighted_audio_cue_variant,
    audio_playback_message_types::PlayAudioCue,
    audio_player_spawning::{spawn_audio_player_or_deferred_playback, AudioCueSelection},
    deterministic_audio_sampling::sample_deterministic_f32_range,
};

pub(super) fn execute_audio_cue_playback_requests(
    mut commands: Commands,
    mut requests: MessageReader<PlayAudioCue>,
    asset_server: Res<AssetServer>,
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    audio_settings: Res<AudioSettings>,
    playing_audio: Query<(Entity, &AudioCueSelection)>,
) {
    let Some(audio_asset_view) = audio_asset_index.get(&audio_assets) else {
        return;
    };
    let mut active_voice_pools: [Vec<(Entity, u8)>; 3] = std::array::from_fn(|_| Vec::new());
    for (playing_entity, active_selection) in &playing_audio {
        let voice_pool_index = match active_selection.usage {
            AudioUsage::Ambient => 0,
            _ if active_selection.emitter.is_some() => 1,
            _ => 2,
        };
        active_voice_pools[voice_pool_index].push((playing_entity, active_selection.priority));
    }
    for request in requests.read() {
        let Some((resolved_cue, variant, usage)) =
            resolve_weighted_audio_cue_variant(audio_asset_view, request.cue, request.selection, 0)
        else {
            continue;
        };
        let voice_limit = match usage {
            AudioUsage::Ambient => audio_asset_view
                .ambient()
                .map_or(10, |policy| policy.voice_limit),
            _ if request.emitter.is_some() => audio_asset_view
                .mixer()
                .map_or(8, |policy| policy.spatial_voice_limit),
            _ => audio_asset_view
                .mixer()
                .map_or(32, |policy| policy.voice_limit),
        } as usize;
        if voice_limit == 0 {
            continue;
        }
        let voice_pool_index = match usage {
            AudioUsage::Ambient => 0,
            _ if request.emitter.is_some() => 1,
            _ => 2,
        };
        let active_voices = &mut active_voice_pools[voice_pool_index];
        if active_voices.len() >= voice_limit {
            let Some((lowest_priority_index, &(_, lowest_priority))) = active_voices
                .iter()
                .enumerate()
                .min_by_key(|(_, voice)| voice.1)
            else {
                continue;
            };
            if lowest_priority >= request.priority {
                continue;
            }
            let (displaced_voice_entity, _) = active_voices.swap_remove(lowest_priority_index);
            commands.entity(displaced_voice_entity).despawn();
        }
        let Some(clip) = resolved_cue.clip(&asset_server, variant.source_id) else {
            continue;
        };
        let playback_settings = PlaybackSettings {
            mode: if request.force_looped || variant.flags & VARIANT_LOOPED != 0 {
                PlaybackMode::Loop
            } else {
                PlaybackMode::Despawn
            },
            volume: Volume::Linear(sample_deterministic_f32_range(
                variant.gain,
                request.selection,
            )),
            speed: sample_deterministic_f32_range(variant.pitch, request.selection.rotate_left(7)),
            spatial: request.emitter.is_some(),
            ..default()
        };
        let playback_entity = spawn_audio_player_or_deferred_playback(
            &mut commands,
            clip,
            AudioCueSelection {
                emitter: request.emitter,
                selection: request.selection,
                usage,
                priority: request.priority,
            },
            playback_settings,
            &audio_settings,
            sample_deterministic_f32_range(
                variant.delay_seconds,
                request.selection.rotate_left(17),
            ),
        );
        active_voices.push((playback_entity, request.priority));
    }
}
