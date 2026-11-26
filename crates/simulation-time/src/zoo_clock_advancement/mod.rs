use bevy::prelude::*;
use openzt2_game_assets::world_definitions::timing::TimekeeperEvent;
use openzt2_world_state::simulation_time::simulation_control_types::SimulationPaused;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::environment::environment_state_types::ZooTickAdvanced;

use super::{
    simulation_clock_types::{
        ZooCalendar, ZooClock, ZooDay, ZooDayAdvanced, ZooDayCadence, ZooMonth,
        ZooTimekeeperElapsed, ZooYearAdvanced,
    },
    timekeeper_cadence::TimekeeperCadences,
    zoo_calendar_calculation::calculate_zoo_calendar_for_absolute_day,
};

pub(crate) fn advance_authoritative_zoo_clock_and_calendar(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    fixed_time: Res<Time<Fixed>>,
    mut cadence_events: MessageWriter<ZooTimekeeperElapsed>,
    mut roots: Query<
        (
            Entity,
            &mut ZooClock,
            &ZooDayCadence,
            &mut ZooCalendar,
            &mut ZooDay,
            &mut ZooMonth,
            &SimulationPaused,
            Option<&mut TimekeeperCadences>,
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
    let Ok((
        root,
        mut zoo_clock,
        cadence,
        mut zoo_calendar,
        mut zoo_day,
        mut zoo_month,
        paused,
        timekeeper,
    )) = roots.single_mut()
    else {
        return;
    };
    let mut advance_clock = || {
        let Some(next) = cadence.next_clock(*zoo_clock) else {
            return;
        };
        let previous_absolute_day = zoo_clock.absolute_day;
        zoo_clock.set_if_neq(next);
        advanced_ticks.write(ZooTickAdvanced(next.tick));
        for absolute_day in (previous_absolute_day..next.absolute_day).map(|day| day + 1) {
            let previous_calendar = *zoo_calendar;
            let current_calendar = calculate_zoo_calendar_for_absolute_day(
                &definitions.timing().calendar,
                absolute_day,
            );
            zoo_calendar.set_if_neq(current_calendar);
            zoo_day.set_if_neq(ZooDay(absolute_day));
            let previous_month = *zoo_month;
            let month_advanced = zoo_month.set_if_neq(ZooMonth::from(current_calendar));
            advanced_days.write(ZooDayAdvanced {
                root,
                previous_day: absolute_day - 1,
                current_day: absolute_day,
                completed_month: month_advanced.then_some(previous_month),
                calendar: current_calendar,
            });
            if current_calendar.year != previous_calendar.year {
                advanced_years.write(ZooYearAdvanced {
                    root,
                    previous_year: previous_calendar.year,
                    current_year: current_calendar.year,
                    calendar: current_calendar,
                });
            }
        }
    };
    if let Some(mut timekeeper) = timekeeper {
        for (event, count) in
            timekeeper.fixed_pulses(fixed_time.delta(), fixed_time.elapsed(), paused.0)
        {
            cadence_events.write(ZooTimekeeperElapsed { root, event, count });
            if event == TimekeeperEvent::Simulation {
                for _ in 0..count {
                    advance_clock();
                }
            }
        }
    } else if !paused.0 {
        advance_clock();
    }
}
