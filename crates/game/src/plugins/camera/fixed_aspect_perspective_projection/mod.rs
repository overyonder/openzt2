use bevy::{
    camera::{CameraProjection, PerspectiveProjection, SubCameraView},
    math::Vec3A,
    prelude::*,
};

/// Perspective projection whose authored frustum aspect remains fixed when
/// the window changes shape. Zoo Tycoon 2 renders its 4:3 world projection to
/// the display surface; only the surrounding UI layout uses display pixels.
#[derive(Clone, Debug)]
pub(super) struct FixedAspectPerspectiveProjection {
    perspective: PerspectiveProjection,
    aspect_ratio: f32,
}

impl FixedAspectPerspectiveProjection {
    pub(super) fn new(fov: f32, aspect_ratio: f32, near: f32, far: f32) -> Self {
        Self {
            perspective: PerspectiveProjection {
                fov,
                aspect_ratio,
                near,
                far,
                ..default()
            },
            aspect_ratio,
        }
    }
}

impl CameraProjection for FixedAspectPerspectiveProjection {
    fn get_clip_from_view(&self) -> Mat4 {
        self.perspective.get_clip_from_view()
    }

    fn get_clip_from_view_for_sub(&self, sub_view: &SubCameraView) -> Mat4 {
        let full_width = sub_view.full_size.x as f32;
        let full_height = sub_view.full_size.y as f32;
        let sub_width = sub_view.size.x as f32;
        let sub_height = sub_view.size.y as f32;
        let offset_x = sub_view.offset.x;
        let offset_y = full_height - (sub_view.offset.y + sub_height);
        let top = self.perspective.near * (self.perspective.fov * 0.5).tan();
        let bottom = -top;
        let right = top * self.aspect_ratio;
        let left = -right;
        let width = right - left;
        let height = top - bottom;
        let left_prime = left + width * offset_x / full_width;
        let right_prime = left + width * (offset_x + sub_width) / full_width;
        let bottom_prime = bottom + height * offset_y / full_height;
        let top_prime = bottom + height * (offset_y + sub_height) / full_height;
        Mat4::from_cols(
            Vec4::new(
                2.0 * self.perspective.near / (right_prime - left_prime),
                0.0,
                0.0,
                0.0,
            ),
            Vec4::new(
                0.0,
                2.0 * self.perspective.near / (top_prime - bottom_prime),
                0.0,
                0.0,
            ),
            Vec4::new(
                (right_prime + left_prime) / (right_prime - left_prime),
                (top_prime + bottom_prime) / (top_prime - bottom_prime),
                0.0,
                -1.0,
            ),
            Vec4::new(0.0, 0.0, self.perspective.near, 0.0),
        )
    }

    fn update(&mut self, _width: f32, _height: f32) {}

    fn far(&self) -> f32 {
        self.perspective.far
    }

    fn get_frustum_corners(&self, z_near: f32, z_far: f32) -> [Vec3A; 8] {
        let near_height = z_near.abs() * (self.perspective.fov * 0.5).tan();
        let far_height = z_far.abs() * (self.perspective.fov * 0.5).tan();
        let near_width = near_height * self.aspect_ratio;
        let far_width = far_height * self.aspect_ratio;
        [
            Vec3A::new(near_width, -near_height, z_near),
            Vec3A::new(near_width, near_height, z_near),
            Vec3A::new(-near_width, near_height, z_near),
            Vec3A::new(-near_width, -near_height, z_near),
            Vec3A::new(far_width, -far_height, z_far),
            Vec3A::new(far_width, far_height, z_far),
            Vec3A::new(-far_width, far_height, z_far),
            Vec3A::new(-far_width, -far_height, z_far),
        ]
    }
}
