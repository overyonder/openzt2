use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    element_number, parse, required, required_element_number, required_number,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use crate::assets::world_definitions::timing::{
    lower_authored_timekeeper_to_simulation_timing_definition, AuthoredTimekeeperEvent,
    AuthoredTimekeeperEventKind,
};
use openzt2_game_data::world_definitions::simulation_time::{
    SimulationCalendarLeapPolicy, SimulationCalendarPolicy, SimulationSpeedMultiplier,
    SimulationTimingDefinition,
};

pub(super) fn bind_timing(
    record: &RecordView<'_, '_>,
    _output: &mut WorldDefinitionLoweringTables,
) -> Result<SimulationTimingDefinition, BindError> {
    if canonicalize_source_document_record_key(&record.semantic_type()) == "bftimekeeper" {
        let mut authored_events = Vec::new();
        for (name, kind) in [
            ("BFTIME_HEARTBEAT", AuthoredTimekeeperEventKind::Heartbeat),
            ("BFTIME_SIMTICK", AuthoredTimekeeperEventKind::Simulation),
            ("BFTIME_SLOWTICK", AuthoredTimekeeperEventKind::Slow),
        ] {
            let event = record
                .children_named(&[name])
                .first()
                .copied()
                .ok_or_else(|| {
                    BindError::record(record, format!("BFTimeKeeper is missing {name}"))
                })?;
            let seconds: f64 = required_element_number(&event, &["interval"])?;
            authored_events.push(AuthoredTimekeeperEvent {
                kind,
                interval_seconds: seconds,
            });
        }
        let time_scale: f32 = required_number(record, &["timeScale"])?;
        return lower_authored_timekeeper_to_simulation_timing_definition(
            f64::from(time_scale),
            &authored_events,
        )
        .map_err(|failure| {
            BindError::record(record, format!("invalid BFTimeKeeper policy: {failure:?}"))
        });
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
    Ok(SimulationTimingDefinition {
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
    })
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
