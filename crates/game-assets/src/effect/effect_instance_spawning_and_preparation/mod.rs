//! Projects loaded particle graphs onto Bevy and Hanabi entities.

use std::collections::{HashMap, HashSet};

use bevy::asset::{
    AssetId as BevyAssetId, AssetLoadFailedEvent, LoadState, RecursiveDependencyLoadState,
    RenderAssetUsages, VisitAssetDependencies,
};
use bevy::diagnostic::FrameCount;
use bevy::ecs::system::SystemParam;
use bevy::gltf::GltfMesh;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy_hanabi::{
    EffectAsset, EffectChildren, EffectDeltaTime, EffectEventCapacity, EffectMesh, EffectParent,
    EffectSpawnBudgetGroup, EffectSpawner, ExternalEffectSpawner, ParticleEffect,
};
use openzt2_game_data::AssetId;

use super::{
    effect_runtime_types::{
        ControlledEffectEmissionSettings, ControlledEffectEmissionState, EffectRetirementReadback,
        ParticleEffectDocumentInstance, ParticleEffectGraphReady, PendingEffectInstance,
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
    frame: Res<'w, FrameCount>,
    prototype_meshes: Res<'w, Assets<GltfMesh>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    combined_prototypes:
        Local<'s, HashMap<(BevyAssetId<ParticleEffectDocumentAsset>, AssetId), Handle<Mesh>>>,
    effects: Res<'w, Assets<EffectAsset>>,
    children: Query<'w, 's, &'static Children>,
    collision_children: Query<'w, 's, &'static EffectChildren>,
    child_roles: Query<'w, 's, &'static AuthoredParticleChild>,
    event_capacities: Query<'w, 's, &'static mut EffectEventCapacity>,
    spawn_groups: Query<'w, 's, &'static mut EffectSpawnBudgetGroup>,
    live: Query<
        'w,
        's,
        (
            &'static LiveParticleEffectInstance,
            &'static mut ParticleEffect,
            &'static mut ControlledEffectEmissionSettings,
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
    pending: Query<(
        Entity,
        Ref<PendingEffectInstance>,
        Has<ParticleEffectWaveSurface>,
    )>,
    newly_pending: Query<Entity, Changed<PendingEffectInstance>>,
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
    let mut discarded_prototypes = HashSet::new();
    for event in events.read() {
        match *event {
            AssetEvent::Added { id } | AssetEvent::LoadedWithDependencies { id } => {
                changed.insert(id);
            }
            AssetEvent::Modified { id } => {
                changed.insert(id);
                discarded_prototypes.insert(id);
            }
            AssetEvent::Removed { id } => {
                removed.insert(id);
                discarded_prototypes.insert(id);
            }
            AssetEvent::Unused { id } => {
                discarded_prototypes.insert(id);
            }
        }
    }
    let failed = failures
        .read()
        .map(|event| event.id)
        .collect::<HashSet<BevyAssetId<_>>>();
    let failed_dependencies: HashSet<_> = material_failures
        .read()
        .map(|event| event.id.untyped())
        .chain(mesh_failures.read().map(|event| event.id.untyped()))
        .collect();
    let failed_documents: HashSet<_> = if failed_dependencies.is_empty() {
        HashSet::new()
    } else {
        documents
            .iter()
            .filter_map(|(id, document)| {
                let mut failed = false;
                document.visit_dependencies(&mut |dependency| {
                    failed |= failed_dependencies.contains(&dependency);
                });
                failed.then_some(id)
            })
            .collect()
    };
    let reloaded: HashSet<_> = changed
        .iter()
        .copied()
        .filter(|&id| documents.contains(id) && asset_server.is_loaded_with_dependencies(id))
        .collect();
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
    if !discarded_prototypes.is_empty() {
        projection
            .combined_prototypes
            .retain(|(document, _), _| !discarded_prototypes.contains(document));
    }
    let selected_pending: Vec<_> = if changed.is_empty()
        && removed.is_empty()
        && failed.is_empty()
        && failed_documents.is_empty()
    {
        newly_pending.iter().collect()
    } else {
        pending
            .iter()
            .filter_map(|(entity, request, _)| {
                let id = request.effect_asset.id();
                (request.is_changed()
                    || changed.contains(&id)
                    || removed.contains(&id)
                    || failed.contains(&id)
                    || failed_documents.contains(&id))
                .then_some(entity)
            })
            .collect()
    };
    for (entity, request, wave) in pending.iter_many(selected_pending).matched() {
        let id = request.effect_asset.id();
        if failed.contains(&id) || removed.contains(&id) {
            projection.commands.entity(entity).try_despawn();
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
            let child = self.take_or_spawn_child(entity, role, &mut previous);
            ordered.push(child);
            self.project_emitter(
                child,
                source,
                document,
                id,
                emitter,
                &emitter.effect,
                manual_count,
                child,
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
        self.project_emitter(
            entity,
            source,
            document,
            id,
            emitter,
            effect,
            manual_count,
            entity,
        );
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
        group_owner: Entity,
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
            let child = self.take_or_spawn_child(entity, role, &mut previous);
            let capacity = EffectEventCapacity(collision.event_capacity);
            if let Ok(mut current) = self.event_capacities.get_mut(child) {
                current.set_if_neq(capacity);
            } else {
                self.commands.entity(child).insert(capacity);
            }
            ordered.push(child);
            self.project_emitter(
                child,
                source,
                document,
                collision.emitter,
                child_emitter,
                &collision.effect,
                0,
                group_owner,
            );
        }
        if !self
            .collision_children
            .get(entity)
            .ok()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .eq(ordered.iter().copied())
        {
            self.commands
                .entity(entity)
                .replace_related::<EffectParent>(&ordered);
        }
        self.reconcile_child_order(entity, ordered, previous);

        match (
            emitter.shared_spawn_capacity,
            self.spawn_groups.get_mut(entity),
        ) {
            (Some(capacity), Ok(mut group)) => {
                group.set_if_neq(EffectSpawnBudgetGroup {
                    owner: group_owner,
                    capacity,
                });
            }
            (Some(capacity), Err(_)) => {
                self.commands.entity(entity).insert(EffectSpawnBudgetGroup {
                    owner: group_owner,
                    capacity,
                });
            }
            (None, Ok(_)) => {
                self.commands
                    .entity(entity)
                    .remove::<EffectSpawnBudgetGroup>();
            }
            (None, Err(_)) => {}
        }

        let prototype =
            self.prepare_prototype_mesh(source.id(), id, emitter.prototype_mesh.as_ref());
        let mut target = self.commands.entity(entity);
        target.remove::<ParticleEffectGraphReady>();
        if let Ok((_, mut particle, mut settings, spawner, draining, mesh, uses_plane, explicit)) =
            self.live.get_mut(entity)
        {
            settings.set_if_neq(emitter.emission_settings.clone());
            if particle.handle != *effect {
                particle.handle = effect.clone();
            }
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
                    retirement.set_if_neq(EffectRetirementReadback::WaitingForParticles(
                        self.frame.0.wrapping_add(1),
                    ));
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
                },
                EffectDeltaTime(0.0),
                ExternalEffectSpawner,
                emitter.emission_settings.clone(),
                ControlledEffectEmissionState {
                    elapsed: default(),
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

    fn prepare_prototype_mesh(
        &mut self,
        document: BevyAssetId<ParticleEffectDocumentAsset>,
        emitter: AssetId,
        prototype: Option<&Handle<GltfMesh>>,
    ) -> Option<EffectMesh> {
        let prototype = self.prototype_meshes.get(prototype?)?;
        if let [primitive] = prototype.primitives.as_slice() {
            return Some(EffectMesh(primitive.mesh.clone()));
        }
        let key = (document, emitter);
        if let Some(handle) = self.combined_prototypes.get(&key) {
            return Some(EffectMesh(handle.clone()));
        }
        let mesh = combine_particle_prototype_primitives(
            prototype
                .primitives
                .iter()
                .map(|primitive| self.meshes.get(&primitive.mesh)),
        )?;
        let handle = self.meshes.add(mesh);
        self.combined_prototypes.insert(key, handle.clone());
        Some(EffectMesh(handle))
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
        role: AuthoredParticleChild,
        previous: &mut Vec<(Entity, AuthoredParticleChild)>,
    ) -> Entity {
        if let Some(index) = previous.iter().position(|&(_, existing)| existing == role) {
            return previous.swap_remove(index).0;
        }
        self.commands
            .spawn((ChildOf(parent), role, Visibility::Inherited))
            .id()
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

fn combine_particle_prototype_primitives<'a>(
    primitives: impl Iterator<Item = Option<&'a Mesh>>,
) -> Option<Mesh> {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for primitive in primitives {
        let primitive = primitive?;
        if primitive.primitive_topology() != PrimitiveTopology::TriangleList {
            return None;
        }
        let VertexAttributeValues::Float32x3(primitive_positions) =
            primitive.attribute(Mesh::ATTRIBUTE_POSITION)?
        else {
            return None;
        };
        let offset = positions.len();
        if let Some(primitive_indices) = primitive.indices() {
            for index in primitive_indices.iter() {
                indices.push(u32::try_from(offset.checked_add(index)?).ok()?);
            }
        } else {
            let start = u32::try_from(offset).ok()?;
            let end = u32::try_from(offset.checked_add(primitive_positions.len())?).ok()?;
            indices.extend(start..end);
        }
        positions.extend_from_slice(primitive_positions);
        if let Some(VertexAttributeValues::Float32x3(primitive_normals)) =
            primitive.attribute(Mesh::ATTRIBUTE_NORMAL)
        {
            normals.extend_from_slice(primitive_normals);
        } else {
            normals.resize(positions.len(), [0.0, 0.0, 1.0]);
        }
        if let Some(VertexAttributeValues::Float32x2(primitive_uvs)) =
            primitive.attribute(Mesh::ATTRIBUTE_UV_0)
        {
            uvs.extend_from_slice(primitive_uvs);
        } else {
            uvs.resize(positions.len(), [0.0, 0.0]);
        }
    }
    (!indices.is_empty()).then(|| {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
    })
}
