use bevy::{
    audio::{PlaybackMode, PlaybackSettings, Volume},
    prelude::*,
};
use openzt2_game_data::audio::{AudioUsage, VARIANT_LOOPED};

use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};
use openzt2_interface_state::settings::audio_settings_types::AudioSettings;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use super::{
    audio_cue_resolution::resolve_weighted_audio_cue_variant,
    audio_playback_message_types::PlayAudioCue,
    audio_player_spawning::{spawn_audio_player_or_deferred_playback, AudioCueSelection},
    deterministic_audio_sampling::sample_deterministic_f32_range,
};

#[derive(Component)]
pub(super) struct PositionedAudioCueEmitter {
    admission_group: Option<openzt2_game_data::AssetId>,
}

pub(super) fn forward_positioned_audio_cue_playback_requests(
    view_state: Option<
        Res<openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterViewState>,
    >,
    mut commands: Commands,
    mut requests: MessageReader<
        openzt2_zoo_state::audio::audio_playback_message_types::PlayPositionedAudioCue,
    >,
    mut cues: MessageWriter<PlayAudioCue>,
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    emitters: Query<(
        Entity,
        &PositionedAudioCueEmitter,
        &Transform,
        Option<&Children>,
    )>,
    world_roots: Query<Entity, With<WorldRoot>>,
) {
    for (entity, _, _, children) in &emitters {
        if children.is_none_or(|children| children.is_empty()) {
            commands.entity(entity).despawn();
        }
    }
    if audio_asset_index.get(&audio_assets).is_none() {
        requests.clear();
        return;
    }
    let Some(view) = view_state else {
        requests.clear();
        return;
    };
    let Some(listener_is_underwater) = view.underwater_status() else {
        requests.clear();
        return;
    };
    let mut pending: Vec<
        Option<openzt2_zoo_state::audio::audio_playback_message_types::PlayPositionedAudioCue>,
    > = Vec::new();
    let mut admitted = Vec::new();
    if let Some(camera) = view.camera_position() {
        for (entity, metadata, transform, children) in &emitters {
            if children.is_some_and(|children| !children.is_empty()) {
                if let Some(group) = metadata.admission_group {
                    admitted.push((
                        group,
                        transform.translation.distance_squared(camera),
                        Some(entity),
                        None,
                    ));
                }
            }
        }
    }
    for request in requests.read() {
        if !(if listener_is_underwater {
            request.listener_below_water
        } else {
            request.listener_above_water
        }) {
            continue;
        }
        if let Some(policy) = request.admission {
            let Some(camera) = view.camera_position() else {
                continue;
            };
            let distance_squared = request.world_position.distance_squared(camera);
            let distances = admitted
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.0 == policy.group)
                .map(|(index, entry)| (index, entry.1))
                .collect::<Vec<_>>();
            let Some(replacement) = positioned_audio_admission(
                distance_squared,
                policy.maximum_distance_squared,
                policy.maximum_voices,
                &distances,
            ) else {
                continue;
            };
            if let Some(index) = replacement {
                let (_, _, entity, queued): (_, _, Option<Entity>, Option<usize>) =
                    admitted.swap_remove(index);
                if let Some(entity) = entity {
                    commands.entity(entity).despawn();
                }
                if let Some(queued) = queued {
                    pending[queued] = None;
                }
            }
            admitted.push((policy.group, distance_squared, None, Some(pending.len())));
        }
        pending.push(Some(*request));
    }
    for request in pending.into_iter().flatten() {
        let Ok(root) = world_roots.single() else {
            continue;
        };
        let emitter = commands
            .spawn((
                PositionedAudioCueEmitter {
                    admission_group: request.admission.map(|policy| policy.group),
                },
                WorldMember { root },
                Transform::from_translation(request.world_position),
                GlobalTransform::default(),
            ))
            .id();
        cues.write(PlayAudioCue {
            cue: request.cue,
            emitter: Some(emitter),
            selection: request.selection,
            priority: request.priority,
            force_looped: false,
        });
    }
}

fn positioned_audio_admission(
    distance_squared: f32,
    maximum_distance_squared: f32,
    maximum_voices: u32,
    existing: &[(usize, f32)],
) -> Option<Option<usize>> {
    if maximum_voices == 0 || !(distance_squared < maximum_distance_squared) {
        return None;
    }
    if existing.len() < maximum_voices as usize {
        return Some(None);
    }
    let &(index, farthest) = existing
        .iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))?;
    (distance_squared < farthest).then_some(Some(index))
}

#[cfg(test)]
mod positioned_audio_admission_tests {
    use super::positioned_audio_admission;

    #[test]
    fn deferred_voice_retains_its_anchor_until_the_voice_is_removed() {
        use crate::audio::audio_player_spawning::{
            spawn_audio_player_or_deferred_playback, AudioCueSelection, DeferredAudioPlayback,
        };
        use bevy::prelude::*;
        use openzt2_game_data::audio::AudioUsage;
        use openzt2_interface_state::settings::audio_settings_types::AudioSettings;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin));
        let position = Vec3::new(12.0, 3.0, -8.0);
        let anchor = app
            .world_mut()
            .spawn(Transform::from_translation(position))
            .id();
        let voice = spawn_audio_player_or_deferred_playback(
            &mut app.world_mut().commands(),
            Handle::default(),
            AudioCueSelection {
                emitter: Some(anchor),
                selection: 0,
                usage: AudioUsage::Effect,
                priority: 0,
            },
            bevy::audio::PlaybackSettings::default(),
            &AudioSettings::default(),
            2.0,
        );
        app.world_mut().flush();
        app.update();
        let world = app.world_mut();
        assert_eq!(
            world.get::<GlobalTransform>(voice).unwrap().translation(),
            position
        );
        assert!(world.get::<DeferredAudioPlayback>(voice).is_some());
        assert!(world
            .get::<Children>(anchor)
            .is_some_and(|children| children.contains(&voice)));
        world.despawn(voice);
        assert!(world
            .get::<Children>(anchor)
            .is_none_or(|children| children.is_empty()));
    }

    #[test]
    fn capped_voices_accept_only_a_closer_replacement() {
        let voices = [(0, 4.0), (1, 16.0)];
        assert_eq!(
            positioned_audio_admission(9.0, 100.0, 2, &voices),
            Some(Some(1))
        );
        assert_eq!(positioned_audio_admission(16.0, 100.0, 2, &voices), None);
        assert_eq!(positioned_audio_admission(25.0, 100.0, 2, &voices), None);
        assert_eq!(positioned_audio_admission(100.0, 100.0, 3, &voices), None);
    }

    #[test]
    fn a_queued_voice_occupies_capacity_before_playback() {
        let queued = [(0, 9.0)];
        assert_eq!(positioned_audio_admission(16.0, 100.0, 1, &queued), None);
        assert_eq!(
            positioned_audio_admission(4.0, 100.0, 1, &queued),
            Some(Some(0))
        );
    }
}

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
