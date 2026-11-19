use super::authored_globe_model_bounds::UiGlobeModelBounds;
use super::authored_globe_presentation_types::UiGlobeCamera;
use super::authored_globe_presentation_types::UiGlobePrimaryModels;
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

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct GlobeSphere {
    pub(super) center: Vec3,
    pub(super) radius: f32,
}

pub(super) fn cache_globe_spheres(
    mut commands: Commands,
    mut worlds: Query<(
        Entity,
        Ref<UiGlobePrimaryModels>,
        &GlobalTransform,
        Option<&mut GlobeSphere>,
    )>,
    models: Query<(Ref<UiGlobeModelBounds>, Ref<Transform>, &GlobalTransform)>,
) {
    for (world, parts, transform, current) in &mut worlds {
        if current.is_some()
            && !parts.is_changed()
            && !parts.iter().any(|part| {
                models
                    .get(part)
                    .is_ok_and(|(bounds, pose, _)| bounds.is_changed() || pose.is_changed())
            })
        {
            continue;
        }
        let inverse = transform.affine().inverse();
        let bounds = parts
            .iter()
            .filter_map(|part| models.get(part).ok())
            .map(|(bounds, _, transform)| {
                let transform = inverse * transform.affine();
                let scale = transform
                    .to_scale_rotation_translation()
                    .0
                    .abs()
                    .max_element();
                let center = transform.transform_point3((bounds.minimum + bounds.maximum) * 0.5);
                let radius =
                    ((bounds.maximum - bounds.minimum).max_element() * 0.5 * scale).max(0.001);
                GlobeSphere { center, radius }
            })
            .reduce(|first, next| GlobeSphere {
                center: first.center,
                radius: first
                    .radius
                    .max(first.center.distance(next.center) + next.radius),
            });
        match (current, bounds) {
            (Some(mut current), Some(bounds)) => {
                current.set_if_neq(bounds);
            }
            (None, Some(bounds)) => {
                commands.entity(world).insert(bounds);
            }
            (Some(_), None) => {
                commands.entity(world).remove::<GlobeSphere>();
            }
            (None, None) => {}
        }
    }
}

pub(crate) fn frame_globe_cameras(
    spheres: Query<(Ref<GlobeSphere>, &GlobalTransform)>,
    scenes: Query<&UiGlobeRenderScene>,
    mut cameras: Query<(&UiGlobeSceneOwner, &mut Transform), With<UiGlobeCamera>>,
) {
    for (owner, mut transform) in &mut cameras {
        let Ok(scene) = scenes.get(owner.0) else {
            continue;
        };
        let Ok((sphere, world)) = spheres.get(scene.world()) else {
            continue;
        };
        let center = world.transform_point(sphere.center);
        let distance = sphere.radius / std::f32::consts::FRAC_PI_8.tan() * 1.1;
        let pose =
            Transform::from_translation(center + Vec3::Z * distance).looking_at(center, Vec3::Y);
        transform.set_if_neq(pose);
    }
}
