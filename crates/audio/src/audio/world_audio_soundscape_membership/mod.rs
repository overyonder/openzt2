use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_assets::audio::audio_asset_types::AudioAsset;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::audio::audio_environment_types::{AudioSoundscape, WorldAudioPreset};

#[derive(Component)]
#[relationship(relationship_target = WorldAudioSoundscapes)]
pub(super) struct AudioSoundscapeOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = AudioSoundscapeOf)]
pub(super) struct WorldAudioSoundscapes(Vec<Entity>);

pub(super) fn bind_world_soundscape_membership(
    mut commands: Commands,
    soundscapes: Query<
        (Entity, &WorldMember, Option<&AudioSoundscapeOf>),
        (
            With<AudioSoundscape>,
            Or<(Added<AudioSoundscape>, Changed<WorldMember>)>,
        ),
    >,
    existing: Query<Entity, With<AudioSoundscapeOf>>,
    active: Query<(), (With<AudioSoundscape>, With<WorldMember>)>,
    mut removed_soundscapes: RemovedComponents<AudioSoundscape>,
    mut removed_members: RemovedComponents<WorldMember>,
) {
    for (entity, member, relationship) in &soundscapes {
        if relationship.is_none_or(|relationship| relationship.0 != member.root) {
            commands
                .entity(entity)
                .insert(AudioSoundscapeOf(member.root));
        }
    }
    for entity in removed_soundscapes.read().chain(removed_members.read()) {
        if existing.contains(entity) && !active.contains(entity) {
            commands.entity(entity).try_remove::<AudioSoundscapeOf>();
        }
    }
}

#[derive(SystemParam)]
pub(super) struct SoundscapeInputChanges<'w, 's> {
    pub(super) world_soundscapes: Query<'w, 's, &'static WorldAudioSoundscapes>,
    changed_soundscapes: Query<
        'w,
        's,
        Entity,
        (
            With<AudioSoundscape>,
            Or<(Changed<AudioSoundscape>, Changed<WorldMember>)>,
        ),
    >,
    presets: Query<'w, 's, (Entity, Ref<'static, WorldAudioPreset>)>,
    removed_presets: RemovedComponents<'w, 's, WorldAudioPreset>,
    asset_events: MessageReader<'w, 's, AssetEvent<AudioAsset>>,
}

impl SoundscapeInputChanges<'_, '_> {
    pub(super) fn affected_soundscapes(&mut self) -> EntityHashSet {
        let changed_assets = self
            .asset_events
            .read()
            .filter_map(|event| match *event {
                AssetEvent::Added { id }
                | AssetEvent::Modified { id }
                | AssetEvent::Removed { id } => Some(id),
                _ => None,
            })
            .collect::<bevy::platform::collections::HashSet<_>>();
        let changed_worlds = self
            .presets
            .iter()
            .filter_map(|(entity, preset)| {
                (preset.is_changed() || changed_assets.contains(&preset.0.id())).then_some(entity)
            })
            .chain(self.removed_presets.read());
        let mut candidates = self.changed_soundscapes.iter().collect::<EntityHashSet>();
        candidates.extend(
            changed_worlds
                .filter_map(|root| self.world_soundscapes.get(root).ok())
                .flat_map(WorldAudioSoundscapes::iter),
        );
        candidates
    }
}
