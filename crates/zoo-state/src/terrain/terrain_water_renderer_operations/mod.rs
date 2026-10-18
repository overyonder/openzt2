use bevy::camera::primitives::{Aabb, Frustum};
use bevy::prelude::{Vec2, Vec3};

use super::terrain_water_geometric_wave_shader_state::NativeTerrainWaterGeometricWaveShaderState;
use super::terrain_water_renderer_types::AuthoredTerrainWaterSurfaceEffectPass;

impl AuthoredTerrainWaterSurfaceEffectPass {
    pub fn intersects_camera_frustum(
        &self,
        frustum: &Frustum,
        geometric_wave_state: &NativeTerrainWaterGeometricWaveShaderState,
    ) -> bool {
        let Some(displacement) = geometric_wave_state
            .maximum_horizontal_and_vertical_shader_displacement(
                self.maximum_absolute_input_height,
            )
        else {
            // Do not reject a surface whose supplied wave inputs cannot be bounded.
            return true;
        };
        let minimum = self.horizontal_minimum - Vec2::splat(displacement.x);
        let maximum = self.horizontal_maximum + Vec2::splat(displacement.x);
        frustum.intersects_obb_identity(&Aabb::from_min_max(
            Vec3::new(
                minimum.x,
                self.surface_plane_height - displacement.y,
                minimum.y,
            ),
            Vec3::new(
                maximum.x,
                self.surface_plane_height + displacement.y,
                maximum.y,
            ),
        ))
    }
}
