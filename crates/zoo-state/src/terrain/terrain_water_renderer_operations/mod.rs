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
        self.wave_expanded_world_bounds(geometric_wave_state)
            .is_none_or(|bounds| frustum.intersects_obb_identity(&bounds))
    }

    pub fn wave_expanded_world_bounds(
        &self,
        geometric_wave_state: &NativeTerrainWaterGeometricWaveShaderState,
    ) -> Option<Aabb> {
        let displacement = geometric_wave_state
            .maximum_horizontal_and_vertical_shader_displacement(
                self.maximum_absolute_input_height,
            )?;
        Some(self.world_bounds(displacement))
    }

    pub fn world_bounds(&self, displacement: Vec2) -> Aabb {
        let minimum = self.horizontal_minimum - Vec2::splat(displacement.x);
        let maximum = self.horizontal_maximum + Vec2::splat(displacement.x);
        Aabb::from_min_max(
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
        )
    }
}
