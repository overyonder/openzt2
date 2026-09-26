//! Shared Hanabi assets, retained while their source documents are loaded.

use super::{
    effect_document_runtime_queries::create_collision_spawn_initialization,
    hanabi_custom_particle_modifiers::CollisionSpawnParticleInitialization,
    preparation::prepare_particle_document_as_hanabi_effect_asset, ParticleEffectDocumentAsset,
};
use bevy::prelude::*;
use bevy_hanabi::prelude::EffectAsset as HanabiEffectAsset;
use openzt2_game_data::{particle::ParticleEffectDocument, AssetId};
use std::collections::HashMap;

type PreparedEffectKey = (
    bevy::asset::AssetId<ParticleEffectDocumentAsset>,
    AssetId,
    Option<(AssetId, u32)>,
);

#[derive(Resource, Default)]
pub(super) struct PreparedHanabiEffectAssets {
    assets: HashMap<PreparedEffectKey, Handle<HanabiEffectAsset>>,
}

impl PreparedHanabiEffectAssets {
    pub(super) fn prepare_effect_asset(
        &mut self,
        document_id: bevy::asset::AssetId<ParticleEffectDocumentAsset>,
        document: &ParticleEffectDocument,
        emitter_id: AssetId,
        collision_spawn: Option<CollisionSpawnParticleInitialization>,
        hanabi_assets: &mut Assets<HanabiEffectAsset>,
    ) -> Option<Handle<HanabiEffectAsset>> {
        let key = (
            document_id,
            emitter_id,
            collision_spawn
                .as_ref()
                .map(|spawn| (spawn.parent_emitter_id, spawn.modifier_index)),
        );
        if let Some(prepared) = self.assets.get(&key) {
            return Some(prepared.clone());
        }
        let prepared = prepare_particle_document_as_hanabi_effect_asset(
            document,
            emitter_id,
            collision_spawn,
        )?;
        let handle = hanabi_assets.add(prepared);
        self.assets.insert(key, handle.clone());
        Some(handle)
    }
    pub(super) fn refresh_source_assets(
        &mut self,
        id: bevy::asset::AssetId<ParticleEffectDocumentAsset>,
        document: &ParticleEffectDocument,
        hanabi_assets: &mut Assets<HanabiEffectAsset>,
    ) {
        self.assets
            .retain(|(document_id, emitter_id, collision_spawn), handle| {
                if *document_id != id {
                    return true;
                }
                let prepared = match collision_spawn {
                    Some((parent_emitter_id, modifier_index)) => {
                        let ParticleEffectDocument::Effect(effect_document) = document else {
                            hanabi_assets.remove(handle.id());
                            return false;
                        };
                        effect_document
                            .emitters
                            .iter()
                            .find(|emitter| emitter.emitter_id == *parent_emitter_id)
                            .and_then(|emitter| emitter.modifiers.get(*modifier_index as usize))
                            .and_then(|modifier| {
                                create_collision_spawn_initialization(
                                    *parent_emitter_id,
                                    *modifier_index as usize,
                                    modifier,
                                )
                            })
                            .filter(|(target, _)| target == emitter_id)
                            .and_then(|(_, spawn)| {
                                prepare_particle_document_as_hanabi_effect_asset(
                                    document,
                                    *emitter_id,
                                    Some(spawn),
                                )
                            })
                    }
                    None => prepare_particle_document_as_hanabi_effect_asset(
                        document,
                        *emitter_id,
                        None,
                    ),
                };
                if let Some(prepared) = prepared {
                    // Keep the handle so live instances receive the updated GPU asset.
                    hanabi_assets
                        .insert(handle.id(), prepared)
                        .expect("cached effect handles retain their asset slots");
                    true
                } else {
                    hanabi_assets.remove(handle.id());
                    false
                }
            });
    }

    pub(super) fn release_source_assets(
        &mut self,
        id: bevy::asset::AssetId<ParticleEffectDocumentAsset>,
    ) {
        self.assets
            .retain(|(document_id, _, _), _| *document_id != id);
    }
}
