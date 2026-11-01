use super::{fame_history_types::FameHistory, fame_types::Fame};
use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::world_definitions::simulation_time::SimulationCalendarPolicy;
use openzt2_world_state::simulation_time::{
    simulation_clock_types::{ZooCalendar, ZooDayAdvanced},
    zoo_calendar_calculation::calculate_zoo_calendar_for_absolute_day,
};

pub(super) fn record_completed_month_fame_history(
    mut days: MessageReader<ZooDayAdvanced>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    fame: Res<Fame>,
    mut fame_history: ResMut<FameHistory>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    record_completed_fame_months(
        &mut fame_history,
        &definitions.timing().calendar,
        fame.half_stars,
        days.read(),
    );
}

fn record_completed_fame_months<'a>(
    history: &mut FameHistory,
    epoch: &SimulationCalendarPolicy,
    half_stars: u8,
    days: impl Iterator<Item = &'a ZooDayAdvanced>,
) {
    for day in days {
        let previous = calculate_zoo_calendar_for_absolute_day(epoch, day.previous_day);
        if (previous.year, previous.month) == (day.calendar.year, day.calendar.month) {
            continue;
        }
        if let Some(completed_month) =
            calculate_zero_based_zoo_month_index(day.calendar, epoch).checked_sub(1)
        {
            history.record_monthly_value(completed_month, half_stars);
        }
    }
}

fn calculate_zero_based_zoo_month_index(
    calendar: ZooCalendar,
    epoch: &SimulationCalendarPolicy,
) -> u32 {
    let month = u32::from(calendar.year) * 12 + u32::from(calendar.month.saturating_sub(1));
    let first_month =
        u32::from(epoch.epoch_year) * 12 + u32::from(epoch.epoch_month.saturating_sub(1));
    month.saturating_sub(first_month)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_year_history_changes_only_when_its_month_finishes() {
        let epoch = SimulationCalendarPolicy {
            epoch_year: 1, epoch_month: 1, epoch_day: 1,
            month_lengths: [31,28,31,30,31,30,31,31,30,31,30,31],
            leap: openzt2_game_data::world_definitions::simulation_time::SimulationCalendarLeapPolicy::Gregorian,
        };
        let mut history = FameHistory::default();
        for index in 0..17 {
            history.record_monthly_value(index, 2);
        }
        let unchanged = history.clone();
        record_completed_fame_months(&mut history, &epoch, 10, std::iter::empty());
        assert_eq!(history, unchanged);
        // Year 2's June starts at absolute day516 and ends at545.
        let daily = ZooDayAdvanced {
            previous_day: 538,
            current_day: 539,
            calendar: ZooCalendar {
                year: 2,
                month: 6,
                day: 24,
            },
        };
        record_completed_fame_months(&mut history, &epoch, 10, std::iter::once(&daily));
        assert_eq!(history, unchanged);
        let next_month = ZooDayAdvanced {
            previous_day: 545,
            current_day: 546,
            calendar: ZooCalendar {
                year: 2,
                month: 7,
                day: 1,
            },
        };
        record_completed_fame_months(&mut history, &epoch, 10, std::iter::once(&next_month));
        assert_eq!(history.samples().len(), 18);
        assert_eq!(history.value(5), Some(2));
        assert_eq!(history.value(17), Some(10));
    }
}
