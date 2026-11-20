//! Projects loaded particle graphs onto Bevy and Hanabi entities.

use std::collections::HashSet;

use bevy::asset::{
    AssetId as BevyAssetId, AssetLoadFailedEvent, LoadState, RecursiveDependencyLoadState,
};
use bevy::ecs::system::SystemParam;
use bevy::gltf::GltfMesh;
use bevy::prelude::*;
use bevy_hanabi::{
    EffectAsset, EffectDeltaTime, EffectMesh, EffectParent, EffectSpawner, ParticleEffect,
};
use openzt2_game_data::AssetId;

use super::{
    effect_runtime_types::{
        ControlledEffectEmissionState, EffectRetirementReadback, ParticleEffectDocumentInstance,
        ParticleEffectGraphReady, PendingEffectInstance,
    },
    explicit_particle_emission::ParticleEffectPointEmitter,
    particle_plane_binding::UsesParticleEffectPlane,
    particle_wave_surface::ParticleEffectWaveSurface,
    prepared_effect_emitters::PreparedParticleEmitter,
    LiveParticleEffectInstance, ParticleEffectDocumentAsset, SpawnCompleteParticleEffectDocument,
    SpawnParticleEffectEmitter,
};
use crate::material::material_asset_types::MaterialAsset;

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
            true,
            spawn_request.effect_transform,
            spawn_request.manual_particle_count,
        );
    }
    for spawn_request in complete_effect_requests.read() {
        spawn_complete_particle_effect_document(&mut commands, spawn_request);
    }
}

pub fn spawn_complete_particle_effect_document(
    commands: &mut Commands,
    request: &SpawnCompleteParticleEffectDocument,
) -> Entity {
    let entity = spawn_pending_effect_instance(
        commands,
        request.particle_effect_asset.clone(),
        None,
        request.parent_entity,
        request.inherit_parent_rotation,
        request.effect_transform,
        request.manual_particle_count,
    );
    commands
        .entity(entity)
        .insert(super::effect_runtime_types::ParticleEffectDocumentInstance(
            request.particle_effect_asset.clone(),
        ));
    entity
}

fn spawn_pending_effect_instance(
    commands: &mut Commands,
    effect_asset: Handle<ParticleEffectDocumentAsset>,
    requested_emitter: Option<AssetId>,
    parent_entity: Option<Entity>,
    inherit_parent_rotation: bool,
    effect_transform: Transform,
    manual_particle_count: u32,
) -> Entity {
    let mut pending_effect_entity = commands.spawn((
        PendingEffectInstance {
            effect_asset,
            requested_emitter,
            manual_particle_count,
        },
        effect_transform,
        // An effect with several root emitters keeps this entity as their
        // parent; without visibility of its own, they are never shown.
        Visibility::default(),
    ));
    if let Some(parent_entity) = parent_entity {
        pending_effect_entity.insert(ChildOf(parent_entity));
        if !inherit_parent_rotation {
            pending_effect_entity.insert(super::effect_runtime_types::WorldAlignedParticleEffect);
        }
    }
    pending_effect_entity.id()
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) enum AuthoredParticleChild {
    Root(AssetId),
    Collision { emitter: AssetId, occurrence: usize },
}

#[derive(SystemParam)]
pub(super) struct PreparedParticleInstanceProjection<'w, 's> {
    commands: Commands<'w, 's>,
    prototype_meshes: Res<'w, Assets<GltfMesh>>,
    effects: Res<'w, Assets<EffectAsset>>,
    children: Query<'w, 's, &'static Children>,
    child_roles: Query<'w, 's, &'static AuthoredParticleChild>,
    live: Query<
        'w,
        's,
        (
            &'static mut LiveParticleEffectInstance,
            &'static mut ParticleEffect,
            Option<&'static mut EffectSpawner>,
            Option<&'static mut EffectRetirementReadback>,
            Option<&'static mut EffectMesh>,
            Has<UsesParticleEffectPlane>,
            Has<ParticleEffectPointEmitter>,
        ),
    >,
}

pub(super) fn reconcile_loaded_particle_effect_instances(
    documents: Res<Assets<ParticleEffectDocumentAsset>>,
    asset_server: Res<AssetServer>,
    mut events: MessageReader<AssetEvent<ParticleEffectDocumentAsset>>,
    mut failures: MessageReader<AssetLoadFailedEvent<ParticleEffectDocumentAsset>>,
    mut material_failures: MessageReader<AssetLoadFailedEvent<MaterialAsset>>,
    mut mesh_failures: MessageReader<AssetLoadFailedEvent<GltfMesh>>,
    mut pending_reloads: Local<HashSet<BevyAssetId<ParticleEffectDocumentAsset>>>,
    pending: Query<(
        Entity,
        Ref<PendingEffectInstance>,
        Has<ParticleEffectWaveSurface>,
    )>,
    document_roots: Query<
        (Entity, &ParticleEffectDocumentInstance),
        Without<PendingEffectInstance>,
    >,
    mut graph: ParamSet<(
        Query<
            (
                Entity,
                &LiveParticleEffectInstance,
                Has<ParticleEffectWaveSurface>,
            ),
            (
                Without<AuthoredParticleChild>,
                Without<PendingEffectInstance>,
            ),
        >,
        PreparedParticleInstanceProjection,
    )>,
) {
    let mut changed = HashSet::new();
    let mut removed = HashSet::new();
    for event in events.read() {
        match *event {
            AssetEvent::Added { id } | AssetEvent::LoadedWithDependencies { id } => {
                changed.insert(id);
            }
            AssetEvent::Modified { id } => {
                changed.insert(id);
                pending_reloads.insert(id);
            }
            AssetEvent::Removed { id } => {
                removed.insert(id);
                pending_reloads.remove(&id);
            }
            AssetEvent::Unused { .. } => {}
        }
    }
    let failed = failures
        .read()
        .map(|event| event.id)
        .collect::<HashSet<BevyAssetId<_>>>();
    let dependency_failed =
        (material_failures.read().count() != 0) | (mesh_failures.read().count() != 0);
    let reloaded = pending_reloads
        .iter()
        .copied()
        .filter(|&id| {
            changed.contains(&id)
                && documents.contains(id)
                && asset_server.is_loaded_with_dependencies(id)
        })
        .collect::<HashSet<_>>();
    pending_reloads.retain(|id| !reloaded.contains(id) && !failed.contains(id));
    let emitter_roots = if reloaded.is_empty() && removed.is_empty() {
        Vec::new()
    } else {
        graph
            .p0()
            .iter()
            .filter_map(|(entity, instance, wave)| {
                let id = instance.effect_asset.id();
                (reloaded.contains(&id) || removed.contains(&id)).then(|| {
                    (
                        entity,
                        instance.effect_asset.clone(),
                        instance.emitter_id,
                        wave,
                    )
                })
            })
            .collect::<Vec<_>>()
    };
    let mut projection = graph.p1();
    for (entity, request, wave) in &pending {
        let id = request.effect_asset.id();
        if failed.contains(&id) || removed.contains(&id) {
            projection.commands.entity(entity).try_despawn();
            continue;
        }
        if !request.is_added() && !changed.contains(&id) && !dependency_failed {
            continue;
        }
        if matches!(asset_server.get_load_state(id), Some(LoadState::Failed(_)))
            || matches!(
                asset_server.get_recursive_dependency_load_state(id),
                Some(RecursiveDependencyLoadState::Failed(_))
            )
        {
            projection.commands.entity(entity).try_despawn();
            continue;
        }
        if !asset_server.is_loaded_with_dependencies(id) {
            continue;
        }
        let Some(document) = documents.get(id) else {
            continue;
        };
        if let Some(emitter) = request.requested_emitter {
            if !projection.project_root_emitter(
                entity,
                &request.effect_asset,
                document,
                emitter,
                wave,
                request.manual_particle_count,
            ) {
                continue;
            }
        } else if !projection.project_document_roots(
            entity,
            &request.effect_asset,
            document,
            request.manual_particle_count,
        ) {
            continue;
        }
        projection
            .commands
            .entity(entity)
            .remove::<PendingEffectInstance>();
    }
    if reloaded.is_empty() && removed.is_empty() {
        return;
    }
    for (entity, document_instance) in &document_roots {
        let source = &document_instance.0;
        if removed.contains(&source.id()) && !documents.contains(source.id()) {
            projection.commands.entity(entity).try_despawn();
        } else if reloaded.contains(&source.id()) {
            if let Some(document) = documents.get(source) {
                projection.project_document_roots(entity, source, document, 0);
            }
        }
    }
    for (entity, source, emitter, wave) in emitter_roots {
        if removed.contains(&source.id()) && !documents.contains(source.id()) {
            projection.commands.entity(entity).try_despawn();
        } else if reloaded.contains(&source.id()) {
            if let Some(document) = documents.get(&source) {
                projection.project_root_emitter(entity, &source, document, emitter, wave, 0);
            }
        }
    }
}

impl PreparedParticleInstanceProjection<'_, '_> {
    fn project_document_roots(
        &mut self,
        entity: Entity,
        source: &Handle<ParticleEffectDocumentAsset>,
        document: &ParticleEffectDocumentAsset,
        manual_count: u32,
    ) -> bool {
        if document.roots.is_empty() {
            error!(document = ?source.id(), "particle effect document has no root emitter");
            self.commands.entity(entity).try_despawn();
            return false;
        }
        let mut previous = self.projected_children(entity);
        let mut ordered = Vec::with_capacity(document.roots.len());
        for &id in &document.roots {
            let Some(emitter) = document.prepared_emitters.get(&id) else {
                continue;
            };
            let role = AuthoredParticleChild::Root(id);
            let child = self.take_or_spawn_child(entity, None, role, &mut previous);
            ordered.push(child);
            self.project_emitter(
                child,
                source,
                document,
                id,
                emitter,
                &emitter.effect,
                manual_count,
            );
        }
        self.reconcile_child_order(entity, ordered, previous);
        true
    }

    fn project_root_emitter(
        &mut self,
        entity: Entity,
        source: &Handle<ParticleEffectDocumentAsset>,
        document: &ParticleEffectDocumentAsset,
        id: AssetId,
        wave: bool,
        manual_count: u32,
    ) -> bool {
        let Some(emitter) = document.prepared_emitters.get(&id) else {
            error!(?id, document = ?source.id(), "particle emitter is absent from its document");
            self.commands.entity(entity).try_despawn();
            return false;
        };
        let effect = if wave {
            let Some(effect) = &emitter.wave_effect else {
                error!(emitter = ?id, "particle emitter does not support a wave surface");
                self.commands.entity(entity).try_despawn();
                return false;
            };
            effect
        } else {
            &emitter.effect
        };
        self.project_emitter(entity, source, document, id, emitter, effect, manual_count);
        true
    }

    fn project_emitter(
        &mut self,
        entity: Entity,
        source: &Handle<ParticleEffectDocumentAsset>,
        document: &ParticleEffectDocumentAsset,
        id: AssetId,
        emitter: &PreparedParticleEmitter,
        effect: &Handle<EffectAsset>,
        manual_count: u32,
    ) {
        let mut previous = self.projected_children(entity);
        let mut ordered = Vec::with_capacity(emitter.collision_children.len());
        for collision in &emitter.collision_children {
            let Some(child_emitter) = document.prepared_emitters.get(&collision.emitter) else {
                continue;
            };
            let role = AuthoredParticleChild::Collision {
                emitter: collision.emitter,
                occurrence: collision.occurrence,
            };
            let child = self.take_or_spawn_child(entity, Some(entity), role, &mut previous);
            ordered.push(child);
            self.project_emitter(
                child,
                source,
                document,
                collision.emitter,
                child_emitter,
                &collision.effect,
                0,
            );
        }
        self.reconcile_child_order(entity, ordered, previous);

        let prototype = emitter
            .prototype_mesh
            .as_ref()
            .and_then(|handle| self.prototype_meshes.get(handle))
            .and_then(|mesh| mesh.primitives.first())
            .map(|primitive| EffectMesh(primitive.mesh.clone()));
        let mut target = self.commands.entity(entity);
        target.remove::<ParticleEffectGraphReady>();
        if let Ok((mut live, mut particle, spawner, draining, mesh, uses_plane, explicit)) =
            self.live.get_mut(entity)
        {
            // Force a refresh of the draw configuration when the winning document changes.
            live.simulation_clock = emitter.clock;
            particle.handle = effect.clone();
            if let (Some(mut spawner), Some(effect)) = (spawner, self.effects.get(effect)) {
                spawner.settings = effect.spawner;
            }
            if emitter.uses_plane != uses_plane {
                if emitter.uses_plane {
                    target.insert(UsesParticleEffectPlane);
                } else {
                    target.remove::<UsesParticleEffectPlane>();
                }
            }
            if let Some(mut retirement) = draining {
                if explicit {
                    retirement.set_if_neq(EffectRetirementReadback::WaitingForParticles);
                } else {
                    target.remove::<(EffectRetirementReadback, bevy_hanabi::ParticleAliveCount)>();
                }
            }
            if !explicit {
                match (prototype, mesh) {
                    (Some(prototype), Some(mut mesh)) => {
                        mesh.set_if_neq(prototype);
                    }
                    (Some(prototype), None) => {
                        target.insert(prototype);
                    }
                    (None, Some(_)) => {
                        target.remove::<EffectMesh>();
                    }
                    (None, None) => {}
                }
            }
        } else {
            target.insert((
                ParticleEffect::new(effect.clone()),
                LiveParticleEffectInstance {
                    effect_asset: source.clone(),
                    emitter_id: id,
                    simulation_clock: emitter.clock,
                },
                EffectDeltaTime(0.0),
                ControlledEffectEmissionState {
                    elapsed_seconds: 0.0,
                    fractional_particle_remainder: 0.0,
                    manual_particle_count: manual_count,
                },
            ));
            if let Some(effect) = self.effects.get(effect) {
                target.insert(EffectSpawner::new(&effect.spawner));
            }
            if emitter.uses_plane {
                target.insert(UsesParticleEffectPlane);
            }
            if let Some(prototype) = prototype {
                target.insert(prototype);
            }
        }
    }

    fn projected_children(&self, parent: Entity) -> Vec<(Entity, AuthoredParticleChild)> {
        self.children
            .get(parent)
            .ok()
            .into_iter()
            .flat_map(|children| children.iter())
            .filter_map(|child| self.child_roles.get(child).ok().map(|role| (child, *role)))
            .collect()
    }

    fn take_or_spawn_child(
        &mut self,
        parent: Entity,
        effect_parent: Option<Entity>,
        role: AuthoredParticleChild,
        previous: &mut Vec<(Entity, AuthoredParticleChild)>,
    ) -> Entity {
        if let Some(index) = previous.iter().position(|&(_, existing)| existing == role) {
            return previous.swap_remove(index).0;
        }
        let mut child = self
            .commands
            .spawn((ChildOf(parent), role, Visibility::Inherited));
        if let Some(effect_parent) = effect_parent {
            child.insert(EffectParent::new(effect_parent));
        }
        child.id()
    }

    fn reconcile_child_order(
        &mut self,
        parent: Entity,
        mut ordered: Vec<Entity>,
        obsolete: Vec<(Entity, AuthoredParticleChild)>,
    ) {
        for (child, _) in obsolete {
            self.commands.entity(child).try_despawn();
        }
        let current = self.children.get(parent).ok();
        ordered.extend(
            current
                .into_iter()
                .flat_map(|children| children.iter())
                .filter(|&child| !self.child_roles.contains(child)),
        );
        if !current
            .into_iter()
            .flat_map(|children| children.iter())
            .eq(ordered.iter().copied())
        {
            self.commands.entity(parent).replace_children(&ordered);
        }
    }
}
