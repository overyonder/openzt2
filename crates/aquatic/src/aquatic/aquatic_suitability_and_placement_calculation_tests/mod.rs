use super::{
    aquatic_simulation_types::{
        AquaticAnimal, AquaticSuitability, AquaticWaterRequirement, LandWaterRequirement,
        TankCapacity, TankEnvironment, TankGeometry, TankWater, WaterQuality,
    },
    aquatic_suitability_and_placement_calculations::calculate_aquatic_suitability,
};

fn valid_test_tank_geometry() -> TankGeometry {
    TankGeometry {
        floor_height: 0.0,
        wall_height: 5.0,
        water_height: 2.0,
        area: 10.0,
        volume: 20.0,
    }
}

#[test]
fn aquatic_suitability_combines_depth_space_water_and_land_facts() {
    let suitability = calculate_aquatic_suitability(
        AquaticAnimal {
            minimum_depth: 1.0,
            initial_space: 10.0,
            additional_space: 5.0,
        },
        AquaticWaterRequirement {
            salinity_permille: [30, 40],
            temperature_c: [10, 20],
            minimum_quality: 800,
        },
        LandWaterRequirement {
            land_fraction_permille: [0, 100],
        },
        valid_test_tank_geometry(),
        TankCapacity {
            used: 10.0,
            required: 10.0,
        },
        TankWater {
            salinity_permille: 35,
            temperature_c: 15,
        },
        WaterQuality(1000),
        TankEnvironment {
            land_fraction_permille: 50,
        },
    );
    assert_eq!(
        suitability,
        AquaticSuitability {
            depth: 1000,
            space: 1000,
            water: 1000,
            land_water: 1000,
            overall: 1000,
        }
    );
}
