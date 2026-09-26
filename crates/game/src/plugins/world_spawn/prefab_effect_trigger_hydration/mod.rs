use bevy::prelude::*;

use crate::assets::{
    effect::{ParticleEffectDocumentAsset, SpawnCompleteParticleEffectDocument},
    scene_prefab::ScenePrefabAsset,
};

use super::world_membership_types::WorldMember;

/// One prefab-authored request to instantiate every emitter in an effect.
#[derive(Component, Debug, Clone)]
pub(super) struct PrefabEffectTrigger {
    handle: Handle<ParticleEffectDocumentAsset>,
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
    triggers: Query<(Entity, &PrefabEffectTrigger, &WorldMember), Without<PrefabEffectHydrated>>,
    effects: Res<Assets<ParticleEffectDocumentAsset>>,
    mut spawn: MessageWriter<SpawnCompleteParticleEffectDocument>,
) {
    for (entity, trigger, _) in &triggers {
        if effects.get(&trigger.handle).is_none() {
            continue;
        }
        spawn.write(SpawnCompleteParticleEffectDocument {
            particle_effect_asset: trigger.handle.clone(),
            parent_entity: Some(entity),
            effect_transform: Transform::IDENTITY,
            manual_particle_count: 0,
        });
        commands.entity(entity).insert(PrefabEffectHydrated);
    }
}
