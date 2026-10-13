use super::{
    aquatic_simulation_types::WaterQuality,
    tank_water_quality_decay_calculation::calculate_tank_water_quality_after_one_day,
};

#[test]
fn tank_water_quality_decay_is_daily_and_saturating() {
    assert_eq!(
        calculate_tank_water_quality_after_one_day(WaterQuality(900), 1.0, 1000),
        WaterQuality(900)
    );
    assert_eq!(
        calculate_tank_water_quality_after_one_day(WaterQuality(900), 1.0, 750),
        WaterQuality(650)
    );
    assert_eq!(
        calculate_tank_water_quality_after_one_day(WaterQuality(100), 1.0, 0),
        WaterQuality(0)
    );
}
