//! Blue Fang particle effects lowered on demand and prepared once for Hanabi.

use bevy::{asset::AssetApp, prelude::*};
use openzt2_game_data::{
    particle::{
        authored_particle_system::ParticleEffectSimulationClock,
        ParticleEffectDocument as ParticleDocument,
    },
    AssetId,
};

mod authored_particle_system_hanabi_effect_preparation;
mod effect_asset_loading;
mod effect_document_runtime_queries;
mod effect_instance_runtime_systems;
mod effect_instance_spawning_and_preparation;
mod effect_runtime_types;
mod effect_source_reloading;
mod hanabi_custom_particle_modifiers;
mod legacy_nif_hanabi_effect_preparation;
mod particle_system_emitter_graph_validation;
mod particle_system_emitter_lowering;
mod particle_system_modifier_lowering;
mod particle_system_renderer_lowering;
mod particle_system_source_conversion_error;
mod particle_system_source_curve_lowering;
mod particle_system_source_node_traversal;
mod particle_system_source_scalar_reading;
mod particle_system_source_vector_reading;
mod preparation;
mod prepared_effect_asset_cache;
mod source;
use effect_asset_loading::EffectAssetLoader;
use effect_instance_runtime_systems::{
    bind_loaded_material_textures_to_pending_effect_instances,
    bind_loaded_named_meshes_to_pending_effect_instances,
    retire_effect_instances_after_calculated_finite_lifetime,
    select_effect_simulation_delta_time_from_authored_clock,
    stop_effect_instances_attached_to_requested_entities,
    update_controlled_effect_emission_from_authored_timing,
};
use effect_instance_spawning_and_preparation::{
    enqueue_requested_effect_instances_for_preparation,
    prepare_pending_effect_instances_from_loaded_authored_assets,
};
use effect_source_reloading::refresh_prepared_effect_assets_from_source_changes;
use prepared_effect_asset_cache::PreparedHanabiEffectAssets;

#[derive(Asset, TypePath, Clone, Debug)]
pub(crate) struct ParticleEffectDocumentAsset {
    pub(crate) particle_document: ParticleDocument,
}

#[derive(Component, Clone, Debug)]
pub(crate) struct LiveParticleEffectInstance {
    simulation_clock: ParticleEffectSimulationClock,
    effect_asset: Handle<ParticleEffectDocumentAsset>,
    emitter_id: AssetId,
}

#[derive(Message, Clone, Debug)]
pub(crate) struct SpawnParticleEffectEmitter {
    pub(crate) particle_effect_asset: Handle<ParticleEffectDocumentAsset>,
    pub(crate) emitter_id: AssetId,
    pub(crate) parent_entity: Option<Entity>,
    pub(crate) effect_transform: Transform,
    pub(crate) manual_particle_count: u32,
}

#[derive(Message, Clone, Debug)]
pub(crate) struct SpawnCompleteParticleEffectDocument {
    pub(crate) particle_effect_asset: Handle<ParticleEffectDocumentAsset>,
    pub(crate) parent_entity: Option<Entity>,
    pub(crate) effect_transform: Transform,
    pub(crate) manual_particle_count: u32,
}

#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct StopParticleEffectsAttachedToEntity {
    pub(crate) attached_entity: Entity,
}

pub(crate) struct ParticleEffectAssetAndRuntimePlugin;

impl Plugin for ParticleEffectAssetAndRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<ParticleEffectDocumentAsset>()
            .init_asset_loader::<EffectAssetLoader>()
            .init_resource::<PreparedHanabiEffectAssets>()
            .add_message::<SpawnParticleEffectEmitter>()
            .add_message::<SpawnCompleteParticleEffectDocument>()
            .add_message::<StopParticleEffectsAttachedToEntity>()
            .add_systems(
                Update,
                (
                    refresh_prepared_effect_assets_from_source_changes,
                    enqueue_requested_effect_instances_for_preparation,
                    prepare_pending_effect_instances_from_loaded_authored_assets,
                    stop_effect_instances_attached_to_requested_entities,
                    bind_loaded_material_textures_to_pending_effect_instances,
                    bind_loaded_named_meshes_to_pending_effect_instances,
                    select_effect_simulation_delta_time_from_authored_clock,
                    retire_effect_instances_after_calculated_finite_lifetime,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                update_controlled_effect_emission_from_authored_timing
                    .after(bevy_hanabi::prelude::EffectSystems::TickSpawners),
            );
    }
}
