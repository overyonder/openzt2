use super::authored_globe_presentation_types::UiGlobeCamera;
use super::authored_globe_presentation_types::UiGlobePrimaryRenderable;
use super::authored_globe_presentation_types::UiGlobeSceneOwner;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobePresentation;
use crate::plugins::world_spawn::prefab_presentation_types::PrefabModel;
use bevy::camera::Viewport;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;

pub(in crate::plugins::ui) fn update_globe_camera_viewports(
    windows: Query<&Window, With<PrimaryWindow>>,
    globes: Query<
        (
            Entity,
            &ComputedNode,
            &UiGlobalTransform,
            &InheritedVisibility,
        ),
        With<UiGlobePresentation>,
    >,
    mut cameras: Query<(&UiGlobeSceneOwner, &mut Camera), With<UiGlobeCamera>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let window_size = UVec2::new(
        window.resolution.physical_width(),
        window.resolution.physical_height(),
    );
    for (owner, mut camera) in &mut cameras {
        let Ok((_, node, transform, visibility)) = globes.get(owner.0) else {
            if camera.is_active {
                camera.is_active = false;
            }
            continue;
        };
        let half = node.size() * 0.5;
        let first = transform.transform_point2(-half);
        let second = transform.transform_point2(half);
        let minimum = first.min(second).max(Vec2::ZERO).floor().as_uvec2();
        let maximum = first
            .max(second)
            .min(window_size.as_vec2())
            .ceil()
            .as_uvec2();
        let size = maximum.saturating_sub(minimum);
        let is_active = visibility.get() && size.x > 0 && size.y > 0;
        let viewport = is_active.then_some(Viewport {
            physical_position: minimum,
            physical_size: size,
            depth: 0.0..1.0,
        });
        let viewport_changed = match (&camera.viewport, &viewport) {
            (Some(current), Some(requested)) => {
                current.physical_position != requested.physical_position
                    || current.physical_size != requested.physical_size
                    || current.depth != requested.depth
            }
            (None, None) => false,
            (Some(_), None) | (None, Some(_)) => true,
        };
        if camera.is_active != is_active || viewport_changed {
            camera.is_active = is_active;
            camera.viewport = viewport;
        }
    }
}

pub(in crate::plugins::ui) fn frame_globe_cameras(
    models: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<bevy::gltf::GltfMesh>>,
    meshes: Res<Assets<Mesh>>,
    prefabs: Query<(&PrefabModel, &GlobalTransform), With<UiGlobePrimaryRenderable>>,
    mut cameras: Query<&mut Transform, With<UiGlobeCamera>>,
) {
    let Some((center, radius)) =
        transformed_model_bounds(&models, &gltf_meshes, &meshes, &prefabs, None)
    else {
        return;
    };
    for mut transform in &mut cameras {
        // The physical globe fills nine tenths of its authored viewport,
        // leaving room for markers whose meshes extend beyond the sphere.
        let distance = radius / (std::f32::consts::FRAC_PI_8.tan()) * 1.1;
        // The authored BFPhysObj is at source Y=+100 and is viewed from the
        // origin toward +Y. Source-to-Bevy conversion maps that axis to -Z,
        // so the equivalent camera belongs on the positive-Z hemisphere.
        let requested_transform =
            Transform::from_translation(center + Vec3::Z * distance).looking_at(center, Vec3::Y);
        if *transform != requested_transform {
            *transform = requested_transform;
        }
    }
}

pub(super) fn transformed_model_bounds(
    models: &Assets<Gltf>,
    gltf_meshes: &Assets<bevy::gltf::GltfMesh>,
    meshes: &Assets<Mesh>,
    prefabs: &Query<(&PrefabModel, &GlobalTransform), With<UiGlobePrimaryRenderable>>,
    relative_to: Option<&GlobalTransform>,
) -> Option<(Vec3, f32)> {
    prefabs.iter().fold(None, |bounds, (prefab, transform)| {
        let Some(model) = prefab.handle.as_ref().and_then(|handle| models.get(handle)) else {
            return bounds;
        };
        let transform = relative_to.map_or(*transform, |parent| {
            GlobalTransform::from(parent.affine().inverse() * transform.affine())
        });
        let scale = transform
            .to_scale_rotation_translation()
            .0
            .abs()
            .max_element();
        crate::assets::model::model_asset_queries::loaded_gltf_model_aggregate_local_bounds(
            model,
            gltf_meshes,
            meshes,
        )
        .into_iter()
        .fold(bounds, |bounds, (minimum, maximum)| {
            let local_center = (minimum + maximum) * 0.5;
            let half_extents = (maximum - minimum) * 0.5;
            let center = transform.transform_point(local_center);
            // Globe meshes are spherical but their generic model bound stores
            // the enclosing AABB diagonal as `radius`. Framing with that value
            // leaves the authored globe sqrt(3) too small inside its bezel.
            let radius = (half_extents.max_element() * scale).max(0.001);
            Some(match bounds {
                None => (center, radius),
                Some((old_center, old_radius)) => (
                    old_center,
                    old_radius.max(old_center.distance(center) + radius),
                ),
            })
        })
    })
}
