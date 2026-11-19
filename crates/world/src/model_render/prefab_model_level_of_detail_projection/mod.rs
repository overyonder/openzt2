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
    let detail_scale = match graphics.object_detail {
        ThreeLevelGraphicsDetail::Low => 0.5,
        ThreeLevelGraphicsDetail::Medium => 1.0,
        ThreeLevelGraphicsDetail::High => 1.5,
    };
    for (group, members, transform) in &groups {
        if !refresh_all
            && !members.is_changed()
            && transform.as_ref().is_some_and(|t| !t.is_changed())
            && !changed_groups.contains(&group)
        {
            continue;
        }
        let transform = transform.as_deref();
        let scale = transform.map_or(1.0, |transform| {
            transform
                .to_scale_rotation_translation()
                .0
                .abs()
                .max_element()
        }) * detail_scale;
        let mut previous_depth = None;
        let mut selected = None;
        for member in members.iter() {
            let Ok((fallback, range, layers, tier_selection, _)) = lods.get(member) else {
                continue;
            };
            if tier_selection.is_some_and(|selection| !selection.0) {
                continue;
            }
            let matches = range.active_without_range || {
                let layers = layers.map(|layers| &layers.0).unwrap_or_default();
                cameras.for_level_of_detail(layers).is_some_and(|camera| {
                    let origin = transform.map_or_else(
                        || fallback.translation(),
                        |t| t.transform_point(range.center_m),
                    );
                    let depth = match previous_depth {
                        Some((id, center, depth)) if id == camera.entity && center == origin => {
                            depth
                        }
                        _ => {
                            let depth = camera
                                .pose
                                .forward()
                                .dot(origin - camera.pose.translation());
                            previous_depth = Some((camera.entity, origin, depth));
                            depth
                        }
                    };
                    depth >= range.near_m * scale && depth < range.far_m * scale
                })
            };
            if matches {
                selected =
                    Some(selected.map_or(range.ordinal, |value: u16| value.min(range.ordinal)));
            }
        }
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
