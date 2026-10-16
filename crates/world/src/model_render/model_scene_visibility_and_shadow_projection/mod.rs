use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::light::NotShadowReceiver;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use openzt2_world_state::model_render::ModelExpanded;

use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabShadowPolicy;

use super::authored_model_material_pass_projection::EffectPassMaterialRenderViewProjection;

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
        let layers = model_scene_render_layers(shadow, layers);
        for entity in std::iter::once(root)
            .chain(children.iter_descendants_depth_first::<Children>(root))
            .filter(|entity| meshes.contains(*entity))
        {
            apply_model_scene_policy(&mut commands, entity, &layers, shadow);
        }
        commands
            .entity(root)
            .insert(ModelSceneVisibilityAndShadowPolicyProjected);
    }
}

/// Scene instances can add or reparent primitives after their root was
/// projected (for example when a catalogue preview replaces one sharing the
/// same model). Give those primitives their root's layers and shadow policy,
/// or a preview camera on its own layer never sees them.
pub(super) fn project_model_scene_visibility_and_shadow_policy_onto_late_primitives(
    mut commands: Commands,
    // Per-view material projections (reflection, refraction) sit beneath
    // their source mesh but own their camera-specific layers.
    primitives: Query<
        (
            Entity,
            Option<&RenderLayers>,
            Has<NotShadowCaster>,
            Has<NotShadowReceiver>,
        ),
        (
            With<Mesh3d>,
            Without<EffectPassMaterialRenderViewProjection>,
            Or<(Added<Mesh3d>, Changed<ChildOf>)>,
        ),
    >,
    parents: Query<&ChildOf>,
    roots: Query<
        (&PrefabShadowPolicy, Option<&RenderLayers>),
        With<ModelSceneVisibilityAndShadowPolicyProjected>,
    >,
) {
    for (primitive, current_layers, not_caster, not_receiver) in &primitives {
        let Some((shadow, layers)) = std::iter::once(primitive)
            .chain(parents.iter_ancestors::<ChildOf>(primitive))
            .find_map(|entity| roots.get(entity).ok())
        else {
            continue;
        };
        let layers = model_scene_render_layers(shadow, layers);
        let mut target = commands.entity(primitive);
        // Re-inserting equal layers would invalidate projected materials.
        if current_layers != Some(&layers) {
            target.insert(layers);
        }
        if !shadow.casts && !not_caster {
            target.insert(NotShadowCaster);
        }
        if !shadow.receives && !not_receiver {
            target.insert(NotShadowReceiver);
        }
    }
}

fn model_scene_render_layers(
    shadow: &PrefabShadowPolicy,
    root_layers: Option<&RenderLayers>,
) -> RenderLayers {
    root_layers.cloned().unwrap_or_else(|| {
        if shadow.visible_in_reflections {
            RenderLayers::from_layers(&[0, 1])
        } else {
            RenderLayers::layer(0)
        }
    })
}

fn apply_model_scene_policy(
    commands: &mut Commands,
    primitive: Entity,
    layers: &RenderLayers,
    shadow: &PrefabShadowPolicy,
) {
    let mut target = commands.entity(primitive);
    target.insert(layers.clone());
    if !shadow.casts {
        target.insert(NotShadowCaster);
    }
    if !shadow.receives {
        target.insert(NotShadowReceiver);
    }
}
