use bevy::{
    camera::visibility::RenderLayers,
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};

use crate::plugins::world_spawn::prefab_presentation_types::PrefabShadowPolicy;

use super::ModelExpanded;

#[derive(Component)]
pub(super) struct ModelSceneVisibilityAndShadowPolicyProjected;

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
    primitives: Query<
        (Entity, Option<&RenderLayers>),
        (With<Mesh3d>, Or<(Added<Mesh3d>, Changed<ChildOf>)>),
    >,
    parents: Query<&ChildOf>,
    roots: Query<
        (&PrefabShadowPolicy, Option<&RenderLayers>),
        With<ModelSceneVisibilityAndShadowPolicyProjected>,
    >,
) {
    for (primitive, current_layers) in &primitives {
        let Some((shadow, layers)) = std::iter::once(primitive)
            .chain(parents.iter_ancestors::<ChildOf>(primitive))
            .find_map(|entity| roots.get(entity).ok())
        else {
            continue;
        };
        let layers = model_scene_render_layers(shadow, layers);
        // Re-inserting equal layers would invalidate projected materials.
        if current_layers != Some(&layers) {
            apply_model_scene_policy(&mut commands, primitive, &layers, shadow);
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
