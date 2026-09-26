//! Converts effect spawn requests into prepared Hanabi effect entities.

use bevy::prelude::*;
use bevy_hanabi::prelude::{EffectAsset as HanabiEffectAsset, ParticleEffect};
use openzt2_game_data::{
    particle::{
        legacy_nif_particle_effect::LegacyNifParticleModifier,
        ParticleEffectDocument as ParticleDocument,
    },
    AssetId,
};

use super::{
    effect_document_runtime_queries::{
        calculate_effect_emitter_tree_lifetime_seconds, collect_all_effect_emitter_ids,
        collect_root_effect_emitter_ids, create_collision_spawn_initialization,
        select_effect_emitter_clock,
    },
    effect_runtime_types::{
        ControlledEffectEmissionState, FiniteEffectInstanceLifetime, PendingEffectInstance,
        PendingEffectMaterialBinding, PendingEffectMeshBinding,
    },
    prepared_effect_asset_cache::PreparedHanabiEffectAssets,
    LiveParticleEffectInstance, ParticleEffectDocumentAsset, SpawnCompleteParticleEffectDocument,
    SpawnParticleEffectEmitter,
};

pub(super) fn enqueue_requested_effect_instances_for_preparation(
    mut commands: Commands,
    mut single_emitter_requests: MessageReader<SpawnParticleEffectEmitter>,
    mut complete_effect_requests: MessageReader<SpawnCompleteParticleEffectDocument>,
) {
    for spawn_request in single_emitter_requests.read() {
        spawn_pending_effect_instance(
            &mut commands,
            spawn_request.particle_effect_asset.clone(),
            Some(spawn_request.emitter_id),
            spawn_request.parent_entity,
            spawn_request.effect_transform,
            spawn_request.manual_particle_count,
        );
    }
    for spawn_request in complete_effect_requests.read() {
        spawn_pending_effect_instance(
            &mut commands,
            spawn_request.particle_effect_asset.clone(),
            None,
            spawn_request.parent_entity,
            spawn_request.effect_transform,
            spawn_request.manual_particle_count,
        );
    }
}

fn spawn_pending_effect_instance(
    commands: &mut Commands,
    effect_asset: Handle<ParticleEffectDocumentAsset>,
    requested_emitter: Option<AssetId>,
    parent_entity: Option<Entity>,
    effect_transform: Transform,
    manual_particle_count: u32,
) {
    let mut pending_effect_entity = commands.spawn((
        PendingEffectInstance {
            effect_asset,
            requested_emitter,
            manual_particle_count,
            collision_spawn_context: None,
        },
        effect_transform,
    ));
    if let Some(parent_entity) = parent_entity {
        pending_effect_entity.insert(ChildOf(parent_entity));
    }
}

pub(super) fn prepare_pending_effect_instances_from_loaded_authored_assets(
    mut commands: Commands,
    effect_assets: Res<Assets<ParticleEffectDocumentAsset>>,
    asset_server: Res<AssetServer>,
    mut hanabi_effect_assets: ResMut<Assets<HanabiEffectAsset>>,
    mut prepared_hanabi_effect_assets: ResMut<PreparedHanabiEffectAssets>,
    pending_effect_instances: Query<(Entity, &PendingEffectInstance)>,
) {
    for (effect_entity, pending_effect_instance) in &pending_effect_instances {
        let Some(effect_asset) = effect_assets.get(&pending_effect_instance.effect_asset) else {
            continue;
        };
        let all_emitter_ids = collect_all_effect_emitter_ids(&effect_asset.particle_document);
        let root_emitter_ids = collect_root_effect_emitter_ids(&effect_asset.particle_document);
        if pending_effect_instance.requested_emitter.is_none() && root_emitter_ids.len() > 1 {
            for root_emitter_id in root_emitter_ids {
                spawn_pending_effect_instance(
                    &mut commands,
                    pending_effect_instance.effect_asset.clone(),
                    Some(root_emitter_id),
                    Some(effect_entity),
                    Transform::IDENTITY,
                    pending_effect_instance.manual_particle_count,
                );
            }
            commands
                .entity(effect_entity)
                .remove::<PendingEffectInstance>();
            continue;
        }
        let Some(authored_emitter_id) = pending_effect_instance
            .requested_emitter
            .or_else(|| root_emitter_ids.first().copied())
        else {
            error!(
                effect_asset = ?pending_effect_instance.effect_asset.id(),
                "particle effect document contains no root emitter"
            );
            commands.entity(effect_entity).despawn();
            continue;
        };
        if !all_emitter_ids.contains(&authored_emitter_id) {
            error!(
                effect_asset = ?pending_effect_instance.effect_asset.id(),
                emitter = ?authored_emitter_id,
                "requested particle emitter does not exist in the loaded effect document"
            );
            commands.entity(effect_entity).despawn();
            continue;
        }
        let Some(hanabi_effect_handle) = prepared_hanabi_effect_assets.prepare_effect_asset(
            pending_effect_instance.effect_asset.id(),
            &effect_asset.particle_document,
            authored_emitter_id,
            pending_effect_instance.collision_spawn_context.clone(),
            &mut hanabi_effect_assets,
        ) else {
            error!(
                effect_asset = ?pending_effect_instance.effect_asset.id(),
                emitter = ?authored_emitter_id,
                "particle emitter preparation rejected an emitter absent from the loaded effect document"
            );
            commands.entity(effect_entity).despawn();
            continue;
        };
        {
            let mut prepared_effect_entity = commands.entity(effect_entity);
            prepared_effect_entity
                .remove::<PendingEffectInstance>()
                .insert((
                    ParticleEffect::new(hanabi_effect_handle),
                    LiveParticleEffectInstance {
                        effect_asset: pending_effect_instance.effect_asset.clone(),
                        emitter_id: authored_emitter_id,
                        simulation_clock: select_effect_emitter_clock(
                            &effect_asset.particle_document,
                            authored_emitter_id,
                        ),
                    },
                    bevy_hanabi::prelude::EffectDeltaTime(0.0),
                    ControlledEffectEmissionState {
                        elapsed_seconds: 0.0,
                        fractional_particle_remainder: 0.0,
                        manual_particle_count: pending_effect_instance.manual_particle_count,
                    },
                ));
            if let Some(effect_lifetime_seconds) = calculate_effect_emitter_tree_lifetime_seconds(
                &effect_asset.particle_document,
                authored_emitter_id,
            ) {
                prepared_effect_entity.insert(FiniteEffectInstanceLifetime(Timer::from_seconds(
                    effect_lifetime_seconds.max(f32::EPSILON),
                    TimerMode::Once,
                )));
            }
        }
        add_legacy_nif_particle_mesh_binding(
            &mut commands,
            &asset_server,
            &effect_asset.particle_document,
            effect_entity,
        );
        add_authored_particle_system_material_and_collision_spawn_bindings(
            &mut commands,
            &asset_server,
            &effect_asset.particle_document,
            &pending_effect_instance.effect_asset,
            effect_entity,
            authored_emitter_id,
        );
    }
}

pub(super) fn add_legacy_nif_particle_mesh_binding(
    commands: &mut Commands,
    asset_server: &AssetServer,
    particle_document: &ParticleDocument,
    effect_entity: Entity,
) {
    let ParticleDocument::LegacyNif(legacy_nif_effect) = particle_document else {
        return;
    };
    let Some(prototype_geometry_block_index) =
        legacy_nif_effect
            .modifiers
            .iter()
            .find_map(|particle_modifier| match particle_modifier {
                LegacyNifParticleModifier::MeshSelection { prototype_blocks } => prototype_blocks
                    .iter()
                    .copied()
                    .find(|prototype_block| *prototype_block >= 0),
                _ => None,
            })
    else {
        return;
    };
    let original_nif_asset_path = std::path::Path::new(&legacy_nif_effect.model)
        .with_extension("nif")
        .to_string_lossy()
        .into_owned();
    commands
        .entity(effect_entity)
        .insert(PendingEffectMeshBinding {
            model_asset: asset_server.load(legacy_nif_effect.model.clone()),
            named_mesh_asset_path: format!(
                "{original_nif_asset_path}#geometry/nif_{prototype_geometry_block_index:08x}"
            ),
        });
}

pub(super) fn add_authored_particle_system_material_and_collision_spawn_bindings(
    commands: &mut Commands,
    asset_server: &AssetServer,
    particle_document: &ParticleDocument,
    effect_asset_handle: &Handle<ParticleEffectDocumentAsset>,
    effect_entity: Entity,
    authored_emitter_id: AssetId,
) {
    let ParticleDocument::Effect(effect_document) = particle_document else {
        return;
    };
    let Some(effect_emitter) = effect_document
        .emitters
        .iter()
        .find(|candidate_emitter| candidate_emitter.emitter_id == authored_emitter_id)
    else {
        return;
    };
    if !effect_emitter.material.is_empty() {
        commands
            .entity(effect_entity)
            .insert(PendingEffectMaterialBinding(
                asset_server.load(effect_emitter.material.clone()),
            ));
    }
    for (target_emitter_id, collision_spawn_context) in
        effect_emitter.modifiers.iter().enumerate().filter_map(
            |(modifier_index, effect_modifier)| {
                create_collision_spawn_initialization(
                    authored_emitter_id,
                    modifier_index,
                    effect_modifier,
                )
            },
        )
    {
        commands.spawn((
            PendingEffectInstance {
                effect_asset: effect_asset_handle.clone(),
                requested_emitter: Some(target_emitter_id),
                manual_particle_count: 0,
                collision_spawn_context: Some(collision_spawn_context),
            },
            ChildOf(effect_entity),
            bevy_hanabi::prelude::EffectParent::new(effect_entity),
            Transform::IDENTITY,
        ));
    }
}
