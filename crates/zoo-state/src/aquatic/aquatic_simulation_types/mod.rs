use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::habitat::habitat_types::HabitatRegion;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tank {
    pub definition: AssetId,
}

/// The topology cell that identifies a tank across rebuilt habitats, undo and
/// saves: the first cell of its sorted habitat region, resolved through the
/// habitat index.
pub fn tank_habitat_seed_cell(region: &HabitatRegion) -> Option<IVec2> {
    region.topology_cells.first().copied()
}

/// Height of a tank wall segment's top, in metres.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct TankWallTopHeight(pub f32);

/// Saved tank state waiting for its habitat to be derived from the loaded walls.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PendingTankRestoration {
    pub seed_cell: IVec2,
    pub definition: AssetId,
    pub geometry: TankGeometry,
    pub water_quality: WaterQuality,
    pub water_limit_missing: bool,
}

/// Tank images retained by a topology transaction while either side is absent.
#[derive(Component, Debug, Clone, Default)]
pub struct TankTopologyRestorations {
    pub created: Box<[PendingTankRestoration]>,
    pub removed: Box<[PendingTankRestoration]>,
}

/// A legacy saved tank whose water ceiling must be inferred after policy loading.
#[derive(Component)]
pub struct TankWaterLimitNeedsInitialization;

/// Marker consumed by show scheduling; eligibility is derived from the live
/// tank depth and immutable loaded depth policy.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShowTankEligible;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct TankGeometry {
    pub floor_height: f32,
    pub wall_height: f32,
    pub water_height: f32,
    /// Player water ceiling; walls may rise without refilling past this level.
    pub water_height_limit: f32,
    /// Lowest ground under the tank's walls, derived from terrain and the
    /// habitat boundary. The floor never rises above it.
    pub wall_base_height: f32,
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
            && self.water_height_limit.is_finite()
            && self.wall_base_height.is_finite()
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
