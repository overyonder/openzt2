use bevy::app::{AnimationSystems, HierarchyPropagatePlugin, Propagate, PropagateSet};
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use openzt2_world_state::model_render::ModelRenderPresentationSet;
use openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabBillboardProjection;

use super::authored_model_material_pass_projection::project_authored_model_material_passes;

#[cfg(test)]
mod tests;

#[derive(Component, Clone, PartialEq)]
pub(super) struct PrefabCameraRenderLayers(pub(super) RenderLayers);

pub(super) fn register_inherited_render_layers(app: &mut App) {
    super::active_model_view_cameras::register_active_model_view_cameras(app);
    app.add_plugins(HierarchyPropagatePlugin::<PrefabCameraRenderLayers>::new(
        PostUpdate,
    ))
    .configure_sets(
        PostUpdate,
        PropagateSet::<PrefabCameraRenderLayers>::default()
            .before(PrefabBillboardProjection)
            .before(ModelRenderPresentationSet::LevelOfDetail),
    )
    .add_systems(
        PostUpdate,
        project_prefab_camera_layer_sources
            .after(AnimationSystems)
            .after(project_authored_model_material_passes)
            .before(PropagateSet::<PrefabCameraRenderLayers>::default()),
    );
}

fn project_prefab_camera_layer_sources(
    mut commands: Commands,
    changed: Query<Entity, Changed<RenderLayers>>,
    mut removed_layers: RemovedComponents<RenderLayers>,
    sources: Query<(
        Option<&RenderLayers>,
        Option<&Propagate<PrefabCameraRenderLayers>>,
    )>,
) {
    for entity in changed.iter().chain(removed_layers.read()) {
        let Ok((layers, propagation)) = sources.get(entity) else {
            continue;
        };
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
