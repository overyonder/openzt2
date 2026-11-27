use bevy::app::{HierarchyPropagatePlugin, Propagate, PropagateSet};
use bevy::camera::visibility::{VisibilitySystems, VisibleEntities};
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use openzt2_world_state::model_render::prefab_render_lod_tier::PrefabRenderLodSelection;
use openzt2_world_state::world_spawn::prefab_model_level_of_detail_visibility_range::{
    PrefabLodRanges, PrefabModelLevelOfDetailVisibilityRange,
};

use super::active_model_view_cameras::ActiveModelViewCameras;
use super::prefab_render_layer_camera_matching::PrefabCameraRenderLayers;

#[derive(Component, Clone, PartialEq, Eq)]
pub(super) struct PrefabLodVisibleCameras(pub(super) EntityHashSet);

pub(super) fn register_prefab_lod_camera_visibility(app: &mut App) {
    app.add_plugins(HierarchyPropagatePlugin::<PrefabLodVisibleCameras>::new(
        PostUpdate,
    ))
    .configure_sets(
        PostUpdate,
        PropagateSet::<PrefabLodVisibleCameras>::default()
            .after(project_prefab_model_level_of_detail_visibility)
            .before(VisibilitySystems::CheckVisibility),
    )
    .add_systems(
        PostUpdate,
        retain_prefab_lod_draws_for_their_camera.after(VisibilitySystems::CheckVisibility),
    );
}

fn retain_prefab_lod_draws_for_their_camera(
    mut views: Query<(Entity, &mut VisibleEntities), With<Camera3d>>,
    lod_members: Query<&PrefabLodVisibleCameras>,
    parents: Query<&ChildOf>,
    lod_group_members: Query<&Propagate<PrefabLodVisibleCameras>>,
) {
    if lod_members.is_empty() {
        return;
    }
    views.par_iter_mut().for_each(|(camera, mut visible)| {
        for entities in visible.entities.values_mut() {
            entities.retain(|&entity| {
                let Ok(nearest) = lod_members.get(entity) else {
                    return true;
                };
                nearest.0.contains(&camera)
                    && parents
                        .iter_ancestors::<ChildOf>(entity)
                        .filter_map(|ancestor| lod_group_members.get(ancestor).ok())
                        .all(|ancestor| ancestor.0 .0.contains(&camera))
            });
        }
    });
}

pub(super) fn project_prefab_model_level_of_detail_visibility(
    mut commands: Commands,
    cameras: Res<ActiveModelViewCameras>,
    groups: Query<(Entity, Ref<PrefabLodRanges>, Option<Ref<GlobalTransform>>)>,
    changed_layers: Query<
        &PrefabModelLevelOfDetailVisibilityRange,
        Or<(
            Changed<PrefabCameraRenderLayers>,
            Changed<PrefabModelLevelOfDetailVisibilityRange>,
            Changed<PrefabRenderLodSelection>,
        )>,
    >,
    mut removed_layers: RemovedComponents<PrefabCameraRenderLayers>,
    mut removed_ranges: RemovedComponents<PrefabModelLevelOfDetailVisibilityRange>,
    mut lods: Query<(
        &GlobalTransform,
        &PrefabModelLevelOfDetailVisibilityRange,
        Option<&PrefabCameraRenderLayers>,
        Option<&PrefabRenderLodSelection>,
        &mut Visibility,
        Option<&mut Propagate<PrefabLodVisibleCameras>>,
    )>,
    mut changed_groups: Local<EntityHashSet>,
) {
    for entity in removed_ranges
        .read()
        .filter(|&entity| !lods.contains(entity))
    {
        commands
            .entity(entity)
            .try_remove::<Propagate<PrefabLodVisibleCameras>>();
    }
    let refresh_all = cameras.is_changed();
    changed_groups.clear();
    if refresh_all {
        removed_layers.clear();
    } else {
        // The ranges in a group share a selected ordinal. A changed member
        // only invalidates its own group's selection.
        changed_groups.extend(changed_layers.iter().map(|range| range.group));
        changed_groups.extend(
            removed_layers
                .read()
                .filter_map(|entity| lods.get(entity).ok().map(|(_, range, ..)| range.group)),
        );
    }
    for (group, members, transform) in &groups {
        if !refresh_all
            && !members.is_changed()
            && transform
                .as_ref()
                .is_some_and(|transform| !transform.is_changed())
            && !changed_groups.contains(&group)
        {
            continue;
        }
        let transform = transform.as_deref();
        let world_scale = transform.map_or(1.0, |transform| {
            transform
                .to_scale_rotation_translation()
                .0
                .abs()
                .max_element()
        });
        let mut selected_views =
            bevy::platform::collections::HashMap::<u16, EntityHashSet>::default();
        for (camera, camera_layers) in &cameras.views {
            let selected = members
                .iter()
                .filter_map(|member| {
                    let (fallback, range, layers, tier_selection, ..) = lods.get(member).ok()?;
                    let layers = layers.map(|layers| &layers.0).unwrap_or_default();
                    if !camera_layers.intersects(layers)
                        || tier_selection.is_some_and(|selection| !selection.0)
                    {
                        return None;
                    }
                    if range.active_without_range {
                        return Some(range.ordinal);
                    }
                    let center = transform.map_or_else(
                        || fallback.translation(),
                        |transform| transform.transform_point(range.center_m),
                    );
                    let distance_squared = camera.pose.translation().distance_squared(center)
                        * camera.lod_view_factor.powi(2);
                    let near = range.near_m * world_scale;
                    let far = range.far_m * world_scale;
                    (distance_squared >= near * near && distance_squared < far * far)
                        .then_some(range.ordinal)
                })
                .max();
            if let Some(ordinal) = selected {
                selected_views
                    .entry(ordinal)
                    .or_default()
                    .insert(camera.entity);
            }
        }
        for member in members.iter() {
            if let Ok((_, range, _, tier_selection, mut visibility, selected)) =
                lods.get_mut(member)
            {
                let cameras = selected_views
                    .get(&range.ordinal)
                    .cloned()
                    .unwrap_or_default();
                let visible =
                    !cameras.is_empty() && tier_selection.is_none_or(|selection| selection.0);
                visibility.set_if_neq(if visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                let next = Propagate(PrefabLodVisibleCameras(cameras));
                if let Some(mut selected) = selected {
                    selected.set_if_neq(next);
                } else {
                    commands.entity(member).try_insert(next);
                }
            }
        }
    }
}
