use bevy::asset::{asset_changed::AssetChanged, AsAssetId, AssetLoadFailedEvent};
use bevy::prelude::*;

use openzt2_game_assets::effect::ParticleEffectDocumentAsset;
use openzt2_game_assets::effect::SpawnCompleteParticleEffectDocument;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

/// One prefab-authored request to instantiate every emitter in an effect.
#[derive(Component, Debug, Clone)]
pub(super) struct PrefabEffectTrigger {
    handle: Handle<ParticleEffectDocumentAsset>,
}

impl AsAssetId for PrefabEffectTrigger {
    type Asset = ParticleEffectDocumentAsset;

    fn as_asset_id(&self) -> bevy::asset::AssetId<Self::Asset> {
        self.handle.id()
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PrefabEffectHydrated;

pub(super) fn spawn_prefab_effect_triggers(
    commands: &mut Commands,
    prefab: &ScenePrefabAsset,
    prefab_entities: &[Entity],
    world_root: Entity,
) {
    for (entity, row) in prefab
        .canonical_scene_prefab_document()
        .entities
        .iter()
        .enumerate()
    {
        let source = prefab_entities[entity];
        for effect in &row.effects {
            let handle = prefab
                .loaded_particle_effect_asset_handle(effect.effect)
                .expect("validated prefab effect dependency was not retained");
            commands.spawn((
                PrefabEffectTrigger {
                    handle: handle.clone(),
                },
                WorldMember { root: world_root },
                Transform::IDENTITY,
                Visibility::Inherited,
                ChildOf(source),
            ));
        }
    }
}

pub(super) fn hydrate_prefab_effect_triggers(
    mut commands: Commands,
    triggers: Query<
        (Entity, Ref<PrefabEffectTrigger>, &WorldMember),
        Without<PrefabEffectHydrated>,
    >,
    effects: Res<Assets<ParticleEffectDocumentAsset>>,
    mut asset_events: MessageReader<AssetEvent<ParticleEffectDocumentAsset>>,
    mut failures: MessageReader<AssetLoadFailedEvent<ParticleEffectDocumentAsset>>,
    changed: Query<
        Entity,
        (
            Without<PrefabEffectHydrated>,
            AssetChanged<PrefabEffectTrigger>,
        ),
    >,
    server: Res<AssetServer>,
    mut spawn: MessageWriter<SpawnCompleteParticleEffectDocument>,
) {
    let ready_assets = asset_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .collect::<bevy::platform::collections::HashSet<_>>();
    let failed = failures
        .read()
        .map(|event| event.id)
        .collect::<bevy::platform::collections::HashSet<_>>();
    for (entity, trigger, _) in &triggers {
        if !trigger.is_changed()
            && !ready_assets.contains(&trigger.handle.id())
            && !failed.contains(&trigger.handle.id())
            && !changed.contains(entity)
        {
            continue;
        }
        if effects.get(&trigger.handle).is_none() {
            if failed.contains(&trigger.handle.id())
                || trigger.is_added()
                    && matches!(
                        server.load_state(trigger.handle.id()),
                        bevy::asset::LoadState::Failed(_)
                    )
            {
                warn!(?entity, asset = ?trigger.handle.id(), "prefab effect could not be loaded");
                commands.entity(entity).try_despawn();
            }
            continue;
        }
        spawn.write(SpawnCompleteParticleEffectDocument {
            particle_effect_asset: trigger.handle.clone(),
            parent_entity: Some(entity),
            inherit_parent_rotation: true,
            effect_transform: Transform::IDENTITY,
            manual_particle_count: 0,
        });
        commands.entity(entity).insert(PrefabEffectHydrated);
    }
}
