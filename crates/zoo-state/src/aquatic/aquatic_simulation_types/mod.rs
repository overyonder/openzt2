use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tank {
    pub definition: AssetId,
}

/// Marks a live tank whose source-authored surface offsets have been applied.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TankSurfaceHydrated;

/// Tank geometry with surface offsets already applied, including restored saves.
#[derive(Bundle, Debug, Clone, Copy, PartialEq)]
pub struct HydratedTankSurface {
    pub geometry: TankGeometry,
    pub offsets_applied: TankSurfaceHydrated,
}

impl HydratedTankSurface {
    pub const fn restored(geometry: TankGeometry) -> Self {
        Self {
            geometry,
            offsets_applied: TankSurfaceHydrated,
        }
    }
}

/// Marker consumed by show scheduling; eligibility is derived from the live
/// tank depth and immutable loaded depth policy.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShowTankEligible;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct TankGeometry {
    pub floor_height: f32,
    pub wall_height: f32,
    pub water_height: f32,
    pub area: f32,
    pub volume: f32,
}

impl TankGeometry {
    pub fn depth(self) -> f32 {
        (self.water_height - self.floor_height).max(0.0)
    }

    pub fn is_valid(self) -> bool {
        let derived_volume = self.area * self.depth();
        let volume_tolerance = derived_volume.abs().max(1.0) * f32::EPSILON * 8.0;
        self.floor_height.is_finite()
            && self.wall_height.is_finite()
            && self.water_height.is_finite()
            && self.area.is_finite()
            && self.volume.is_finite()
            && self.floor_height <= self.water_height
            && self.water_height <= self.wall_height
            && self.area >= 0.0
            && self.volume >= 0.0
            && (self.volume - derived_volume).abs() <= volume_tolerance
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WaterQuality(pub u16);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct TankCapacity {
    pub used: f32,
    pub required: f32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct AquaticAnimal {
    pub minimum_depth: f32,
    pub initial_space: f32,
    pub additional_space: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AquaticHome(pub Entity);

/// Current water facts. Source ranges live on `AquaticWaterRequirement`.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TankWater {
    pub salinity_permille: u16,
    pub temperature_c: i16,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AquaticWaterRequirement {
    pub salinity_permille: [u16; 2],
    pub temperature_c: [i16; 2],
    pub minimum_quality: u16,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LandWaterRequirement {
    pub land_fraction_permille: [u16; 2],
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TankEnvironment {
    pub land_fraction_permille: u16,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AquaticSuitability {
    pub depth: u16,
    pub space: u16,
    pub water: u16,
    pub land_water: u16,
    pub overall: u16,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TankFiltration {
    pub litres_per_zoo_day: u32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TankPopulation(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarinePlacementFailure {
    NotTank,
    InsufficientDepth,
    InsufficientSpace,
    IncompatibleWater,
    InvalidGeometry,
}
