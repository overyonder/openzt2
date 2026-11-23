use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::environment::environment_state_types::ZooTickAdvanced;

use super::{
    simulation_clock_types::{
        ZooCalendar, ZooClock, ZooDay, ZooDayAdvanced, ZooMonth, ZooYearAdvanced,
    },
    zoo_calendar_calculation::calculate_zoo_calendar_for_absolute_day,
};

pub(crate) fn advance_authoritative_zoo_clock_and_calendar(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut roots: Query<
        (
            Entity,
            &mut ZooClock,
            &mut ZooCalendar,
            &mut ZooDay,
            &mut ZooMonth,
        ),
        With<WorldRoot>,
    >,
    mut advanced_ticks: MessageWriter<ZooTickAdvanced>,
    mut advanced_days: MessageWriter<ZooDayAdvanced>,
    mut advanced_years: MessageWriter<ZooYearAdvanced>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Ok((root, mut zoo_clock, mut zoo_calendar, mut zoo_day, mut zoo_month)) =
        roots.single_mut()
    else {
        return;
    };
    let previous_absolute_day = zoo_clock.absolute_day;
    if !advance_zoo_clock_by_one_tick(&mut zoo_clock, definitions.timing().ticks_per_day) {
        return;
    }
    advanced_ticks.write(ZooTickAdvanced(zoo_clock.tick));
    if zoo_clock.absolute_day == previous_absolute_day {
        return;
    }
    let previous_calendar = *zoo_calendar;
    *zoo_calendar = calculate_zoo_calendar_for_absolute_day(
        &definitions.timing().calendar,
        zoo_clock.absolute_day,
    );
    zoo_day.set_if_neq(ZooDay(zoo_clock.absolute_day));
    let previous_month = *zoo_month;
    let month_advanced = zoo_month.set_if_neq(ZooMonth::from(*zoo_calendar));
    advanced_days.write(ZooDayAdvanced {
        root,
        previous_day: previous_absolute_day,
        current_day: zoo_clock.absolute_day,
        completed_month: month_advanced.then_some(previous_month),
        calendar: *zoo_calendar,
    });
    if zoo_calendar.year != previous_calendar.year {
        advanced_years.write(ZooYearAdvanced {
            root,
            previous_year: previous_calendar.year,
            current_year: zoo_calendar.year,
            calendar: *zoo_calendar,
        });
    }
}

pub(crate) fn advance_zoo_clock_by_one_tick(zoo_clock: &mut ZooClock, ticks_per_day: u32) -> bool {
    if ticks_per_day == 0 || zoo_clock.tick_in_day >= ticks_per_day {
        return false;
    }
    let Some(next_tick) = zoo_clock.tick.checked_add(1) else {
        return false;
    };
    zoo_clock.tick = next_tick;
    zoo_clock.tick_in_day += 1;
    if zoo_clock.tick_in_day == ticks_per_day {
        let Some(next_absolute_day) = zoo_clock.absolute_day.checked_add(1) else {
            zoo_clock.tick_in_day -= 1;
            zoo_clock.tick -= 1;
            return false;
        };
        zoo_clock.tick_in_day = 0;
        zoo_clock.absolute_day = next_absolute_day;
    }
    true
}
