use bevy::prelude::*;

/// Rendering animation owned by one water ray effect entity.
#[derive(Component)]
pub struct TerrainWaterRayAnimation {
    strip_texture_transforms: [[Mat4; 2]; 2],
    disk_texture_transforms: [Mat4; 2],
    disk_phase: f32,
    intensity: f32,
}

impl Default for TerrainWaterRayAnimation {
    fn default() -> Self {
        Self {
            strip_texture_transforms: [
                [
                    Mat4::from_scale(Vec3::new(2.0, 1.0, 1.0)),
                    Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)),
                ],
                [
                    Mat4::from_scale(Vec3::new(-4.0, 1.0, 1.0)),
                    Mat4::from_scale(Vec3::new(2.0, 1.0, 1.0)),
                ],
            ],
            disk_texture_transforms: [
                Mat4::from_scale(Vec3::new(0.4, 0.8, 1.0)),
                Mat4::from_cols(
                    Vec4::new(-0.3, 0.0, 0.0, 0.0),
                    Vec4::new(0.0, -0.6, 0.0, 0.0),
                    Vec4::new(0.0, 0.5, 1.0, 0.0),
                    Vec4::W,
                ),
            ],
            disk_phase: 0.0,
            intensity: 0.0,
        }
    }
}

impl TerrainWaterRayAnimation {
    pub fn intensity(&self) -> f32 {
        self.intensity
    }

    pub fn strip_texture_transforms(&self) -> &[[Mat4; 2]; 2] {
        &self.strip_texture_transforms
    }

    pub fn disk_texture_transforms(&self) -> &[Mat4; 2] {
        &self.disk_texture_transforms
    }

    pub fn advance_intensity(&mut self, target: f32, delta_seconds: f32) {
        let difference = target - self.intensity;
        let step = 4.0 * delta_seconds;
        if difference > 0.0 {
            self.intensity = if step < difference {
                self.intensity + step
            } else {
                target
            };
        } else if difference < 0.0 {
            self.intensity = if -step > difference {
                self.intensity - step
            } else {
                target
            };
        }
    }

    pub fn advance_texture_offsets(&mut self, delta_seconds: f32, secondary_strip_enabled: bool) {
        if !(delta_seconds > 0.0) {
            return;
        }
        self.strip_texture_transforms[0][0].z_axis.x =
            wrap_texture_offset(self.strip_texture_transforms[0][0].z_axis.x + 0.2 * delta_seconds);
        self.strip_texture_transforms[0][1].z_axis.x = wrap_texture_offset(
            self.strip_texture_transforms[0][1].z_axis.x + (1.0 / 7.0) * delta_seconds,
        );
        if secondary_strip_enabled {
            self.strip_texture_transforms[1][0].z_axis.x = wrap_texture_offset(
                self.strip_texture_transforms[1][0].z_axis.x + 0.2 * delta_seconds * 0.47,
            );
            self.strip_texture_transforms[1][1].z_axis.x = wrap_texture_offset(
                self.strip_texture_transforms[1][1].z_axis.x + (1.0 / 7.0) * delta_seconds * 0.51,
            );
        }
        self.disk_phase = wrap_texture_offset(self.disk_phase + 0.7 * delta_seconds);
        let (sine, cosine) = (self.disk_phase * std::f32::consts::TAU).sin_cos();
        self.disk_texture_transforms[0].z_axis.x =
            wrap_texture_offset(self.disk_texture_transforms[0].z_axis.x + 0.1 * delta_seconds);
        self.disk_texture_transforms[0].z_axis.y = sine * 0.01;
        self.disk_texture_transforms[1].z_axis.x =
            wrap_texture_offset(self.disk_texture_transforms[1].z_axis.x + 0.11 * delta_seconds);
        self.disk_texture_transforms[1].z_axis.y = cosine * 0.01;
    }
}

fn wrap_texture_offset(value: f32) -> f32 {
    if value < 0.0 {
        value % 1.0 + 1.0
    } else if value > 1.0 {
        value % 1.0
    } else {
        value
    }
}
