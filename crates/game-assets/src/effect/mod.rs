//! Blue Fang particle effects lowered on demand and prepared once for Hanabi.

use bevy::{asset::AssetApp, prelude::*};
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

pub mod authored_particle_system_hanabi_effect_preparation;
pub mod effect_asset_loading;
mod effect_document_runtime_queries;
pub mod effect_instance_runtime_systems;
pub mod effect_instance_spawning_and_preparation;
pub mod effect_runtime_types;
pub mod explicit_particle_emission;
pub mod hanabi_custom_particle_modifiers;
pub mod legacy_nif_hanabi_effect_preparation;
pub mod particle_plane_binding;
mod particle_renderer_materials;
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
pub(crate) mod prepared_effect_emitters;
pub mod source;
use effect_asset_loading::EffectAssetLoader;
use effect_instance_runtime_systems::{
    confirm_particle_readbacks_before_draining_collision_children,
    retire_empty_particle_effect_document_roots, tick_authored_particle_emitters,
};
use effect_instance_spawning_and_preparation::{
    enqueue_requested_effect_instances_for_preparation, reconcile_loaded_particle_effect_instances,
};

#[derive(Asset, TypePath, Clone, Debug)]
pub struct ParticleEffectDocumentAsset {
    roots: Vec<AssetId>,
    decal_emitter: Option<(AssetId, bool)>,
    #[dependency]
    prepared_emitters: BTreeMap<AssetId, prepared_effect_emitters::PreparedParticleEmitter>,
}

impl ParticleEffectDocumentAsset {
    pub fn root_emitter_ids(&self) -> &[AssetId] {
        &self.roots
    }

    /// Authored PSYS roots and the maximum initial particle size of each root.
    pub fn authored_root_emitters_with_initial_size(
        &self,
    ) -> impl Iterator<Item = (AssetId, f32)> + '_ {
        self.roots.iter().filter_map(|&id| {
            self.prepared_emitters
                .get(&id)?
                .authored_initial_size
                .map(|size| (id, size))
        })
    }

    /// The first decal emitter in authored order and whether it has a wave shader.
    pub fn decal_emitter(&self) -> Option<(AssetId, bool)> {
        self.decal_emitter
    }
}

#[derive(Component, Clone, Debug)]
pub struct LiveParticleEffectInstance {
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
            .add_observer(effect_instance_runtime_systems::retire_drained_particle_effect_tree)
            .add_plugins(particle_plane_binding::ParticleEffectPlaneBindingPlugin)
            .add_systems(
                PostUpdate,
                effect_instance_runtime_systems::align_attached_particle_effects_to_world_axes
                    .after(bevy::app::AnimationSystems)
                    .before(bevy::transform::TransformSystems::Propagate),
            )
            .init_asset_loader::<EffectAssetLoader>()
            .add_message::<SpawnParticleEffectEmitter>()
            .add_message::<SpawnCompleteParticleEffectDocument>()
            .add_message::<StopParticleEffectsAttachedToEntity>()
            .add_message::<explicit_particle_emission::AppendParticleEffectPoints>()
            .add_systems(
                Update,
                (
                    (
                        enqueue_requested_effect_instances_for_preparation,
                        explicit_particle_emission::prepare_explicit_particle_emitters,
                    )
                        .before(reconcile_loaded_particle_effect_instances),
                    reconcile_loaded_particle_effect_instances,
                    effect_instance_runtime_systems::stop_requested_particle_effect_children
                        .after(reconcile_loaded_particle_effect_instances)
                        .run_if(
                            bevy::ecs::schedule::common_conditions::on_message::<
                                StopParticleEffectsAttachedToEntity,
                            >,
                        ),
                ),
            )
            .add_systems(
                Update,
                particle_renderer_materials::project_particle_renderer_materials
                    .after(reconcile_loaded_particle_effect_instances),
            )
            .add_systems(
                PostUpdate,
                (
                    effect_instance_runtime_systems::publish_ready_particle_effect_graphs
                        .after(bevy_hanabi::EffectSystems::CompileEffects),
                    tick_authored_particle_emitters
                        .after(
                            effect_instance_runtime_systems::publish_ready_particle_effect_graphs,
                        )
                        .in_set(bevy_hanabi::EffectSystems::TickSpawners),
                ),
            )
            .add_systems(
                PostUpdate,
                (
                    explicit_particle_emission::collect_requested_particle_points.run_if(
                        on_message::<explicit_particle_emission::AppendParticleEffectPoints>,
                    ),
                    explicit_particle_emission::clear_submitted_particle_points,
                    explicit_particle_emission::upload_queued_particle_points
                        .after(explicit_particle_emission::collect_requested_particle_points)
                        .after(explicit_particle_emission::clear_submitted_particle_points)
                        .before(confirm_particle_readbacks_before_draining_collision_children),
                    (
                        confirm_particle_readbacks_before_draining_collision_children,
                        retire_empty_particle_effect_document_roots,
                    )
                        .chain(),
                )
                    .after(bevy_hanabi::EffectSystems::TickSpawners),
            )
            .add_systems(
                PostUpdate,
                (
                    particle_wave_surface::project_particle_view_fog
                        .after(TransformSystems::Propagate),
                    particle_wave_surface::write_particle_wave_surface_uniforms
                        .before(bevy_hanabi::EffectSystems::UpdatePropertiesFromAsset),
                ),
            );
    }
}
