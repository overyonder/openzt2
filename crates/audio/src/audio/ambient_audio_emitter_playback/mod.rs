use super::audio_request_lanes::{ProducedAudioRequest, AMBIENT_EMITTER};
use bevy::prelude::*;

use super::world_audio_soundscape_membership::WorldAudioSoundscapes;
use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::relationship::RelationshipTarget;
use openzt2_game_assets::audio::audio_asset_types::{AudioAsset, AudioAssets};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldMembers};

use super::{
    audio_environment_types::{AmbientAudioEmitter, AudioSoundscape},
    audio_playback_message_types::{PlayAudioCue, StopAudioCue},
};

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) struct AmbientEmitterPlayback {
    cue: AssetId,
    selection: u64,
}

pub(super) fn play_changed_allowed_ambient_audio_emitters(
    mut commands: Commands,
    audio_assets: Res<Assets<AudioAsset>>,
    audio_asset_index: Res<AudioAssets>,
    changed_emitters: Query<
        Entity,
        (
            With<AmbientAudioEmitter>,
            Or<(Changed<AmbientAudioEmitter>, Changed<WorldMember>)>,
        ),
    >,
    emitters: Query<(Entity, &AmbientAudioEmitter, &WorldMember)>,
    changed_soundscapes: Query<&WorldMember, Changed<AudioSoundscape>>,
    soundscapes: Query<&AudioSoundscape>,
    worlds: Query<&WorldMembers>,
    world_soundscapes: Query<&WorldAudioSoundscapes>,
    changed_membership: Query<Entity, Changed<WorldAudioSoundscapes>>,
    mut removed_soundscapes: RemovedComponents<WorldAudioSoundscapes>,
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
    let changed_worlds = changed_soundscapes
        .iter()
        .map(|member| member.root)
        .chain(changed_membership.iter())
        .chain(removed_soundscapes.read())
        .collect::<EntityHashSet>();
    let Some(view) = audio_asset_index.get(&audio_assets) else {
        return;
    };
    let mut candidates = changed_emitters.iter().collect::<EntityHashSet>();
    if audio_asset_index.is_changed() {
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
            .and_then(|soundscape| view.soundscape_allowed_entities(soundscape.biome))
            .is_some_and(|allowed| allowed.allowed_entities.contains(&emitter.definition));
        let next = eligible.then_some(AmbientEmitterPlayback {
            cue: emitter.cue,
            selection: emitter.selection,
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
            requests.write(ProducedAudioRequest::Play(PlayAudioCue {
                cue: next.cue,
                emitter: Some(entity),
                selection: next.selection,
                priority: 16,
                force_looped: false,
            }));
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
