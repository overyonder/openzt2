use bevy::camera::visibility::RenderLayers;
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use openzt2_game_assets::graphics_settings_types::{GraphicsSettings, ThreeLevelGraphicsDetail};
use openzt2_world_state::world_spawn::prefab_model_level_of_detail_visibility_range::{
    PrefabLodRanges, PrefabModelLevelOfDetailVisibilityRange,
};

use super::prefab_render_layer_camera_matching::PrefabCameraRenderLayers;

#[derive(Default)]
pub(super) struct LodSelection {
    previous_cameras: Vec<(Entity, Vec3, Dir3, RenderLayers)>,
    current_cameras: Vec<(Entity, Vec3, Dir3, RenderLayers)>,
    changed_groups: EntityHashSet,
}

pub(super) fn project_prefab_model_level_of_detail_visibility(
    cameras: Query<(Entity, &Camera, &GlobalTransform, Option<&RenderLayers>), With<Camera3d>>,
    graphics: Res<GraphicsSettings>,
    groups: Query<(Entity, Ref<PrefabLodRanges>, Option<Ref<GlobalTransform>>)>,
    changed_layers: Query<
        &PrefabModelLevelOfDetailVisibilityRange,
        Changed<PrefabCameraRenderLayers>,
    >,
    mut removed_layers: RemovedComponents<PrefabCameraRenderLayers>,
    mut lods: Query<(
        &GlobalTransform,
        &PrefabModelLevelOfDetailVisibilityRange,
        Option<&PrefabCameraRenderLayers>,
        &mut Visibility,
    )>,
    mut selection: Local<LodSelection>,
) {
    let selection = &mut *selection;
    selection.current_cameras.clear();
    // Water reflection and refraction captures also see the world's layers,
    // but render to textures before the view that shows them. List the views
    // drawn last first, so a model's level of detail follows the screen.
    let mut active_cameras = cameras
        .iter()
        .filter(|(_, camera, ..)| camera.is_active)
        .collect::<Vec<_>>();
    active_cameras.sort_by_key(|(entity, camera, ..)| (std::cmp::Reverse(camera.order), *entity));
    selection
        .current_cameras
        .extend(
            active_cameras
                .into_iter()
                .map(|(entity, _, transform, layers)| {
                    (
                        entity,
                        transform.translation(),
                        transform.forward(),
                        layers.cloned().unwrap_or_default(),
                    )
                }),
        );
    let refresh_all =
        graphics.is_changed() || selection.current_cameras != selection.previous_cameras;
    std::mem::swap(
        &mut selection.current_cameras,
        &mut selection.previous_cameras,
    );
    selection.changed_groups.clear();
    if refresh_all {
        removed_layers.clear();
    } else {
        // The ranges in a group share a selected ordinal. A changed member
        // only invalidates its own group's selection.
        selection
            .changed_groups
            .extend(changed_layers.iter().map(|range| range.group));
        selection.changed_groups.extend(
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
            && !selection.changed_groups.contains(&group)
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
            let Ok((fallback, range, layers, _)) = lods.get(member) else {
                continue;
            };
            let matches = range.active_without_range || {
                let layers = layers.map(|layers| &layers.0).unwrap_or_default();
                selection
                    .previous_cameras
                    .iter()
                    .find(|(_, _, _, camera_layers)| camera_layers.intersects(layers))
                    .is_some_and(|(camera, position, forward, _)| {
                        let origin = transform.map_or_else(
                            || fallback.translation(),
                            |t| t.transform_point(range.center_m),
                        );
                        let depth = match previous_depth {
                            Some((id, center, depth)) if id == *camera && center == origin => depth,
                            _ => {
                                let depth = forward.dot(origin - *position);
                                previous_depth = Some((*camera, origin, depth));
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
            if let Ok((_, range, _, mut visibility)) = lods.get_mut(member) {
                visibility.set_if_neq(if Some(range.ordinal) == selected {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
    }
}
