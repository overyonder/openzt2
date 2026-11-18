use bevy::app::{HierarchyPropagatePlugin, Propagate, PropagateSet};
use bevy::camera::visibility::{RenderLayers, VisibilitySystems};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabShadowPolicy;
use openzt2_zoo_state::terrain::terrain_water_caustic_types::TerrainWaterCausticScenePass;

use super::authored_model_material_pass_projection::project_authored_model_material_passes;

#[cfg(test)]
mod tests;

#[derive(Component, Clone, PartialEq)]
struct ModelDrawShadowPolicy {
    layers: RenderLayers,
    casts: bool,
    receives: bool,
}

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProjectedModelDrawShadowPolicy {
    layers: bool,
    casting: bool,
    receiving: bool,
}

pub(super) fn register_inherited_model_draw_shadow_policy(app: &mut App) {
    app.add_plugins(HierarchyPropagatePlugin::<ModelDrawShadowPolicy>::new(
        PostUpdate,
    ))
    .add_systems(
        PostUpdate,
        (
            publish_model_draw_shadow_policy_sources
                .before(PropagateSet::<ModelDrawShadowPolicy>::default()),
            project_inherited_model_draw_shadow_policy
                .after(PropagateSet::<ModelDrawShadowPolicy>::default())
                .before(project_authored_model_material_passes)
                .before(VisibilitySystems::CheckVisibility),
        ),
    );
}

fn publish_model_draw_shadow_policy_sources(
    mut commands: Commands,
    changed: Query<
        Entity,
        (
            With<PrefabShadowPolicy>,
            Or<(Changed<PrefabShadowPolicy>, Changed<RenderLayers>)>,
        ),
    >,
    mut removed_layers: RemovedComponents<RenderLayers>,
    mut removed_policies: RemovedComponents<PrefabShadowPolicy>,
    mut sources: Query<(
        &PrefabShadowPolicy,
        Option<&RenderLayers>,
        Option<&mut Propagate<ModelDrawShadowPolicy>>,
    )>,
) {
    for entity in changed.iter().chain(removed_layers.read()) {
        let Ok((shadow, layers, previous)) = sources.get_mut(entity) else {
            continue;
        };
        let policy = Propagate(ModelDrawShadowPolicy {
            layers: layers.cloned().unwrap_or_else(|| {
                if shadow.visible_in_reflections {
                    RenderLayers::from_layers(&[0, 1])
                } else {
                    RenderLayers::layer(0)
                }
            }),
            casts: shadow.casts,
            receives: shadow.receives,
        });
        if let Some(mut previous) = previous {
            previous.set_if_neq(policy);
        } else {
            commands.entity(entity).insert(policy);
        }
    }
    for entity in removed_policies.read() {
        if !sources.contains(entity) {
            if let Ok(mut target) = commands.get_entity(entity) {
                target.remove::<Propagate<ModelDrawShadowPolicy>>();
            }
        }
    }
}

fn project_inherited_model_draw_shadow_policy(
    mut commands: Commands,
    changed: Query<
        Entity,
        (
            With<Mesh3d>,
            Or<(Changed<ModelDrawShadowPolicy>, Added<Mesh3d>)>,
        ),
    >,
    mut removed: RemovedComponents<ModelDrawShadowPolicy>,
    mut draws: Query<
        (
            Option<&ModelDrawShadowPolicy>,
            Option<&mut RenderLayers>,
            Has<NotShadowCaster>,
            Has<NotShadowReceiver>,
            Option<&mut ProjectedModelDrawShadowPolicy>,
        ),
        (With<Mesh3d>, Without<TerrainWaterCausticScenePass>),
    >,
) {
    for entity in changed.iter().chain(removed.read()) {
        let Ok((policy, layers, no_casting, no_receiving, previous)) = draws.get_mut(entity) else {
            continue;
        };
        let mut projected = previous.as_deref().copied().unwrap_or_default();
        let mut target = commands.entity(entity);
        if let Some(policy) = policy {
            if let Some(mut layers) = layers {
                if projected.layers {
                    layers.set_if_neq(policy.layers.clone());
                }
            } else {
                target.insert(policy.layers.clone());
                projected.layers = true;
            }
        } else if projected.layers {
            target.remove::<RenderLayers>();
            projected.layers = false;
        }

        let blocks_casting = policy.is_some_and(|policy| !policy.casts);
        let blocks_receiving = policy.is_some_and(|policy| !policy.receives);
        if blocks_casting && !no_casting {
            target.insert(NotShadowCaster);
            projected.casting = true;
        } else if !blocks_casting && projected.casting {
            target.remove::<NotShadowCaster>();
            projected.casting = false;
        }
        if blocks_receiving && !no_receiving {
            target.insert(NotShadowReceiver);
            projected.receiving = true;
        } else if !blocks_receiving && projected.receiving {
            target.remove::<NotShadowReceiver>();
            projected.receiving = false;
        }
        if let Some(mut previous) = previous {
            previous.set_if_neq(projected);
        } else if projected != ProjectedModelDrawShadowPolicy::default() {
            target.insert(projected);
        }
    }
}
