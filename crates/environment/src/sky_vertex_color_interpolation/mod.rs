use bevy::platform::collections::HashSet;
use bevy::{
    ecs::relationship::RelationshipTarget,
    gltf::{Gltf, GltfMesh, GltfNode},
    mesh::VertexAttributeValues,
    prelude::*,
};
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_zoo_state::environment::environment_state_types::WorldEnvironment;

use super::{
    daylight_curve_sampling::{
        encode_daylight_fraction_as_u16, select_cyclic_daylight_curve_sample,
    },
    environment_presentation_types::{EnvironmentVisualOwner, EnvironmentVisualSampleIndex},
    environment_state_types::Daylight,
};

#[derive(Component, PartialEq)]
pub(super) struct SkyVertexColorSources(Vec<SkyVertexColorSource>);

#[derive(Component)]
pub(super) struct SkyVertexColorGroup;

#[derive(Component)]
#[relationship(relationship_target = SkyVertexColorPrimitives)]
pub(super) struct SkyVertexColorPrimitiveOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = SkyVertexColorPrimitiveOf)]
pub(super) struct SkyVertexColorPrimitives(Vec<Entity>);

#[derive(PartialEq)]
struct SkyVertexColorSource {
    daylight_fraction: u16,
    model: Handle<Gltf>,
    node_name: String,
}

#[derive(Component, PartialEq)]
pub(super) struct SkyVertexColorMeshes(Vec<SkyVertexColorMesh>);

#[derive(Default, PartialEq)]
struct SkyVertexColorMesh {
    node: Option<AssetId<GltfNode>>,
    gltf_mesh: Option<AssetId<GltfMesh>>,
    mesh: Option<Handle<Mesh>>,
}

#[derive(Component)]
pub(super) struct InterpolatedSkyMesh {
    mesh: Handle<Mesh>,
    base_mesh: Handle<Mesh>,
    sampled_fraction: f32,
}

fn changed_asset_ids<A: Asset>(events: &mut MessageReader<AssetEvent<A>>) -> HashSet<AssetId<A>> {
    events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id }
            | AssetEvent::Removed { id } => Some(id),
            AssetEvent::Unused { .. } => None,
        })
        .collect()
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn retain_sky_vertex_color_keyframe_models(
    mut commands: Commands,
    server: Res<AssetServer>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut prefab_events: MessageReader<AssetEvent<ScenePrefabAsset>>,
    environments: Query<&WorldEnvironment>,
    mut visuals: Query<(
        Entity,
        &EnvironmentVisualSampleIndex,
        &EnvironmentVisualOwner,
        Ref<SkyVertexColorGroup>,
        Option<&mut SkyVertexColorSources>,
    )>,
) {
    let changed_prefabs = changed_asset_ids(&mut prefab_events);
    let definitions_changed = active.is_changed() || definitions.is_changed();
    let Some(definitions) = active.get(&definitions) else {
        return;
    };
    for (entity, index, owner, group_added, current) in &mut visuals {
        if !group_added.is_added() && changed_prefabs.is_empty() && !definitions_changed {
            continue;
        }
        let Some(definition) = environments
            .get(owner.0)
            .ok()
            .and_then(|environment| definitions.find_environment(environment.definition))
        else {
            continue;
        };
        let Some(sample) = definition.visual_samples.get(index.0 as usize) else {
            continue;
        };
        let group = definition.visual_samples.iter().filter(|candidate| {
            candidate.layer == sample.layer
                && candidate.order == sample.order
                && candidate.kind == sample.kind
        });
        if !group_added.is_added()
            && !definitions_changed
            && !group.clone().any(|sample| {
                definitions
                    .scene(sample.asset)
                    .is_some_and(|prefab| changed_prefabs.contains(&prefab.id()))
            })
        {
            continue;
        }
        let sources: Option<Vec<_>> = group
            .map(|sample| {
                let prefab_handle = definitions.scene(sample.asset)?;
                let prefab = prefabs.get(&prefab_handle)?;
                let renderable = prefab
                    .canonical_scene_prefab_document()
                    .entities
                    .iter()
                    .flat_map(|entity| &entity.renderables)
                    .next()?;
                Some(SkyVertexColorSource {
                    daylight_fraction: encode_daylight_fraction_as_u16(sample.day_fraction),
                    model: server.load(prefab.model_asset_path(renderable.model)?.to_owned()),
                    node_name: renderable.scene_name.clone(),
                })
            })
            .collect();
        match (current, sources) {
            (Some(mut current), Some(sources)) => {
                current.set_if_neq(SkyVertexColorSources(sources));
            }
            (None, Some(sources)) => {
                commands
                    .entity(entity)
                    .insert(SkyVertexColorSources(sources));
            }
            (Some(_), None) => {
                commands
                    .entity(entity)
                    .remove::<(SkyVertexColorSources, SkyVertexColorMeshes)>();
            }
            (None, None) => {}
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn resolve_sky_vertex_color_meshes(
    mut commands: Commands,
    models: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut model_events: MessageReader<AssetEvent<Gltf>>,
    mut node_events: MessageReader<AssetEvent<GltfNode>>,
    mut gltf_mesh_events: MessageReader<AssetEvent<GltfMesh>>,
    mut visuals: Query<(
        Entity,
        Ref<SkyVertexColorSources>,
        Option<&mut SkyVertexColorMeshes>,
    )>,
) {
    let changed_models = changed_asset_ids(&mut model_events);
    let changed_nodes = changed_asset_ids(&mut node_events);
    let changed_gltf_meshes = changed_asset_ids(&mut gltf_mesh_events);
    let resolve = |source: &SkyVertexColorSource| {
        let mut resolved = SkyVertexColorMesh::default();
        if let Some(model) = models.get(&source.model) {
            resolved.node = model
                .named_nodes
                .get(source.node_name.as_str())
                .or_else(|| model.nodes.first())
                .map(Handle::id);
        }
        resolved.gltf_mesh = resolved
            .node
            .and_then(|id| nodes.get(id))
            .and_then(|node| node.mesh.as_ref())
            .map(Handle::id);
        resolved.mesh = resolved
            .gltf_mesh
            .and_then(|id| gltf_meshes.get(id))
            .and_then(|mesh| mesh.primitives.first())
            .map(|primitive| primitive.mesh.clone());
        resolved
    };
    for (entity, sources, current) in &mut visuals {
        let Some(mut current) = current else {
            commands.entity(entity).insert(SkyVertexColorMeshes(
                sources.0.iter().map(resolve).collect(),
            ));
            continue;
        };
        if sources.is_changed() {
            current.set_if_neq(SkyVertexColorMeshes(
                sources.0.iter().map(resolve).collect(),
            ));
            continue;
        }
        for (index, source) in sources.0.iter().enumerate() {
            let dependency = &current.0[index];
            if changed_models.contains(&source.model.id())
                || dependency
                    .node
                    .is_some_and(|id| changed_nodes.contains(&id))
                || dependency
                    .gltf_mesh
                    .is_some_and(|id| changed_gltf_meshes.contains(&id))
            {
                let resolved = resolve(source);
                if *dependency != resolved {
                    current.0[index] = resolved;
                }
            }
        }
    }
}

#[derive(Message)]
pub(super) struct SkyPrimitiveTreeChanged(Entity);

pub(super) fn invalidate_discarded_sky_tree_parent(
    discard: On<Discard<ChildOf>>,
    parents: Query<&ChildOf>,
    groups: Query<(), With<SkyVertexColorGroup>>,
    mut changed: MessageWriter<SkyPrimitiveTreeChanged>,
) {
    if let Some(group) = parents
        .iter_ancestors::<ChildOf>(discard.entity)
        .find(|entity| groups.contains(*entity))
    {
        changed.write(SkyPrimitiveTreeChanged(group));
    }
}

pub(super) fn link_first_sky_vertex_color_primitive(
    mut commands: Commands,
    visuals: Query<Option<&SkyVertexColorPrimitives>, With<SkyVertexColorGroup>>,
    added_groups: Query<Entity, Added<SkyVertexColorGroup>>,
    changed_hierarchy: Query<Entity, Or<(Added<Mesh3d>, Changed<ChildOf>)>>,
    parents: Query<&ChildOf>,
    mut tree_changes: MessageReader<SkyPrimitiveTreeChanged>,
    children: Query<&Children>,
    primitives: Query<(), With<Mesh3d>>,
) {
    let mut affected: bevy::ecs::entity::EntityHashSet = added_groups
        .iter()
        .chain(tree_changes.read().map(|change| change.0))
        .collect();
    for entity in &changed_hierarchy {
        if let Some(group) = std::iter::once(entity)
            .chain(parents.iter_ancestors::<ChildOf>(entity))
            .find(|entity| visuals.contains(*entity))
        {
            affected.insert(group);
        }
    }
    for group in affected {
        let Ok(previous) = visuals.get(group) else {
            continue;
        };
        let previous = previous.and_then(|primitives| primitives.iter().next());
        let next = std::iter::once(group)
            .chain(children.iter_descendants_depth_first::<Children>(group))
            .find(|entity| primitives.contains(*entity));
        if next != previous {
            if let Some(previous) = previous {
                commands
                    .entity(previous)
                    .try_remove::<SkyVertexColorPrimitiveOf>();
            }
            if let Some(next) = next {
                commands
                    .entity(next)
                    .try_insert(SkyVertexColorPrimitiveOf(group));
            }
        }
    }
}

pub(super) fn unlink_removed_sky_vertex_color_mesh(
    removed: On<Remove<Mesh3d>>,
    primitives: Query<&SkyVertexColorPrimitiveOf>,
    mut changed: MessageWriter<SkyPrimitiveTreeChanged>,
    mut commands: Commands,
) {
    if let Ok(link) = primitives.get(removed.entity) {
        changed.write(SkyPrimitiveTreeChanged(link.0));
        commands
            .entity(removed.entity)
            .try_remove::<SkyVertexColorPrimitiveOf>();
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn interpolate_sky_model_vertex_colors(
    mut commands: Commands,
    environments: Query<&Daylight>,
    visuals: Query<(
        &SkyVertexColorPrimitives,
        &EnvironmentVisualOwner,
        Ref<SkyVertexColorSources>,
        Ref<SkyVertexColorMeshes>,
    )>,
    mut primitives: Query<(&mut Mesh3d, Option<&mut InterpolatedSkyMesh>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mesh_events: MessageReader<AssetEvent<Mesh>>,
) {
    let changed_meshes = changed_asset_ids(&mut mesh_events);
    for (primitives_of_visual, owner, sources, resolved) in &visuals {
        let Ok(daylight) = environments.get(owner.0) else {
            continue;
        };
        let Some(entity) = primitives_of_visual.iter().next() else {
            continue;
        };
        let Ok((mut primitive, mut projected)) = primitives.get_mut(entity) else {
            continue;
        };
        let fraction_already_sampled = projected.as_ref().is_some_and(|projected| {
            projected.mesh == primitive.0
                && (projected.sampled_fraction - daylight.fraction).abs() <= 0.001
        });
        if fraction_already_sampled
            && !sources.is_changed()
            && !resolved.is_changed()
            && changed_meshes.is_empty()
        {
            continue;
        }
        let Some(curve) =
            select_cyclic_daylight_curve_sample(daylight.fraction, sources.0.len(), |index| {
                sources.0[index].daylight_fraction
            })
        else {
            continue;
        };
        let source_mesh = |index: usize| {
            let handle = resolved.0.get(index)?.mesh.as_ref()?;
            Some((meshes.get(handle)?, handle.id()))
        };
        let base_mesh = projected
            .as_ref()
            .filter(|projected| projected.mesh == primitive.0)
            .map_or_else(
                || primitive.0.clone(),
                |projected| projected.base_mesh.clone(),
            );
        let base_changed = changed_meshes.contains(&base_mesh.id());
        let colors = source_mesh(curve.left_keyframe_index)
            .zip(source_mesh(curve.right_keyframe_index))
            .and_then(|((left, left_id), (right, right_id))| {
                if fraction_already_sampled
                    && !sources.is_changed()
                    && !resolved.is_changed()
                    && !base_changed
                    && !changed_meshes.contains(&left_id)
                    && !changed_meshes.contains(&right_id)
                {
                    return None;
                }
                let (
                    Some(VertexAttributeValues::Float32x4(left)),
                    Some(VertexAttributeValues::Float32x4(right)),
                ) = (
                    left.attribute(Mesh::ATTRIBUTE_COLOR),
                    right.attribute(Mesh::ATTRIBUTE_COLOR),
                )
                else {
                    return None;
                };
                (left.len() == right.len()).then(|| {
                    left.iter()
                        .zip(right)
                        .map(|(left, right)| {
                            std::array::from_fn::<_, 4, _>(|channel| {
                                ((left[channel]
                                    + (right[channel] - left[channel])
                                        * curve.interpolation_fraction)
                                    * 255.0)
                                    .clamp(0.0, 255.0)
                                    .trunc()
                                    / 255.0
                            })
                        })
                        .collect::<Vec<_>>()
                })
            });
        let Some(colors) = colors else { continue };
        let Some(mesh) = meshes.get(&base_mesh) else {
            continue;
        };
        if mesh.count_vertices() != colors.len() {
            continue;
        }
        let output = if let Some(projected) = projected
            .as_mut()
            .filter(|projected| projected.mesh == primitive.0 && !base_changed)
        {
            projected.sampled_fraction = daylight.fraction;
            primitive.0.clone()
        } else {
            let mesh = mesh.clone();
            let output = meshes.add(mesh);
            primitive.0 = output.clone();
            let next = InterpolatedSkyMesh {
                mesh: output.clone(),
                base_mesh,
                sampled_fraction: daylight.fraction,
            };
            if let Some(mut current) = projected {
                *current = next;
            } else {
                commands.entity(entity).insert(next);
            }
            output
        };
        if let Some(mut mesh) = meshes.get_mut(&output) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        }
    }
}
