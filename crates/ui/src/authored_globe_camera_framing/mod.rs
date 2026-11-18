use super::authored_globe_model_bounds::UiGlobeModelBounds;
use super::authored_globe_presentation_types::UiGlobeCamera;
use super::authored_globe_presentation_types::UiGlobePrimaryRenderable;
use super::authored_globe_presentation_types::UiGlobeRenderScene;
use super::authored_globe_presentation_types::UiGlobeSceneOwner;
use crate::authored_globe_presentation_types::UiGlobePresentation;
use bevy::camera::Viewport;
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;

pub(crate) fn update_globe_camera_viewports(
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

pub(crate) fn frame_globe_cameras(
    prefabs: Query<(
        &UiGlobePrimaryRenderable,
        &UiGlobeModelBounds,
        &GlobalTransform,
    )>,
    scenes: Query<&UiGlobeRenderScene>,
    mut cameras: Query<(&UiGlobeSceneOwner, &mut Transform), With<UiGlobeCamera>>,
) {
    for (owner, mut transform) in &mut cameras {
        let Ok(scene) = scenes.get(owner.0) else {
            continue;
        };
        let Some((center, radius)) = transformed_model_bounds(&prefabs, scene.world, None) else {
            continue;
        };
        let distance = radius / (std::f32::consts::FRAC_PI_8.tan()) * 1.1;
        let pose =
            Transform::from_translation(center + Vec3::Z * distance).looking_at(center, Vec3::Y);
        transform.set_if_neq(pose);
    }
}

pub(super) fn transformed_model_bounds(
    prefabs: &Query<(
        &UiGlobePrimaryRenderable,
        &UiGlobeModelBounds,
        &GlobalTransform,
    )>,
    globe_world: Entity,
    relative_to: Option<&GlobalTransform>,
) -> Option<(Vec3, f32)> {
    prefabs
        .iter()
        .filter(|(owner, _, _)| owner.0 == globe_world)
        .map(|(_, bounds, transform)| {
            let transform = transform.affine();
            let transform =
                relative_to.map_or(transform, |parent| parent.affine().inverse() * transform);
            let scale = transform
                .to_scale_rotation_translation()
                .0
                .abs()
                .max_element();
            let center = transform.transform_point3((bounds.minimum + bounds.maximum) * 0.5);
            let radius = ((bounds.maximum - bounds.minimum).max_element() * 0.5 * scale).max(0.001);
            (center, radius)
        })
        .reduce(|(center, radius), (other_center, other_radius)| {
            (
                center,
                radius.max(center.distance(other_center) + other_radius),
            )
        })
}
