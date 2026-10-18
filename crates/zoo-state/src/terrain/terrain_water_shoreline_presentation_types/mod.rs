use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainWaterShoreConfiguration;

/// One shoreline draw, parented to the terrain chunk that owns its mesh.
#[derive(Component, Debug)]
pub struct TerrainWaterShorelineSurface {
    pub material_owner: Entity,
    pub biome_index: u8,
    pub surface_plane_height: f32,
}

/// Animated texture coordinates for one shoreline material instance.
#[derive(Component, Debug)]
pub struct TerrainWaterShorelinePresentation {
    bottom_rate: f32,
    top_rate: f32,
    bottom_uv_map: Vec4,
    top_uv_map: Vec4,
}

impl TerrainWaterShorelinePresentation {
    pub fn from_configuration(configuration: Option<&TerrainWaterShoreConfiguration>) -> Self {
        let mut state = Self {
            bottom_rate: 0.01,
            top_rate: -0.06,
            bottom_uv_map: Vec4::new(1.0, 0.25, 0.0, 0.0),
            top_uv_map: Vec4::new(1.0, 0.2, 0.5, 0.5),
        };
        state.apply_configuration(configuration);
        state
    }

    /// Updates material properties while retaining the accumulated texture phase.
    pub fn apply_configuration(&mut self, configuration: Option<&TerrainWaterShoreConfiguration>) {
        if let Some(rate) = configuration.and_then(|configuration| configuration.bottom_rate) {
            self.bottom_rate = rate;
        }
        if let Some(rate) = configuration.and_then(|configuration| configuration.top_rate) {
            self.top_rate = rate;
        }
        if let Some(size) = configuration
            .and_then(|configuration| configuration.bottom_size)
            .filter(|size| *size > 0.0)
        {
            self.bottom_uv_map.y = size.recip();
        }
        if let Some(size) = configuration
            .and_then(|configuration| configuration.top_size)
            .filter(|size| *size > 0.0)
        {
            self.top_uv_map.y = size.recip();
        }
    }

    pub fn advance(&mut self, delta_seconds: f32) {
        for (uv_map, rate) in [
            (&mut self.bottom_uv_map, self.bottom_rate),
            (&mut self.top_uv_map, self.top_rate),
        ] {
            uv_map.w += rate * delta_seconds;
            // Each frame performs one correction and preserves the endpoint at one.
            if uv_map.w > 1.0 {
                uv_map.w -= 1.0;
            } else if uv_map.w < 0.0 {
                uv_map.w += 1.0;
            }
        }
    }

    pub fn bottom_uv_map(&self) -> Vec4 {
        self.bottom_uv_map
    }

    pub fn top_uv_map(&self) -> Vec4 {
        self.top_uv_map
    }
}
