//! Blue Fang particle effects lowered on demand and prepared once for Hanabi.

use bevy::{asset::AssetApp, prelude::*};
use openzt2_game_data::{
    particle::{
        authored_particle_system::ParticleEffectSimulationClock,
        ParticleEffectDocument as ParticleDocument,
    },
    AssetId,
};

pub mod authored_particle_system_hanabi_effect_preparation;
pub mod effect_asset_loading;
pub mod effect_document_runtime_queries;
pub mod effect_instance_runtime_systems;
pub mod effect_instance_spawning_and_preparation;
pub mod effect_runtime_types;
pub mod effect_source_reloading;
pub mod explicit_particle_emission;
pub mod hanabi_custom_particle_modifiers;
pub mod legacy_nif_hanabi_effect_preparation;
pub mod particle_plane_binding;
pub mod particle_system_emitter_graph_validation;
pub mod particle_system_emitter_lowering;
pub mod particle_system_modifier_lowering;
pub mod particle_system_renderer_lowering;
pub mod particle_system_source_conversion_error;
pub mod particle_system_source_curve_lowering;
pub mod particle_system_source_node_traversal;
pub mod particle_system_source_scalar_reading;
pub mod particle_system_source_vector_reading;
pub mod particle_wave_surface;
pub mod preparation;
pub mod prepared_effect_asset_cache;
pub mod source;
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
pub struct ParticleEffectDocumentAsset {
    pub particle_document: ParticleDocument,
}

#[derive(Component, Clone, Debug)]
pub struct LiveParticleEffectInstance {
    simulation_clock: ParticleEffectSimulationClock,
    effect_asset: Handle<ParticleEffectDocumentAsset>,
    emitter_id: AssetId,
}

#[derive(Message, Clone, Debug)]
pub struct SpawnParticleEffectEmitter {
    pub particle_effect_asset: Handle<ParticleEffectDocumentAsset>,
    pub emitter_id: AssetId,
    pub parent_entity: Option<Entity>,
    pub effect_transform: Transform,
    pub manual_particle_count: u32,
}

#[derive(Message, Clone, Debug)]
pub struct SpawnCompleteParticleEffectDocument {
    pub particle_effect_asset: Handle<ParticleEffectDocumentAsset>,
    pub parent_entity: Option<Entity>,
    pub inherit_parent_rotation: bool,
    pub effect_transform: Transform,
    pub manual_particle_count: u32,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct StopParticleEffectsAttachedToEntity {
    pub attached_entity: Entity,
    /// None stops all documents attached to the entity.
    pub effect_asset: Option<bevy::asset::AssetId<ParticleEffectDocumentAsset>>,
}

pub struct ParticleEffectAssetAndRuntimePlugin;

impl Plugin for ParticleEffectAssetAndRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<ParticleEffectDocumentAsset>()
            .add_systems(
                PostUpdate,
                effect_instance_runtime_systems::align_attached_particle_effects_to_world_axes
                    .after(bevy::app::AnimationSystems)
                    .before(bevy::transform::TransformSystems::Propagate),
            )
            .init_asset_loader::<EffectAssetLoader>()
            .init_resource::<PreparedHanabiEffectAssets>()
            .add_message::<SpawnParticleEffectEmitter>()
            .add_message::<SpawnCompleteParticleEffectDocument>()
            .add_message::<StopParticleEffectsAttachedToEntity>()
            .add_message::<explicit_particle_emission::AppendParticleEffectPoints>()
            .add_systems(
                Update,
                (
                    refresh_prepared_effect_assets_from_source_changes,
                    enqueue_requested_effect_instances_for_preparation,
                    explicit_particle_emission::prepare_explicit_particle_emitters,
                    prepare_pending_effect_instances_from_loaded_authored_assets,
                    explicit_particle_emission::retain_explicit_particle_emitters,
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
                (
                    update_controlled_effect_emission_from_authored_timing,
                    explicit_particle_emission::upload_explicit_particle_emission_points,
                )
                    .chain()
                    .after(bevy_hanabi::prelude::EffectSystems::TickSpawners),
            )
            .add_systems(
                PostUpdate,
                particle_plane_binding::upload_particle_effect_planes
                    .before(bevy_hanabi::EffectSystems::UpdatePropertiesFromAsset),
            )
            .add_systems(
                PostUpdate,
                particle_wave_surface::upload_particle_wave_surface_uniforms
                    .before(bevy_hanabi::EffectSystems::UpdatePropertiesFromAsset),
            );
    }
}
