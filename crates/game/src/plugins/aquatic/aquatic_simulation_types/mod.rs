use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tank {
    pub(crate) definition: AssetId,
}

/// Marks a live tank whose source-authored surface offsets have been applied.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TankSurfaceHydrated;

/// Tank geometry with surface offsets already applied, including restored saves.
#[derive(Bundle, Debug, Clone, Copy, PartialEq)]
pub(crate) struct HydratedTankSurface {
    pub(crate) geometry: TankGeometry,
    pub(crate) offsets_applied: TankSurfaceHydrated,
}

impl HydratedTankSurface {
    pub(crate) const fn restored(geometry: TankGeometry) -> Self {
        Self {
            geometry,
            offsets_applied: TankSurfaceHydrated,
        }
    }
}

/// Marker consumed by show scheduling; eligibility is derived from the live
/// tank depth and immutable loaded depth policy.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ShowTankEligible;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct TankGeometry {
    pub(crate) floor_height: f32,
    pub(crate) wall_height: f32,
    pub(crate) water_height: f32,
    pub(crate) area: f32,
    pub(crate) volume: f32,
}

impl TankGeometry {
    pub(crate) fn depth(self) -> f32 {
        (self.water_height - self.floor_height).max(0.0)
    }

    pub(crate) fn is_valid(self) -> bool {
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
pub(crate) struct WaterQuality(pub(crate) u16);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct TankCapacity {
    pub(crate) used: f32,
    pub(crate) required: f32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct AquaticAnimal {
    pub(crate) minimum_depth: f32,
    pub(crate) initial_space: f32,
    pub(crate) additional_space: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AquaticHome(pub(crate) Entity);

/// Current water facts. Source ranges live on `AquaticWaterRequirement`.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TankWater {
    pub(crate) salinity_permille: u16,
    pub(crate) temperature_c: i16,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AquaticWaterRequirement {
    pub(crate) salinity_permille: [u16; 2],
    pub(crate) temperature_c: [i16; 2],
    pub(crate) minimum_quality: u16,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LandWaterRequirement {
    pub(crate) land_fraction_permille: [u16; 2],
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TankEnvironment {
    pub(crate) land_fraction_permille: u16,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AquaticSuitability {
    pub(crate) depth: u16,
    pub(crate) space: u16,
    pub(crate) water: u16,
    pub(crate) land_water: u16,
    pub(crate) overall: u16,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TankFiltration {
    pub(crate) litres_per_zoo_day: u32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TankPopulation(pub(crate) u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarinePlacementFailure {
    NotTank,
    InsufficientDepth,
    InsufficientSpace,
    IncompatibleWater,
    InvalidGeometry,
}
