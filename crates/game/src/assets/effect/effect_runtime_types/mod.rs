//! Private Bevy runtime state for loading and running particle-effect instances.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::{
    hanabi_custom_particle_modifiers::CollisionSpawnParticleInitialization,
    ParticleEffectDocumentAsset,
};
use crate::assets::material::material_asset_types::MaterialAsset;

#[derive(Component, Clone)]
pub(super) struct PendingEffectInstance {
    pub(super) effect_asset: Handle<ParticleEffectDocumentAsset>,
    pub(super) requested_emitter: Option<AssetId>,
    pub(super) manual_particle_count: u32,
    pub(super) collision_spawn_context: Option<CollisionSpawnParticleInitialization>,
}

#[derive(Component)]
pub(super) struct FiniteEffectInstanceLifetime(pub(super) Timer);

#[derive(Component, Clone, Debug)]
pub(super) struct ControlledEffectEmissionState {
    pub(super) elapsed_seconds: f32,
    pub(super) fractional_particle_remainder: f32,
    pub(super) manual_particle_count: u32,
}

#[derive(Component)]
pub(super) struct PendingEffectMaterialBinding(pub(super) Handle<MaterialAsset>);

#[derive(Component)]
pub(super) struct PendingEffectMeshBinding {
    pub(super) model_asset: Handle<bevy::gltf::Gltf>,
    pub(super) named_mesh_asset_path: String,
}
