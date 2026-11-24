use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use openzt2_game_assets::graphics_settings_types::{GraphicsSettings, ThreeLevelGraphicsDetail};
use openzt2_world_state::world_spawn::prefab_model_level_of_detail_visibility_range::{
    simple_lod_ranges, PrefabLodRanges, PrefabModelLevelOfDetailVisibilityRange, SimpleLodRange,
    SimpleLodSwitching,
};

use crate::model_render::prefab_render_layer_camera_matching::PrefabRenderLayers;

#[derive(Default)]
pub(super) struct LodSelection {
    layer_revision: u64,
    cameras: Vec<(Entity, Vec3, Dir3, RenderLayers)>,
}

pub(super) fn project_prefab_model_level_of_detail_visibility(
    cameras: Query<(Entity, &Camera, &GlobalTransform, Option<&RenderLayers>), With<Camera3d>>,
    graphics: Res<GraphicsSettings>,
    groups: Query<(Ref<PrefabLodRanges>, Option<Ref<GlobalTransform>>)>,
    mut inherited_layers: PrefabRenderLayers,
    mut lods: Query<(
        &GlobalTransform,
        &PrefabModelLevelOfDetailVisibilityRange,
        &mut Visibility,
        Option<&SimpleLodRange>,
    )>,
    mut selection: Local<LodSelection>,
    mut current_cameras: Local<Vec<(Entity, Vec3, Dir3, RenderLayers)>>,
) {
    let selection = &mut *selection;
    current_cameras.clear();
    current_cameras.extend(
        cameras
            .iter()
            .filter(|(_, camera, ..)| camera.is_active)
            .map(|(entity, _, transform, layers)| {
                (
                    entity,
                    transform.translation(),
                    transform.forward(),
                    layers.cloned().unwrap_or_default(),
                )
            }),
    );
    let layers_changed = selection.layer_revision != inherited_layers.revision();
    selection.layer_revision = inherited_layers.revision();
    let refresh_all =
        layers_changed || graphics.is_changed() || *current_cameras != selection.cameras;
    std::mem::swap(&mut *current_cameras, &mut selection.cameras);
    let detail_scale = match graphics.object_detail {
        ThreeLevelGraphicsDetail::Low => 0.5,
        ThreeLevelGraphicsDetail::Medium => 1.0,
        ThreeLevelGraphicsDetail::High => 1.5,
    };
    for (members, transform) in &groups {
        if !refresh_all
            && !members.is_changed()
            && transform.as_ref().is_some_and(|t| !t.is_changed())
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
            let Ok((fallback, range, _, simple)) = lods.get(member) else {
                continue;
            };
            let (near_m, far_m) = simple.map_or((range.near_m, range.far_m), |simple| {
                (simple.near_m, simple.far_m)
            });
            let matches = range.active_without_range || {
                let layers = inherited_layers.get(member);
                selection
                    .cameras
                    .iter()
                    .find(|(_, _, _, camera_layers)| camera_layers.intersects(&layers))
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
                        depth >= near_m * scale && depth < far_m * scale
                    })
            };
            if matches {
                selected =
                    Some(selected.map_or(range.ordinal, |value: u16| value.min(range.ordinal)));
            }
        }
        for member in members.iter() {
            if let Ok((_, range, mut visibility, _)) = lods.get_mut(member) {
                visibility.set_if_neq(if Some(range.ordinal) == selected {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
    }
}

/// Gives the LOD levels of a `SimpleLodSwitching` presentation the game's own
/// distances once, when they spawn.
pub(super) fn apply_simple_lod_switching(
    mut commands: Commands,
    groups: Query<(Entity, &PrefabLodRanges), Added<PrefabLodRanges>>,
    parents: Query<&ChildOf>,
    switching: Query<(), With<SimpleLodSwitching>>,
    ranges: Query<&PrefabModelLevelOfDetailVisibilityRange>,
) {
    for (group, members) in &groups {
        if !std::iter::once(group)
            .chain(parents.iter_ancestors(group))
            .any(|entity| switching.contains(entity))
        {
            continue;
        }
        let far = members
            .iter()
            .map(|member| {
                ranges
                    .get(member)
                    .map_or(f32::INFINITY, |range| range.far_m)
            })
            .collect::<Vec<_>>();
        for (member, (near_m, far_m)) in members.iter().zip(simple_lod_ranges(&far)) {
            commands
                .entity(member)
                .insert(SimpleLodRange { near_m, far_m });
        }
    }
}
