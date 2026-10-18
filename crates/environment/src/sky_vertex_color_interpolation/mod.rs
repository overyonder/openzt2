use bevy::{
    gltf::{Gltf, GltfMesh, GltfNode},
    mesh::VertexAttributeValues,
    prelude::*,
};
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::world_definitions::environment::EnvironmentVisualFlags;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_zoo_state::environment::environment_state_types::WorldEnvironment;

use super::{
    daylight_curve_sampling::{
        encode_daylight_fraction_as_u16, select_cyclic_daylight_curve_sample,
    },
    environment_presentation_types::{EnvironmentVisualOwner, EnvironmentVisualSampleIndex},
    environment_state_types::Daylight,
};

#[derive(Component)]
pub(super) struct SkyVertexColorSources(Vec<SkyVertexColorSource>);

struct SkyVertexColorSource {
    sample_index: usize,
    model: Handle<Gltf>,
    node_name: String,
}

#[derive(Component)]
pub(super) struct InterpolatedSkyMesh {
    mesh: Handle<Mesh>,
    sampled_fraction: f32,
}

pub(super) fn retain_sky_vertex_color_keyframe_models(
    mut commands: Commands,
    server: Res<AssetServer>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    environments: Query<&WorldEnvironment>,
    visuals: Query<
        (
            Entity,
            &EnvironmentVisualSampleIndex,
            &EnvironmentVisualOwner,
        ),
        (With<PrefabPresentation>, Without<SkyVertexColorSources>),
    >,
) {
    let Some(definitions) = active.get(&definitions) else {
        return;
    };
    for (entity, index, owner) in &visuals {
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
        if !sample
            .flags
            .contains_all(EnvironmentVisualFlags::INTERPOLATE_VERTEX_COLORS)
        {
            continue;
        }
        let group = definition
            .visual_samples
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                candidate.layer == sample.layer
                    && candidate.order == sample.order
                    && candidate.kind == sample.kind
            });
        if !group
            .clone()
            .next()
            .is_some_and(|(first, _)| first == index.0 as usize)
        {
            continue;
        }
        let sources: Option<Vec<_>> = group
            .map(|(sample_index, sample)| {
                let prefab_handle = definitions.scene(sample.asset)?;
                let prefab = prefabs.get(&prefab_handle)?;
                let renderable = prefab
                    .canonical_scene_prefab_document()
                    .entities
                    .iter()
                    .flat_map(|entity| &entity.renderables)
                    .next()?;
                Some(SkyVertexColorSource {
                    sample_index,
                    model: server.load(prefab.model_asset_path(renderable.model)?.to_owned()),
                    node_name: renderable.scene_name.clone(),
                })
            })
            .collect();
        if let Some(sources) = sources {
            commands
                .entity(entity)
                .insert(SkyVertexColorSources(sources));
        }
    }
}

pub(super) fn interpolate_sky_model_vertex_colors(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    environments: Query<(&WorldEnvironment, &Daylight)>,
    visuals: Query<(Entity, &EnvironmentVisualOwner, &SkyVertexColorSources)>,
    children: Query<&Children>,
    mut primitives: Query<(&mut Mesh3d, Option<&mut InterpolatedSkyMesh>)>,
    models: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(definitions) = active.get(&definitions) else {
        return;
    };
    for (root, owner, sources) in &visuals {
        let Ok((environment, daylight)) = environments.get(owner.0) else {
            continue;
        };
        let Some(definition) = definitions.find_environment(environment.definition) else {
            continue;
        };
        let Some(entity) = std::iter::once(root)
            .chain(children.iter_descendants_depth_first::<Children>(root))
            .find(|entity| primitives.contains(*entity))
        else {
            continue;
        };
        let Ok((mut primitive, mut projected)) = primitives.get_mut(entity) else {
            continue;
        };
        if projected.as_ref().is_some_and(|projected| {
            projected.mesh == primitive.0
                && (projected.sampled_fraction - daylight.fraction).abs() <= 0.001
        }) {
            continue;
        }
        let Some(curve) =
            select_cyclic_daylight_curve_sample(daylight.fraction, sources.0.len(), |index| {
                encode_daylight_fraction_as_u16(
                    definition.visual_samples[sources.0[index].sample_index].day_fraction,
                )
            })
        else {
            continue;
        };
        let source_mesh = |source: &SkyVertexColorSource| -> Option<&Mesh> {
            let model = models.get(&source.model)?;
            let node = model
                .named_nodes
                .get(source.node_name.as_str())
                .or_else(|| model.nodes.first())
                .and_then(|node| nodes.get(node))?;
            let mesh = gltf_meshes.get(node.mesh.as_ref()?)?.primitives.first()?;
            meshes.get(&mesh.mesh)
        };
        let colors = source_mesh(&sources.0[curve.left_keyframe_index])
            .zip(source_mesh(&sources.0[curve.right_keyframe_index]))
            .and_then(|(left, right)| {
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
        let Some(mesh) = meshes.get(&primitive.0) else {
            continue;
        };
        if mesh.count_vertices() != colors.len() {
            continue;
        }
        let output = if let Some(projected) = projected
            .as_mut()
            .filter(|projected| projected.mesh == primitive.0)
        {
            projected.sampled_fraction = daylight.fraction;
            primitive.0.clone()
        } else {
            let mesh = mesh.clone();
            let output = meshes.add(mesh);
            primitive.0 = output.clone();
            commands.entity(entity).insert(InterpolatedSkyMesh {
                mesh: output.clone(),
                sampled_fraction: daylight.fraction,
            });
            output
        };
        if let Some(mut mesh) = meshes.get_mut(&output) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        }
    }
}
