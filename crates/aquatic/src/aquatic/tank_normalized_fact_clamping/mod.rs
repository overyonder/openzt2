use bevy::prelude::*;

use super::aquatic_simulation_types::{Tank, TankEnvironment, TankWater, WaterQuality};

pub(super) fn clamp_changed_normalized_tank_facts_to_permille_range(
    mut tanks: Query<
        (&mut WaterQuality, &mut TankWater, &mut TankEnvironment),
        (
            With<Tank>,
            Or<(
                Changed<WaterQuality>,
                Changed<TankWater>,
                Changed<TankEnvironment>,
            )>,
        ),
    >,
) {
    for (mut water_quality, mut tank_water, mut tank_environment) in &mut tanks {
        water_quality.0 = water_quality.0.min(1000);
        tank_water.salinity_permille = tank_water.salinity_permille.min(1000);
        tank_environment.land_fraction_permille = tank_environment.land_fraction_permille.min(1000);
    }
}
