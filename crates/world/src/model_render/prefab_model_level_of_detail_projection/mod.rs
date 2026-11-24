use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use openzt2_game_assets::graphics_settings_types::{GraphicsSettings, ThreeLevelGraphicsDetail};
use openzt2_world_state::model_render::prefab_render_lod_tier::PrefabRenderLodSelection;
use openzt2_world_state::world_spawn::prefab_model_level_of_detail_visibility_range::{
    PrefabLodRanges, PrefabModelLevelOfDetailVisibilityRange,
};

use super::active_model_view_cameras::ActiveModelViewCameras;
use super::prefab_render_layer_camera_matching::PrefabCameraRenderLayers;

pub(super) fn project_prefab_model_level_of_detail_visibility(
    cameras: Res<ActiveModelViewCameras>,
    graphics: Res<GraphicsSettings>,
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
    mut lods: Query<(
        &GlobalTransform,
        &PrefabModelLevelOfDetailVisibilityRange,
        Option<&PrefabCameraRenderLayers>,
        Option<&PrefabRenderLodSelection>,
        &mut Visibility,
    )>,
    mut changed_groups: Local<EntityHashSet>,
) {
    let refresh_all = graphics.is_changed() || cameras.is_changed();
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
    let view_factor: f32 = match graphics.object_detail {
        ThreeLevelGraphicsDetail::Low => 2.0,
        ThreeLevelGraphicsDetail::Medium => 1.0,
        ThreeLevelGraphicsDetail::High => 1.0 / 1.5,
    };
    let view_factor_squared = view_factor * view_factor;
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
        let selected = members
            .iter()
            .filter_map(|member| {
                let (fallback, range, layers, tier_selection, _) = lods.get(member).ok()?;
                if tier_selection.is_some_and(|selection| !selection.0) {
                    return None;
                }
                if range.active_without_range {
                    return Some(range.ordinal);
                }
                let layers = layers.map(|layers| &layers.0).unwrap_or_default();
                let camera = cameras.for_level_of_detail(layers)?;
                let center = transform.map_or_else(
                    || fallback.translation(),
                    |transform| transform.transform_point(range.center_m),
                );
                let distance_squared =
                    camera.pose.translation().distance_squared(center) * view_factor_squared;
                let near = range.near_m * world_scale;
                let far = range.far_m * world_scale;
                (distance_squared >= near * near && distance_squared < far * far)
                    .then_some(range.ordinal)
            })
            .max();
        for member in members.iter() {
            if let Ok((_, range, _, tier_selection, mut visibility)) = lods.get_mut(member) {
                visibility.set_if_neq(
                    if Some(range.ordinal) == selected
                        && tier_selection.is_none_or(|selection| selection.0)
                    {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    },
                );
            }
        }
    }
}
