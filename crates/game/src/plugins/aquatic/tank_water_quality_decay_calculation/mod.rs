use super::aquatic_simulation_types::WaterQuality;

/// Removes the unfiltered share of one day's biological load.
pub(super) fn calculate_tank_water_quality_after_one_day(
    current_water_quality: WaterQuality,
    used_cubic_metres: f32,
    filtered_litres: u32,
) -> WaterQuality {
    let used_litres = (used_cubic_metres.max(0.0) * 1000.0)
        .ceil()
        .min(u32::MAX as f32) as u32;
    if used_litres == 0 || filtered_litres >= used_litres {
        return WaterQuality(current_water_quality.0.min(1000));
    }
    let unfiltered_litres = used_litres - filtered_litres;
    let water_quality_loss = ((u64::from(unfiltered_litres) * 1000 + u64::from(used_litres) - 1)
        / u64::from(used_litres)) as u16;
    WaterQuality(
        current_water_quality
            .0
            .min(1000)
            .saturating_sub(water_quality_loss),
    )
}
