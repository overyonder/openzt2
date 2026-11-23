use super::{fame_history_types::FameHistory, fame_types::Fame};
use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::simulation_time::simulation_clock_types::{ZooDayAdvanced, ZooMonth};
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted, world_membership_types::WorldRoot,
};

pub(super) fn record_completed_month_fame_history(
    mut days: MessageReader<ZooDayAdvanced>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut roots: Query<(&Fame, &mut FameHistory), (With<WorldRoot>, With<WorldLoadCompleted>)>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let epoch = &definitions.timing().calendar;
    let first_month =
        u32::from(epoch.epoch_year) * 12 + u32::from(epoch.epoch_month.saturating_sub(1));
    for day in days.read() {
        let Some(month) = day.completed_month else {
            continue;
        };
        let Ok((fame, mut history)) = roots.get_mut(day.root) else {
            continue;
        };
        record_completed_fame_month(&mut history, first_month, fame.half_stars, month);
    }
}

fn record_completed_fame_month(
    history: &mut FameHistory,
    first_month: u32,
    half_stars: u8,
    month: ZooMonth,
) {
    if let Some(month) = month.0.checked_sub(first_month) {
        history.record_monthly_value(month, half_stars);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_year_history_changes_only_when_its_month_finishes() {
        let mut history = FameHistory::default();
        for index in 0..17 {
            history.record_monthly_value(index, 2);
        }
        record_completed_fame_month(&mut history, 12, 10, ZooMonth(29));
        assert_eq!(history.samples().len(), 18);
        assert_eq!(history.value(5), Some(2));
        assert_eq!(history.value(17), Some(10));
    }
}
