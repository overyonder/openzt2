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
    mut sources: Query<(
        Option<&RenderLayers>,
        Option<&mut Propagate<PrefabCameraRenderLayers>>,
    )>,
) {
    for entity in changed.iter().chain(removed_layers.read()) {
        let Ok((layers, propagation)) = sources.get_mut(entity) else {
            continue;
        };
        match (layers, propagation) {
            (Some(layers), Some(propagation)) => {
                propagation
                    .map_unchanged(|propagation| &mut propagation.0 .0)
                    .set_if_neq(layers.clone());
            }
            (Some(layers), None) => {
                commands
                    .entity(entity)
                    .insert(Propagate(PrefabCameraRenderLayers(layers.clone())));
            }
            (None, Some(_)) => {
                commands
                    .entity(entity)
                    .remove::<Propagate<PrefabCameraRenderLayers>>();
            }
            (None, None) => {}
        }
    }
}
