use bevy::ecs::relationship::RelationshipTarget;
use bevy::platform::collections::HashMap;
use bevy::{
    audio::{PlaybackMode, PlaybackSettings, Volume},
    prelude::*,
};
use openzt2_game_data::audio::VARIANT_LOOPED;
use openzt2_game_data::AssetId;

use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};
use openzt2_interface_state::settings::audio_settings_types::AudioSettings;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use super::{
    audio_cue_resolution::resolve_weighted_audio_cue_variant,
    audio_playback_message_types::{PlayAudioCue, PlayPositionedAudioCue},
    audio_player_spawning::{
        spawn_audio_player_or_deferred_playback, AudioCueSelection, AudioVoiceAdmission,
        CueVoicePool, CueVoices, NonspatialCueOwner,
    },
    audio_request_lanes::AdmitAudioCue,
    audio_stage_playback_projection::EnvironmentVoiceGain,
    deterministic_audio_sampling::sample_deterministic_f32_range,
};

#[cfg(test)]
mod positioned_audio_admission_tests {
    use super::positioned_audio_admission;

    #[test]
    fn deferred_voice_retains_its_anchor_until_the_voice_is_removed() {
        use crate::audio::audio_player_spawning::{
            spawn_audio_player_or_deferred_playback, AudioCueSelection, AudioVoiceAdmission,
            DeferredAudioPlayback,
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
            AudioVoiceAdmission::default(),
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
            positioned_audio_admission(9.0, 100.0, 2, voices.iter().copied()),
            Some(Some(1))
        );
        assert_eq!(
            positioned_audio_admission(16.0, 100.0, 2, voices.iter().copied()),
            None
        );
        assert_eq!(
            positioned_audio_admission(25.0, 100.0, 2, voices.iter().copied()),
            None
        );
        assert_eq!(
            positioned_audio_admission(100.0, 100.0, 3, voices.iter().copied()),
            None
        );
    }

    #[test]
    fn a_queued_voice_occupies_capacity_before_playback() {
        let queued = [(0, 9.0)];
        assert_eq!(
            positioned_audio_admission(16.0, 100.0, 1, queued.iter().copied()),
            None
        );
        assert_eq!(
            positioned_audio_admission(4.0, 100.0, 1, queued.iter().copied()),
            Some(Some(0))
        );
    }
}

#[derive(Component)]
pub(super) struct PositionedAudioCueEmitter;

#[derive(Component, Clone, Copy)]
pub(super) struct PositionedListenerWater {
    pub(super) above: bool,
    pub(super) below: bool,
}

impl PositionedListenerWater {
    pub(super) fn allows(self, underwater: Option<bool>) -> bool {
        match underwater {
            Some(true) => self.below,
            Some(false) => self.above,
            None => self.above && self.below,
        }
    }
}

#[derive(Component)]
pub(super) struct PositionedAudioGroup(AssetId);

#[derive(Component)]
#[relationship(relationship_target = PositionedGroupEmitters)]
pub(super) struct PositionedEmitterOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = PositionedEmitterOf)]
pub(super) struct PositionedGroupEmitters(Vec<Entity>);

#[derive(Clone, Copy)]
enum PositionedVoice {
    Active(Entity),
    Admitted(usize),
}

pub(super) fn retire_empty_positioned_audio_emitters(
    mut commands: Commands,
    created: Query<Entity, Added<PositionedAudioCueEmitter>>,
    mut children_removed: RemovedComponents<Children>,
    empty: Query<(), (With<PositionedAudioCueEmitter>, Without<Children>)>,
) {
    for entity in created.iter().chain(children_removed.read()) {
        if empty.contains(entity) {
            commands.entity(entity).try_despawn();
        }
    }
}

pub(super) fn retire_empty_positioned_audio_groups(
    mut commands: Commands,
    mut emitters_removed: RemovedComponents<PositionedGroupEmitters>,
    empty: Query<(), (With<PositionedAudioGroup>, Without<PositionedGroupEmitters>)>,
) {
    for entity in emitters_removed.read() {
        if empty.contains(entity) {
            commands.entity(entity).try_despawn();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn forward_positioned_audio_cue_playback_requests(
    listener: super::spatial_cue_distance::SpatialCueListener,
    mut commands: Commands,
    mut requests: MessageReader<PlayPositionedAudioCue>,
    mut cues: MessageWriter<AdmitAudioCue>,
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    emitters: Query<(Entity, &Transform, &Children), With<PositionedAudioCueEmitter>>,
    groups: Query<(
        Entity,
        &PositionedAudioGroup,
        &WorldMember,
        Option<&PositionedGroupEmitters>,
    )>,
    world_roots: Query<Entity, With<WorldRoot>>,
) {
    let Ok(root) = world_roots.single() else {
        requests.clear();
        return;
    };
    if audio_asset_index.get(&audio_assets).is_none() {
        requests.clear();
        return;
    }
    let (listener, _) = listener.snapshot();
    let underwater = listener.underwater;
    let camera = listener.camera_position;
    let mut active_groups = HashMap::new();
    for (entity, group, member, emitters) in &groups {
        if member.root == root {
            active_groups.insert(group.0, (entity, emitters));
        }
    }
    let mut admissions: HashMap<AssetId, (Entity, Vec<(PositionedVoice, f32)>)> = HashMap::new();
    let mut pending: Vec<
        Option<(
            PlayPositionedAudioCue,
            PositionedListenerWater,
            Option<Entity>,
        )>,
    > = Vec::new();
    for &request in requests.read() {
        let water = PositionedListenerWater {
            above: request.listener_above_water,
            below: request.listener_below_water,
        };
        if !water.allows(underwater) {
            continue;
        }
        let group = if let Some(policy) = request.admission {
            let Some(camera) = camera else { continue };
            let distance = request.world_position.distance_squared(camera);
            if policy.maximum_voices == 0
                || distance.partial_cmp(&policy.maximum_distance_squared)
                    != Some(std::cmp::Ordering::Less)
            {
                continue;
            }
            let (group, voices) = admissions.entry(policy.group).or_insert_with(|| {
                if let Some((group, members)) = active_groups.get(&policy.group) {
                    let voices = members
                        .iter()
                        .flat_map(|target| target.iter())
                        .filter_map(|entity| emitters.get(entity).ok())
                        .map(|(entity, pose, _)| {
                            (
                                PositionedVoice::Active(entity),
                                pose.translation.distance_squared(camera),
                            )
                        })
                        .collect();
                    (*group, voices)
                } else {
                    let group = commands
                        .spawn((PositionedAudioGroup(policy.group), WorldMember { root }))
                        .id();
                    (group, Vec::new())
                }
            });
            let Some(replacement) = positioned_audio_admission(
                distance,
                policy.maximum_distance_squared,
                policy.maximum_voices,
                voices
                    .iter()
                    .enumerate()
                    .map(|(index, voice)| (index, voice.1)),
            ) else {
                continue;
            };
            if let Some(index) = replacement {
                match voices.swap_remove(index).0 {
                    PositionedVoice::Active(entity) => {
                        commands.entity(entity).try_despawn();
                    }
                    PositionedVoice::Admitted(index) => pending[index] = None,
                }
            }
            voices.push((PositionedVoice::Admitted(pending.len()), distance));
            Some(*group)
        } else {
            None
        };
        pending.push(Some((request, water, group)));
    }
    for (request, water, group) in pending.into_iter().flatten() {
        let pose = Transform::from_translation(request.world_position);
        let mut emitter = commands.spawn((
            PositionedAudioCueEmitter,
            water,
            WorldMember { root },
            pose,
            GlobalTransform::from(pose),
        ));
        if let Some(group) = group {
            emitter.insert(PositionedEmitterOf(group));
        }
        cues.write(AdmitAudioCue {
            cue: PlayAudioCue {
                cue: request.cue,
                emitter: Some(emitter.id()),
                selection: request.selection,
                priority: request.priority,
                force_looped: request.force_looped,
            },
            lifetime_owner: None,
        });
    }
}

fn positioned_audio_admission(
    distance_squared: f32,
    maximum_distance_squared: f32,
    maximum_voices: u32,
    existing: impl ExactSizeIterator<Item = (usize, f32)>,
) -> Option<Option<usize>> {
    if maximum_voices == 0
        || distance_squared.partial_cmp(&maximum_distance_squared) != Some(std::cmp::Ordering::Less)
    {
        return None;
    }
    if existing.len() < maximum_voices as usize {
        return Some(None);
    }
    let (index, farthest) = existing.max_by(|left, right| left.1.total_cmp(&right.1))?;
    (distance_squared < farthest).then_some(Some(index))
}

use openzt2_animals::animal_health::RampageAudioMix;

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_audio_cue_playback_requests(
    mut commands: Commands,
    mut requests: MessageReader<AdmitAudioCue>,
    asset_server: Res<AssetServer>,
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    audio_settings: Res<AudioSettings>,
    pools: Query<(Entity, &CueVoicePool, Option<&CueVoices>)>,
    owners: Query<Entity, With<NonspatialCueOwner>>,
    environment_gains: Query<&EnvironmentVoiceGain>,
    soundscapes: Query<(), With<super::audio_environment_types::AudioSoundscape>>,
    playing: Query<&AudioCueSelection>,
    listener: super::spatial_cue_distance::SpatialCueListener,
    world_members: Query<&WorldMember>,
    worlds: Query<Entity, With<WorldRoot>>,
    rampages: Query<&RampageAudioMix>,
) {
    let Some(view) = audio_asset_index.get(&audio_assets) else {
        requests.clear();
        return;
    };
    let nonspatial_owner = owners.single().ok();
    let (listener_pose, _) = listener.snapshot();
    let mut pool_entities = [None; 3];
    for (entity, pool, members) in &pools {
        pool_entities[pool.index()] = Some((entity, members));
    }
    let mut admissions: [Option<Vec<(Entity, u8)>>; 3] = Default::default();
    for request in requests.read() {
        let cue = &request.cue;
        let Some((resolved, variant, usage)) =
            resolve_weighted_audio_cue_variant(view, cue.cue, cue.selection, 0)
        else {
            debug!(cue = ?cue.cue, "audio cue has no playable variant");
            continue;
        };
        let spatial = cue.emitter.is_some();
        let looped = cue.force_looped || variant.flags & VARIANT_LOOPED != 0;
        let emitter_pose = cue.emitter.and_then(|entity| listener.emitter(entity));
        let mut distance = spatial.then(|| {
            super::spatial_cue_distance::SpatialCueDistance::from_authored_range(
                variant.distance,
                emitter_pose.is_none_or(|(_, water)| water.is_none()),
                looped,
            )
        });
        let distance_gain = distance.as_mut().map(|policy| {
            emitter_pose.map_or(0.0, |(position, water)| {
                listener_pose.gain(policy, position, water, view.mixer())
            })
        });
        if !looped && distance_gain == Some(0.0) {
            continue;
        }
        let pool = CueVoicePool::for_cue(usage, spatial);
        let voice_limit = match pool {
            CueVoicePool::Ambient => view.ambient().map_or(10, |policy| policy.voice_limit),
            CueVoicePool::Spatial => view.mixer().map_or(8, |policy| policy.spatial_voice_limit),
            CueVoicePool::Nonspatial => view.mixer().map_or(32, |policy| policy.voice_limit),
        } as usize;
        let Some((pool_entity, members)) = pool_entities[pool.index()].filter(|_| voice_limit != 0)
        else {
            continue;
        };
        let voices = admissions[pool.index()].get_or_insert_with(|| {
            members
                .into_iter()
                .flat_map(|members| members.iter())
                .filter_map(|entity| {
                    playing
                        .get(entity)
                        .ok()
                        .map(|voice| (entity, voice.priority))
                })
                .collect()
        });
        if voices.len() >= voice_limit {
            let Some((lowest_index, &(_, lowest_priority))) =
                voices.iter().enumerate().min_by_key(|(_, voice)| voice.1)
            else {
                continue;
            };
            if lowest_priority >= cue.priority {
                continue;
            }
            let (displaced, _) = voices.swap_remove(lowest_index);
            commands.entity(displaced).try_despawn();
        }
        let Some(clip) = resolved.clip(&asset_server, variant.source_id) else {
            continue;
        };
        let settings = PlaybackSettings {
            mode: if looped {
                PlaybackMode::Loop
            } else {
                PlaybackMode::Despawn
            },
            volume: Volume::Linear(sample_deterministic_f32_range(variant.gain, cue.selection)),
            speed: sample_deterministic_f32_range(variant.pitch, cue.selection.rotate_left(7)),
            spatial,
            spatial_scale: distance.map(|distance| distance.panning_scale(view.mixer())),
            ..default()
        };
        let world = (usage != openzt2_game_data::audio::AudioUsage::Ui)
            .then(|| {
                cue.emitter
                    .or(request.lifetime_owner)
                    .and_then(|owner| world_members.get(owner).ok().map(|member| member.root))
                    .or_else(|| {
                        request
                            .lifetime_owner
                            .filter(|&owner| worlds.contains(owner))
                    })
                    .or_else(|| worlds.single().ok())
            })
            .flatten();
        let voice = spawn_audio_player_or_deferred_playback(
            &mut commands,
            clip,
            AudioCueSelection {
                emitter: cue.emitter,
                selection: cue.selection,
                usage,
                priority: cue.priority,
            },
            settings,
            &audio_settings,
            sample_deterministic_f32_range(variant.delay_seconds, cue.selection.rotate_left(17)),
            AudioVoiceAdmission {
                pool: Some(pool_entity),
                nonspatial_owner,
                lifetime_owner: request.lifetime_owner,
                distance,
                distance_gain,
                world,
                rampage_gain: world.map(|root| {
                    super::audio_player_spawning::rampage_voice_gain(
                        rampages.get(root).ok(),
                        cue.selection,
                    )
                }),
                stage_acoustics: (!matches!(
                    usage,
                    openzt2_game_data::audio::AudioUsage::Ui
                        | openzt2_game_data::audio::AudioUsage::Music
                ))
                .then(|| {
                    super::stage_voice_acoustics::StageVoiceAcoustics::new(
                        &variant.filter_layer,
                        variant.soften_gain,
                        cue.emitter,
                        cue.emitter
                            .is_some_and(|emitter| soundscapes.contains(emitter)),
                    )
                }),
                environment_gain: request
                    .lifetime_owner
                    .and_then(|owner| environment_gains.get(owner).ok())
                    .map(|gain| gain.0),
            },
        );
        voices.push((voice, cue.priority));
    }
}
