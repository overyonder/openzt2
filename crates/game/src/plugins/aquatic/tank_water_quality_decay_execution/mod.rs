use bevy::prelude::*;

use crate::plugins::simulation_time::simulation_clock_types::ZooDayAdvanced;

use super::{
    aquatic_simulation_types::{Tank, TankCapacity, TankFiltration, WaterQuality},
    tank_water_quality_decay_calculation::calculate_tank_water_quality_after_one_day,
};

pub(super) fn decay_tank_water_quality_after_advanced_zoo_days(
    mut advanced_zoo_days: MessageReader<ZooDayAdvanced>,
    mut tanks: Query<(&TankCapacity, &TankFiltration, &mut WaterQuality), With<Tank>>,
) {
    for advanced_zoo_day in advanced_zoo_days.read() {
        if advanced_zoo_day.current_day <= advanced_zoo_day.previous_day {
            continue;
        }
        for (capacity, filtration, mut water_quality) in &mut tanks {
            *water_quality = calculate_tank_water_quality_after_one_day(
                *water_quality,
                capacity.used,
                filtration.litres_per_zoo_day,
            );
        }
    }
}
