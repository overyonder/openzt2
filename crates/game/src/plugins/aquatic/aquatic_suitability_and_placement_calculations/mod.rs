use super::aquatic_simulation_types::{
    AquaticAnimal, AquaticSuitability, AquaticWaterRequirement, LandWaterRequirement,
    MarinePlacementFailure, TankCapacity, TankEnvironment, TankGeometry, TankWater, WaterQuality,
};

fn ratio_permille(available: f32, required: f32) -> u16 {
    if !available.is_finite() || !required.is_finite() || available < 0.0 || required < 0.0 {
        return 0;
    }
    if required == 0.0 {
        return 1000;
    }
    ((available / required) * 1000.0).clamp(0.0, 1000.0) as u16
}

fn range_suitability(value: i32, range: [i32; 2]) -> u16 {
    if range[0] > range[1] {
        return 0;
    }
    if (range[0]..=range[1]).contains(&value) {
        return 1000;
    }
    0
}

pub(super) fn calculate_aquatic_suitability(
    animal: AquaticAnimal,
    water_needs: AquaticWaterRequirement,
    land_needs: LandWaterRequirement,
    geometry: TankGeometry,
    capacity: TankCapacity,
    water: TankWater,
    quality: WaterQuality,
    environment: TankEnvironment,
) -> AquaticSuitability {
    if !geometry.is_valid() {
        return AquaticSuitability::default();
    }
    let depth = ratio_permille(geometry.depth(), animal.minimum_depth);
    let space = ratio_permille(geometry.volume, capacity.required.max(animal.initial_space));
    let salinity = range_suitability(
        i32::from(water.salinity_permille),
        water_needs.salinity_permille.map(i32::from),
    );
    let temperature = range_suitability(
        i32::from(water.temperature_c),
        water_needs.temperature_c.map(i32::from),
    );
    let quality = ratio_permille(f32::from(quality.0), f32::from(water_needs.minimum_quality));
    let water_score = salinity.min(temperature).min(quality);
    let land_water = range_suitability(
        i32::from(environment.land_fraction_permille),
        land_needs.land_fraction_permille.map(i32::from),
    );
    AquaticSuitability {
        depth,
        space,
        water: water_score,
        land_water,
        overall: depth.min(space).min(water_score).min(land_water),
    }
}

pub(crate) fn validate_marine_placement(
    animal: AquaticAnimal,
    water_needs: AquaticWaterRequirement,
    land_needs: LandWaterRequirement,
    geometry: TankGeometry,
    capacity: TankCapacity,
    water: TankWater,
    quality: WaterQuality,
    environment: TankEnvironment,
) -> Result<(), MarinePlacementFailure> {
    if !geometry.is_valid()
        || !animal.minimum_depth.is_finite()
        || !animal.initial_space.is_finite()
        || !animal.additional_space.is_finite()
        || animal.minimum_depth < 0.0
        || animal.initial_space < 0.0
        || animal.additional_space < 0.0
        || !capacity.used.is_finite()
        || !capacity.required.is_finite()
        || capacity.used < 0.0
        || capacity.required < 0.0
        || water_needs.salinity_permille[0] > water_needs.salinity_permille[1]
        || water_needs.temperature_c[0] > water_needs.temperature_c[1]
        || water_needs.minimum_quality > 1000
        || land_needs.land_fraction_permille[0] > land_needs.land_fraction_permille[1]
        || land_needs.land_fraction_permille[1] > 1000
        || quality.0 > 1000
        || environment.land_fraction_permille > 1000
    {
        return Err(MarinePlacementFailure::InvalidGeometry);
    }
    if geometry.depth() < animal.minimum_depth {
        return Err(MarinePlacementFailure::InsufficientDepth);
    }
    let next_space = if capacity.used <= 0.0 {
        animal.initial_space
    } else {
        animal.additional_space
    };
    if geometry.volume < capacity.required + next_space.max(0.0) {
        return Err(MarinePlacementFailure::InsufficientSpace);
    }
    let score = calculate_aquatic_suitability(
        animal,
        water_needs,
        land_needs,
        geometry,
        capacity,
        water,
        quality,
        environment,
    );
    if score.water < 1000 || score.land_water < 1000 {
        return Err(MarinePlacementFailure::IncompatibleWater);
    }
    Ok(())
}
