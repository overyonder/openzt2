use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Current raw base ambient register for this water row's material passes.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct TerrainWaterRegionLight {
    raw_base_rgba: Vec4,
    source_light_direction: Vec3,
}

impl TerrainWaterRegionLight {
    pub fn from_sample(raw_base_rgba: Vec4, source_light_direction: Vec3) -> Self {
        Self {
            raw_base_rgba,
            source_light_direction,
        }
    }

    pub fn raw_base_rgba(&self) -> Vec4 {
        self.raw_base_rgba
    }
    pub fn source_light_direction(&self) -> Vec3 {
        self.source_light_direction
    }
}

#[derive(Component)]
pub struct TerrainWaterParticleEffects {
    pub quality: u32,
}

#[derive(Component)]
pub struct TerrainWaterSuspensionEmitter {
    pub emission_index: usize,
    pub initial_fill: bool,
    pub particle_radius: f32,
}

#[derive(Component)]
pub struct TerrainWaterBubbleEmitter {
    pub emitter: AssetId,
    pub emission_index: usize,
    pub remaining_seconds: f32,
}

/// A derived emission volume; terrain membership and source rows remain authoritative.
#[derive(Clone, Copy, Debug)]
pub struct TerrainWaterSuspensionVolume {
    pub origin: Vec3,
    pub size: Vec3,
}

impl TerrainWaterSuspensionVolume {
    pub fn volume_m3(self) -> f32 {
        self.size.x * self.size.y * self.size.z
    }
}
