use crate::physics::authored_water_surface_impact_projection::sample_accepted_water_surface;
use bevy::prelude::*;
use openzt2_game_assets::{
    effect::{
        effect_instance_spawning_and_preparation::spawn_complete_particle_effect_document,
        particle_plane_binding::ParticleEffectPlane, ParticleEffectDocumentAsset,
        SpawnCompleteParticleEffectDocument,
    },
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
};
use openzt2_zoo_state::terrain::{
    terrain_chunk_types::{TerrainChunk, TerrainIndex},
    terrain_water_presentation_types::{
        TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionQuery,
        TerrainWaterRegionRetired, TerrainWaterRegionSource,
    },
    terrain_world_sampling::terrain_chunk_at,
};

#[derive(Component)]
pub(super) struct PendingAnimationWaterEffect {
    pub(super) effect: Handle<ParticleEffectDocumentAsset>,
    pub(super) attach_to_target: bool,
    pub(super) disable_distance_culling: bool,
    pub(super) offset: f32,
}

#[allow(clippy::too_many_arguments)]
fn spawn_ready_animation_water_effects(
    commands: &mut Commands,
    pending: &Query<(Entity, &PendingAnimationWaterEffect, &ChildOf)>,
    transforms: &Query<&GlobalTransform>,
    chunks: &Query<&ViewVisibility, With<TerrainChunk>>,
    terrain_index: &TerrainIndex,
    indexes: &Query<&TerrainWaterMembershipIndex>,
    regions: &TerrainWaterRegionQuery,
    terrains: &Assets<TerrainAsset>,
) {
    for (job, effect, attachment) in pending {
        commands.entity(job).try_despawn();
        let target = attachment.parent();
        let Ok(pose) = transforms.get(target) else {
            continue;
        };
        let position = pose.translation().xz();
        if !effect.disable_distance_culling
            && !terrain_chunk_at(terrain_index, position)
                .and_then(|chunk| chunks.get(chunk).ok())
                .is_some_and(|visibility| visibility.get())
        {
            continue;
        }
        let Some((height, region)) =
            sample_accepted_water_surface(terrain_index, terrains, indexes, regions, position)
        else {
            continue;
        };
        let owner = effect.attach_to_target.then_some(target);
        let root = spawn_complete_particle_effect_document(
            commands,
            &SpawnCompleteParticleEffectDocument {
                particle_effect_asset: effect.effect.clone(),
                parent_entity: owner,
                inherit_parent_rotation: true,
                effect_transform: if owner.is_some() {
                    Transform::IDENTITY
                } else {
                    pose.compute_transform()
                },
                manual_particle_count: 0,
            },
        );
        let binding = AnimationWaterEffect {
            region,
            offset: effect.offset,
        };
        commands
            .entity(root)
            .insert((binding, binding.plane(height)));
    }
}

#[derive(Component, Clone, Copy)]
#[relationship(relationship_target = AnimationWaterEffects)]
pub(super) struct AnimationWaterEffect {
    #[relationship]
    pub(super) region: Entity,
    pub(super) offset: f32,
}

#[derive(Component)]
#[relationship_target(relationship = AnimationWaterEffect, linked_spawn)]
pub(super) struct AnimationWaterEffects(Vec<Entity>);

impl AnimationWaterEffect {
    pub(super) fn plane(self, height: f32) -> ParticleEffectPlane {
        let normal = if self.offset > 0.0 {
            Vec3::Y
        } else {
            Vec3::NEG_Y
        };
        ParticleEffectPlane(normal.extend(height + self.offset))
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update_animation_water_effect_planes(
    mut commands: Commands,
    pending: Query<(Entity, &PendingAnimationWaterEffect, &ChildOf)>,
    transforms: Query<&GlobalTransform>,
    chunks: Query<&ViewVisibility, With<TerrainChunk>>,
    terrain_index: Single<Ref<TerrainIndex>>,
    changed: Query<Entity, (With<AnimationWaterEffect>, Changed<GlobalTransform>)>,
    mut effects: Query<(
        Entity,
        Ref<AnimationWaterEffect>,
        &GlobalTransform,
        &mut ParticleEffectPlane,
    )>,
    indexes: Query<&TerrainWaterMembershipIndex>,
    changed_water: Query<
        (),
        Or<(
            Changed<TerrainWaterMembershipIndex>,
            Changed<TerrainWaterRegionEdit>,
            Changed<TerrainWaterRegionSource>,
            Added<TerrainWaterRegionRetired>,
        )>,
    >,
    mut removed_indexes: RemovedComponents<TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    terrains: Res<Assets<TerrainAsset>>,
) {
    let terrain_index = terrain_index.into_inner();
    spawn_ready_animation_water_effects(
        &mut commands,
        &pending,
        &transforms,
        &chunks,
        &terrain_index,
        &indexes,
        &regions,
        &terrains,
    );
    let index_removed = removed_indexes.read().count() != 0;
    let mut project = |(entity, binding, transform, mut plane): (
        Entity,
        Ref<AnimationWaterEffect>,
        &GlobalTransform,
        Mut<ParticleEffectPlane>,
    )| {
        // New effects already have the plane sampled at their resolved pose.
        if binding.is_added() {
            return;
        }
        let sample = sample_accepted_water_surface(
            &terrain_index,
            &terrains,
            &indexes,
            &regions,
            transform.translation().xz(),
        );
        if let Some((height, _)) = sample.filter(|(_, region)| *region == binding.region) {
            let next = binding.plane(height);
            if plane.0 != next.0 {
                plane.0 = next.0;
            }
        } else {
            commands.entity(entity).try_despawn();
        }
    };
    if index_removed || terrain_index.is_changed() || !changed_water.is_empty() {
        for effect in &mut effects {
            project(effect);
        }
    } else {
        let mut affected = effects.iter_many_mut(changed.iter()).matched();
        while let Some(effect) = affected.fetch_next() {
            project(effect);
        }
    }
}
