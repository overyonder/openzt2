use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    required_number, simple_error, source_distance_cm,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::world_definitions::aquatic_habitats::{
    TankDepthPolicy, TankEditPolicy, TankSurfacePolicy,
};

pub(super) fn bind_tank_policy(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if output.document.tank_surface_policy.is_some() || output.document.tank_depth_policy.is_some()
    {
        return Err(BindError::record(
            record,
            "duplicate resolved ZTTankMgr policy",
        ));
    }
    let surface = TankSurfacePolicy {
        water_height_offset_cm: source_distance_cm(
            required_number(record, &["waterHeightOffset"])?,
            record,
        )?,
        floor_height_offset_cm: source_distance_cm(
            required_number(record, &["floorHeightOffset"])?,
            record,
        )?,
    };
    let depth = TankDepthPolicy {
        minimum_water_depth_cm: source_distance_cm(
            required_number(record, &["minimumWaterDepth"])?,
            record,
        )?
        .try_into()
        .map_err(|_| BindError::record(record, "negative minimumWaterDepth"))?,
        minimum_show_tank_depth_cm: source_distance_cm(
            required_number(record, &["minimumShowTankDepth"])?,
            record,
        )?
        .try_into()
        .map_err(|_| BindError::record(record, "negative minimumShowTankDepth"))?,
    };
    output.document.tank_surface_policy = Some(surface);
    output.document.tank_depth_policy = Some(depth);
    Ok(())
}

pub(super) fn bind_tank_edit_policy(
    record: &RecordView<'_, '_>,
) -> Result<TankEditPolicy, BindError> {
    let speeds_mps = [
        required_number(record, &["tankFloorSpeed"])?,
        required_number(record, &["tankWallSpeed"])?,
        required_number(record, &["tankFillDrainSpeed"])?,
    ];
    let speed_multipliers = [
        required_number(record, &["tankDynamicSpeedAdjFast"])?,
        required_number(record, &["tankDynamicSpeedAdjSlow"])?,
    ];
    let fill_speed_fraction = [
        required_number(record, &["tankDynamicFillSpeedStartPct"])?,
        required_number(record, &["tankDynamicFillSpeedEndPct"])?,
    ];
    let drain_speed_fraction = [
        required_number(record, &["tankDynamicDrainSpeedStartPct"])?,
        required_number(record, &["tankDynamicDrainSpeedEndPct"])?,
    ];
    let rate = |speed_mps: f64| {
        let millimetres = speed_mps * 1_000.0;
        if !millimetres.is_finite() || !(1.0..=f64::from(u32::MAX)).contains(&millimetres) {
            return Err(simple_error(
                "tank manipulation speed does not fit positive millimetres per second",
            ));
        }
        Ok(millimetres.round() as u32)
    };
    let multiplier = |value: f64| {
        if !value.is_finite() || value <= 0.0 {
            return Err(simple_error(
                "tank manipulation speed multiplier must be finite and positive",
            ));
        }
        let scaled = (value * 65_536.0).round();
        if !(1.0..=f64::from(u32::MAX)).contains(&scaled) {
            return Err(simple_error(
                "tank manipulation speed multiplier exceeds stored Q16",
            ));
        }
        Ok(scaled as u32)
    };
    let fraction = |value: f64| {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(simple_error(
                "tank manipulation dynamic-speed fraction is outside 0..=1",
            ));
        }
        Ok((value * 1_000.0).round() as u16)
    };
    Ok(TankEditPolicy {
        floor_mm_per_second: rate(speeds_mps[0])?,
        wall_mm_per_second: rate(speeds_mps[1])?,
        water_mm_per_second: rate(speeds_mps[2])?,
        fast_multiplier_q16: multiplier(speed_multipliers[0])?,
        slow_multiplier_q16: multiplier(speed_multipliers[1])?,
        fill_speed_fraction_permille: [
            fraction(fill_speed_fraction[0])?,
            fraction(fill_speed_fraction[1])?,
        ],
        drain_speed_fraction_permille: [
            fraction(drain_speed_fraction[0])?,
            fraction(drain_speed_fraction[1])?,
        ],
    })
}
