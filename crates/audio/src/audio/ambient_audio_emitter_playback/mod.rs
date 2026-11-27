use super::audio_request_lanes::{ProducedAudioRequest, AMBIENT_EMITTER};
use bevy::{ecs::system::SystemParam, prelude::*};

use super::world_audio_soundscape_membership::WorldAudioSoundscapes;
use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::relationship::RelationshipTarget;
use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldMembers};

use super::{
    audio_environment_types::{AmbientAudioEmitter, AudioSoundscape, WorldAudioPreset},
    audio_playback_message_types::{PlayAudioCue, StopAudioCue},
};

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) struct AmbientEmitterPlayback {
    cue: AssetId,
    selection: u64,
    world: Entity,
}

#[derive(SystemParam)]
pub(super) struct AmbientAudioWorldChanges<'w, 's> {
    soundscapes: Query<'w, 's, &'static WorldMember, Changed<AudioSoundscape>>,
    presets: Query<'w, 's, Entity, Changed<WorldAudioPreset>>,
    removed_presets: RemovedComponents<'w, 's, WorldAudioPreset>,
    membership: Query<'w, 's, Entity, Changed<WorldAudioSoundscapes>>,
    removed_soundscapes: RemovedComponents<'w, 's, WorldAudioSoundscapes>,
}

pub(super) fn play_changed_allowed_ambient_audio_emitters(
    mut commands: Commands,
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    mut audio_documents: MessageReader<AssetEvent<AudioAsset>>,
    changed_emitters: Query<
        Entity,
        (
            With<AmbientAudioEmitter>,
            Or<(Changed<AmbientAudioEmitter>, Changed<WorldMember>)>,
        ),
    >,
    emitters: Query<(Entity, &AmbientAudioEmitter, &WorldMember)>,
    mut world_changes: AmbientAudioWorldChanges,
    soundscapes: Query<&AudioSoundscape>,
    worlds: Query<&WorldMembers>,
    world_soundscapes: Query<&WorldAudioSoundscapes>,
    presets: Query<&WorldAudioPreset>,
    mut removed_emitters: RemovedComponents<AmbientAudioEmitter>,
    mut removed_members: RemovedComponents<WorldMember>,
    mut active_emitters: Query<&mut AmbientEmitterPlayback>,
    mut requests: MessageWriter<ProducedAudioRequest<AMBIENT_EMITTER>>,
) {
    for entity in removed_emitters.read().chain(removed_members.read()) {
        if emitters.contains(entity) {
            continue;
        }
        if let Ok(previous) = active_emitters.get(entity) {
            requests.write(ProducedAudioRequest::Stop(StopAudioCue {
                emitter: Some(entity),
                selection: previous.selection,
            }));
            commands
                .entity(entity)
                .try_remove::<AmbientEmitterPlayback>();
        }
    }
    let changed_worlds = world_changes
        .soundscapes
        .iter()
        .map(|member| member.root)
        .chain(world_changes.membership.iter())
        .chain(world_changes.presets.iter())
        .chain(world_changes.removed_presets.read())
        .chain(world_changes.removed_soundscapes.read())
        .collect::<EntityHashSet>();
    let view = audio_asset_index.get(&audio_assets);
    let documents_changed = audio_documents.read().fold(false, |changed, event| {
        changed
            || matches!(
                event,
                AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
            )
    });
    let mut candidates = changed_emitters.iter().collect::<EntityHashSet>();
    if audio_asset_index.is_changed() || documents_changed {
        // A winning biome document can replace the policy of every loaded world.
        candidates.extend(emitters.iter().map(|(entity, ..)| entity));
    }
    candidates.extend(
        changed_worlds
            .into_iter()
            .filter_map(|root| worlds.get(root).ok())
            .flat_map(WorldMembers::iter),
    );
    for entity in candidates {
        let Ok((_, emitter, member)) = emitters.get(entity) else {
            continue;
        };
        let previous = active_emitters.get_mut(entity).ok();
        let eligible = world_soundscapes
            .get(member.root)
            .ok()
            .and_then(|instances| {
                instances
                    .iter()
                    .find_map(|entity| soundscapes.get(entity).ok())
            })
            .and_then(|soundscape| {
                presets
                    .get(member.root)
                    .ok()
                    .and_then(|preset| audio_assets.get(&preset.0))
                    .and_then(|preset| {
                        preset
                            .document()
                            .soundscape_allowed_entities
                            .iter()
                            .find(|allowed| allowed.biome == soundscape.biome)
                    })
                    .or_else(|| view?.soundscape_allowed_entities(soundscape.biome))
            })
            .is_some_and(|allowed| {
                allowed.allowed_entities.contains(&emitter.definition)
                    && view.is_some_and(|view| view.cue(emitter.cue).is_some())
            });
        let next = eligible.then_some(AmbientEmitterPlayback {
            cue: emitter.cue,
            selection: emitter.selection,
            world: member.root,
        });
        if next.as_ref() == previous.as_deref() {
            continue;
        }
        if let Some(previous) = previous.as_deref() {
            requests.write(ProducedAudioRequest::Stop(StopAudioCue {
                emitter: Some(entity),
                selection: previous.selection,
            }));
        }
        if let Some(next) = next {
            requests.write(ProducedAudioRequest::PlayOwned(
                PlayAudioCue {
                    cue: next.cue,
                    emitter: Some(entity),
                    selection: next.selection,
                    priority: 16,
                    force_looped: false,
                },
                next.world,
            ));
            if let Some(mut previous) = previous {
                previous.set_if_neq(next);
            } else {
                commands.entity(entity).insert(next);
            }
        } else {
            commands.entity(entity).remove::<AmbientEmitterPlayback>();
        }
    }
}
