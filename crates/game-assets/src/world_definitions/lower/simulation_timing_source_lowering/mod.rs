use std::time::Duration;

use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    element_bool, element_number, parse, required, required_element_number, required_number,
};
use crate::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use crate::world_definitions::timing::{AuthoredTimekeeperPolicy, TimekeeperEventTiming};
use openzt2_game_data::world_definitions::simulation_time::{
    SimulationCalendarLeapPolicy, SimulationCalendarPolicy, SimulationSpeedMultiplier,
    SimulationTimingDefinition,
};

pub(super) fn bind_timing(
    record: &RecordView<'_, '_>,
    _output: &mut WorldDefinitionLoweringTables,
) -> Result<(SimulationTimingDefinition, Option<AuthoredTimekeeperPolicy>), BindError> {
    if canonicalize_source_document_record_key(&record.semantic_type()) == "bftimekeeper" {
        let event = |name: &str| -> Result<TimekeeperEventTiming, BindError> {
            let events = record.children_named(&[name]);
            let [event] = &events[..] else {
                return Err(BindError::record(
                    record,
                    format!("BFTimeKeeper requires one {name}"),
                ));
            };
            let seconds = required_element_number::<f64>(event, &["interval"])?;
            let interval = Duration::try_from_secs_f64(seconds)
                .ok()
                .filter(|duration| !duration.is_zero())
                .ok_or_else(|| BindError::record(record, format!("invalid {name} interval")))?;
            Ok(TimekeeperEventTiming {
                interval,
                pauses_with_simulation: element_bool(event, &["paused"], false)?,
            })
        };
        let time_scale: f64 = required_number(record, &["timeScale"])?;
        if !time_scale.is_finite() || time_scale <= 0.0 {
            return Err(BindError::record(record, "invalid BFTimeKeeper timeScale"));
        }
        let policy = AuthoredTimekeeperPolicy {
            heartbeat: event("BFTIME_HEARTBEAT")?,
            simulation: event("BFTIME_SIMTICK")?,
            slow: event("BFTIME_SLOWTICK")?,
            time_scale,
        };
        let seconds = policy.simulation.interval.as_secs_f64();
        let ticks_per_day = (86_400.0 / (seconds * time_scale)).round();
        if !(1.0..=f64::from(u32::MAX)).contains(&ticks_per_day) {
            return Err(BindError::record(
                record,
                "BFTimeKeeper zoo day exceeds tick range",
            ));
        }
        return Ok((
            SimulationTimingDefinition {
                fixed_hz: seconds.recip().round().clamp(1.0, f64::from(u16::MAX)) as u16,
                ticks_per_day: ticks_per_day as u32,
                speed_multipliers: vec![SimulationSpeedMultiplier {
                    numerator: 1,
                    denominator: 1,
                }],
                calendar: SimulationCalendarPolicy {
                    epoch_year: 1,
                    epoch_month: 1,
                    epoch_day: 1,
                    month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
                    leap: SimulationCalendarLeapPolicy::Gregorian,
                },
            },
            Some(policy),
        ));
    }
    let speed_multipliers = record
        .children_named(&["speed", "speedMultiplier"])
        .into_iter()
        .map(|speed| {
            Ok(SimulationSpeedMultiplier {
                numerator: element_number(&speed, &["numerator"], 1)?,
                denominator: element_number(&speed, &["denominator"], 1)?,
            })
        })
        .collect::<Result<Vec<_>, BindError>>()?;
    if speed_multipliers.is_empty() {
        return Err(BindError::record(
            record,
            "simulation timing contains no speed multipliers",
        ));
    }
    Ok((
        SimulationTimingDefinition {
            fixed_hz: required_number(record, &["fixedHz", "updatesPerSecond"])?,
            ticks_per_day: required_number(record, &["ticksPerDay"])?,
            speed_multipliers,
            calendar: SimulationCalendarPolicy {
                epoch_year: required_number(record, &["epochYear", "startYear"])?,
                epoch_month: required_number(record, &["epochMonth", "startMonth"])?,
                epoch_day: required_number(record, &["epochDay", "startDay"])?,
                month_lengths: parse_month_lengths(required(record, &["monthLengths"])?, record)?,
                leap: match canonicalize_source_document_record_key(
                    record.value(&["leapPolicy"]).unwrap_or("gregorian"),
                )
                .as_str()
                {
                    "none" => SimulationCalendarLeapPolicy::None,
                    "gregorian" => SimulationCalendarLeapPolicy::Gregorian,
                    value => {
                        return Err(BindError::record(
                            record,
                            format!("unknown leap policy {value}"),
                        ));
                    }
                },
            },
        },
        None,
    ))
}

pub(super) fn parse_month_lengths(
    value: &str,
    record: &RecordView<'_, '_>,
) -> Result<[u8; 12], BindError> {
    let values = value
        .split(|ch: char| ch == ',' || ch == ';' || ch.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| parse(part, record, "monthLengths"))
        .collect::<Result<Vec<u8>, _>>()?;
    values.try_into().map_err(|values: Vec<u8>| {
        BindError::record(
            record,
            format!("monthLengths needs 12 entries, found {}", values.len()),
        )
    })
}
