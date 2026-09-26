//! Refreshes prepared particles and live bindings after a source change.

use super::{
    effect_document_runtime_queries::{
        calculate_effect_emitter_tree_lifetime_seconds, select_effect_emitter_clock,
    },
    effect_instance_spawning_and_preparation::{
        add_authored_particle_system_material_and_collision_spawn_bindings,
        add_legacy_nif_particle_mesh_binding,
    },
    effect_runtime_types::{
        ControlledEffectEmissionState, FiniteEffectInstanceLifetime, PendingEffectMaterialBinding,
        PendingEffectMeshBinding,
    },
    prepared_effect_asset_cache::PreparedHanabiEffectAssets,
    LiveParticleEffectInstance, ParticleEffectDocumentAsset,
};
use bevy::prelude::*;
use bevy_hanabi::prelude::{
    EffectAsset as HanabiEffectAsset, EffectMaterial, EffectMesh, EffectParent, EffectSpawner,
    ParticleEffect,
};
use std::collections::HashSet;

pub(super) fn refresh_prepared_effect_assets_from_source_changes(
    mut commands: Commands,
    mut source_events: MessageReader<AssetEvent<ParticleEffectDocumentAsset>>,
    source_assets: Res<Assets<ParticleEffectDocumentAsset>>,
    asset_server: Res<AssetServer>,
    mut prepared_assets: ResMut<PreparedHanabiEffectAssets>,
    mut hanabi_assets: ResMut<Assets<HanabiEffectAsset>>,
    mut live_instances: Query<(
        Entity,
        &mut LiveParticleEffectInstance,
        &mut ParticleEffect,
        Option<&mut EffectSpawner>,
        &ControlledEffectEmissionState,
        Option<&FiniteEffectInstanceLifetime>,
        Option<&EffectParent>,
    )>,
) {
    let mut refreshed_documents = HashSet::new();
    for event in source_events.read() {
        match *event {
            AssetEvent::Modified { id } => {
                if !refreshed_documents.insert(id) {
                    continue;
                }
                let Some(source) = source_assets.get(id) else {
                    continue;
                };
                prepared_assets.refresh_source_assets(
                    id,
                    &source.particle_document,
                    &mut hanabi_assets,
                );
                for (
                    entity,
                    mut instance,
                    mut particle_effect,
                    spawner,
                    emission_state,
                    previous_lifetime,
                    parent_effect,
                ) in &mut live_instances
                {
                    if instance.effect_asset.id() == id {
                        if parent_effect.is_some() {
                            commands.entity(entity).try_despawn();
                            continue;
                        }
                        let Some(prepared) = hanabi_assets.get(&particle_effect.handle) else {
                            commands.entity(entity).try_despawn();
                            continue;
                        };
                        instance.simulation_clock = select_effect_emitter_clock(
                            &source.particle_document,
                            instance.emitter_id,
                        );
                        particle_effect.set_changed();
                        if let Some(mut spawner) = spawner {
                            spawner.settings = prepared.spawner;
                            spawner.active = prepared.spawner.starts_active();
                        }
                        if let Some(duration) = calculate_effect_emitter_tree_lifetime_seconds(
                            &source.particle_document,
                            instance.emitter_id,
                        ) {
                            let mut timer =
                                Timer::from_seconds(duration.max(f32::EPSILON), TimerMode::Once);
                            timer.set_elapsed(previous_lifetime.map_or_else(
                                || {
                                    std::time::Duration::from_secs_f32(
                                        emission_state.elapsed_seconds,
                                    )
                                },
                                |lifetime| lifetime.0.elapsed(),
                            ));
                            commands
                                .entity(entity)
                                .insert(FiniteEffectInstanceLifetime(timer));
                        } else {
                            commands
                                .entity(entity)
                                .remove::<FiniteEffectInstanceLifetime>();
                        }
                        commands.entity(entity).remove::<(
                            EffectMaterial,
                            EffectMesh,
                            PendingEffectMaterialBinding,
                            PendingEffectMeshBinding,
                        )>();
                        add_legacy_nif_particle_mesh_binding(
                            &mut commands,
                            &asset_server,
                            &source.particle_document,
                            entity,
                        );
                        add_authored_particle_system_material_and_collision_spawn_bindings(
                            &mut commands,
                            &asset_server,
                            &source.particle_document,
                            &instance.effect_asset,
                            entity,
                            instance.emitter_id,
                        );
                    }
                }
            }
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => {
                prepared_assets.release_source_assets(id);
                for (entity, instance, _, _, _, _, _) in &live_instances {
                    if instance.effect_asset.id() == id {
                        commands.entity(entity).try_despawn();
                    }
                }
            }
            AssetEvent::Added { .. } | AssetEvent::LoadedWithDependencies { .. } => {}
        }
    }
}
