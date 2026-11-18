use bevy::app::{AnimationSystems, HierarchyPropagatePlugin, Propagate, PropagateSet};
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use openzt2_world_state::model_render::ModelRenderPresentationSet;
use openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabBillboardProjection;

use super::authored_model_material_pass_projection::project_authored_model_material_passes;
use super::model_scene_visibility_and_shadow_projection::project_model_scene_visibility_and_shadow_policy_onto_late_primitives;
use super::prefab_fixed_function_world_lighting_projection::project_prefab_fixed_function_world_lighting_policies;
use super::world_entity_model_suppression::project_world_entity_model_suppression;
use super::world_entity_model_suppression::SuppressedModelRenderLayers;

#[cfg(test)]
mod tests;

#[derive(Component, Clone, PartialEq)]
pub(super) struct PrefabCameraRenderLayers(pub(super) RenderLayers);

pub(super) fn register_inherited_render_layers(app: &mut App) {
    app.add_plugins(HierarchyPropagatePlugin::<PrefabCameraRenderLayers>::new(
        PostUpdate,
    ))
    .configure_sets(
        PostUpdate,
        PropagateSet::<PrefabCameraRenderLayers>::default()
            .before(PrefabBillboardProjection)
            .before(ModelRenderPresentationSet::LevelOfDetail)
            .before(project_prefab_fixed_function_world_lighting_policies),
    )
    .add_systems(
        PostUpdate,
        project_prefab_camera_layer_sources
            .after(AnimationSystems)
            .after(project_model_scene_visibility_and_shadow_policy_onto_late_primitives)
            .after(project_authored_model_material_passes)
            .after(project_world_entity_model_suppression)
            .before(PropagateSet::<PrefabCameraRenderLayers>::default()),
    );
}

fn project_prefab_camera_layer_sources(
    mut commands: Commands,
    changed: Query<Entity, Or<(Changed<RenderLayers>, Changed<SuppressedModelRenderLayers>)>>,
    mut removed_layers: RemovedComponents<RenderLayers>,
    mut removed_suppression: RemovedComponents<SuppressedModelRenderLayers>,
    sources: Query<(
        Option<&RenderLayers>,
        Option<&SuppressedModelRenderLayers>,
        Option<&Propagate<PrefabCameraRenderLayers>>,
    )>,
) {
    for entity in changed
        .iter()
        .chain(removed_layers.read())
        .chain(removed_suppression.read())
    {
        let Ok((layers, saved, propagation)) = sources.get(entity) else {
            continue;
        };
        // Suppressed draws still use their ordinary camera policy. A saved
        // absence of layers must continue to inherit a parent's policy.
        let layers = saved.map_or(layers, |saved| saved.0.as_ref());
        if layers == propagation.map(|Propagate(PrefabCameraRenderLayers(layers))| layers) {
            continue;
        }
        let mut target = commands.entity(entity);
        if let Some(layers) = layers {
            target.insert(Propagate(PrefabCameraRenderLayers(layers.clone())));
        } else {
            target.remove::<Propagate<PrefabCameraRenderLayers>>();
        }
    }
}
