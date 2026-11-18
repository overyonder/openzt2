use bevy::prelude::*;

use super::aquatic_simulation_types::{Tank, TankEnvironment, TankWater, WaterQuality};

pub(super) fn clamp_changed_normalized_tank_facts_to_permille_range(
    mut quality: Query<&mut WaterQuality, (With<Tank>, Changed<WaterQuality>)>,
    mut water: Query<&mut TankWater, (With<Tank>, Changed<TankWater>)>,
    mut environment: Query<&mut TankEnvironment, (With<Tank>, Changed<TankEnvironment>)>,
) {
    for mut quality in &mut quality {
        if quality.0 > 1000 {
            quality.0 = 1000;
        }
    }
    for mut water in &mut water {
        if water.salinity_permille > 1000 {
            water.salinity_permille = 1000;
        }
    }
    for mut environment in &mut environment {
        if environment.land_fraction_permille > 1000 {
            environment.land_fraction_permille = 1000;
        }
    }
}
