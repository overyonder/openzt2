use bevy::{
    camera::visibility::RenderLayers, ecs::entity::EntityHashMap, math::Affine3A, prelude::*,
};

use crate::plugins::world_spawn::prefab_model_level_of_detail_visibility_range::PrefabModelLevelOfDetailVisibilityRange;

use super::prefab_render_layer_camera_matching::render_layers_for_prefab_entity_and_ancestor_chain;

pub(super) fn project_prefab_model_level_of_detail_visibility(
    cameras: Query<(Entity, &Camera, &GlobalTransform, Option<&RenderLayers>), With<Camera3d>>,
    groups: Query<Ref<GlobalTransform>>,
    hierarchy: Query<(Option<&ChildOf>, Option<&RenderLayers>)>,
    mut lods: Query<(
        Entity,
        Ref<GlobalTransform>,
        Ref<PrefabModelLevelOfDetailVisibilityRange>,
        &mut Visibility,
    )>,
    mut camera_state_buffers: Local<(
        Vec<(Entity, bool, Affine3A, RenderLayers)>,
        Vec<(Entity, bool, Affine3A, RenderLayers)>,
    )>,
) {
    let (previous_camera_states, current_camera_states) = &mut *camera_state_buffers;
    current_camera_states.clear();
    current_camera_states.extend(cameras.iter().map(|(entity, camera, transform, layers)| {
        (
            entity,
            camera.is_active,
            transform.affine(),
            layers.cloned().unwrap_or_default(),
        )
    }));
    let camera_changed = *previous_camera_states != *current_camera_states;
    std::mem::swap(previous_camera_states, current_camera_states);
    let level_of_detail_changed = lods.iter().any(|(_, transform, range, _)| {
        range.is_changed()
            || groups
                .get(range.group)
                .map_or_else(|_| transform.is_changed(), |group| group.is_changed())
    });
    if !camera_changed && !level_of_detail_changed {
        return;
    }
    let level_of_detail_range_matches = lods
        .iter()
        .map(|(entity, transform, range, _)| {
            let lod_layers = render_layers_for_prefab_entity_and_ancestor_chain(entity, &hierarchy);
            let camera_transform = cameras
                .iter()
                .find(|(_, camera, _, camera_layers)| {
                    camera.is_active && camera_layers.unwrap_or_default().intersects(&lod_layers)
                })
                .map(|(_, _, transform, _)| transform);
            let range_matches = camera_transform
                .map(|camera_transform| {
                    if range.active_without_range {
                        return true;
                    }
                    let (origin, authored_range_scale) = groups.get(range.group).map_or_else(
                        |_| (transform.translation(), 1.0),
                        |value| {
                            let (scale, _, _) = value.to_scale_rotation_translation();
                            (
                                value.transform_point(range.center_m),
                                scale.abs().max_element(),
                            )
                        },
                    );
                    // NiRangeLODData selects from camera-space Z depth, not
                    // radial camera distance. Its ranges are half-open and
                    // the first matching authored range owns the group.
                    let camera_space_depth = camera_transform
                        .forward()
                        .dot(origin - camera_transform.translation());
                    camera_space_depth >= range.near_m * authored_range_scale
                        && camera_space_depth < range.far_m * authored_range_scale
                })
                .unwrap_or(range.active_without_range);
            (range.group, range.ordinal, range_matches)
        })
        .collect::<Vec<_>>();
    let mut selected_ordinal_by_group = EntityHashMap::<u16>::default();
    for (group, ordinal, range_matches) in &level_of_detail_range_matches {
        if *range_matches {
            selected_ordinal_by_group
                .entry(*group)
                .and_modify(|selected| *selected = (*selected).min(*ordinal))
                .or_insert(*ordinal);
        }
    }
    for ((_, ordinal, _), (_, _, range, mut visibility)) in
        level_of_detail_range_matches.into_iter().zip(&mut lods)
    {
        let requested_visibility = if selected_ordinal_by_group.get(&range.group) == Some(&ordinal)
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != requested_visibility {
            *visibility = requested_visibility;
        }
    }
}
