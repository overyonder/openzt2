use bevy::{
    camera::visibility::RenderLayers,
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
    world_serialization::WorldInstanceReady,
};

use crate::plugins::world_spawn::prefab_presentation_types::{PrefabModel, PrefabShadowPolicy};

use super::ModelExpanded;

#[cfg(test)]
mod tests;

#[derive(Component)]
pub(super) struct ModelSceneVisibilityAndShadowPolicyProjected;

pub(super) fn invalidate_model_scene_visibility_and_shadow_policy_on_world_instance_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    models: Query<(), With<PrefabModel>>,
) {
    if models.contains(ready.event_target()) {
        // A rebuilt instance replaces the meshes beneath the surviving model.
        commands
            .entity(ready.event_target())
            .remove::<ModelSceneVisibilityAndShadowPolicyProjected>();
    }
}

pub(super) fn project_model_scene_visibility_and_shadow_policy(
    mut commands: Commands,
    roots: Query<
        (Entity, &PrefabShadowPolicy, Option<&RenderLayers>),
        (
            With<ModelExpanded>,
            Without<ModelSceneVisibilityAndShadowPolicyProjected>,
        ),
    >,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
) {
    for (root, shadow, layers) in &roots {
        if std::iter::once(root)
            .chain(children.iter_descendants_depth_first::<Children>(root))
            .all(|entity| !meshes.contains(entity))
        {
            continue;
        }
        let layers = layers.cloned().unwrap_or_else(|| {
            if shadow.visible_in_reflections {
                RenderLayers::from_layers(&[0, 1])
            } else {
                RenderLayers::layer(0)
            }
        });
        for entity in std::iter::once(root)
            .chain(children.iter_descendants_depth_first::<Children>(root))
            .filter(|entity| meshes.contains(*entity))
        {
            let mut target = commands.entity(entity);
            target.insert(layers.clone());
            if !shadow.casts {
                target.insert(NotShadowCaster);
            }
            if !shadow.receives {
                target.insert(NotShadowReceiver);
            }
        }
        commands
            .entity(root)
            .insert(ModelSceneVisibilityAndShadowPolicyProjected);
    }
}
