use bevy::prelude::*;

use crate::{
    application_lifecycle::GamePhase,
    plugins::world_spawn::{
        world_hydration_completion::WorldLoadFinished, world_load_failure::WorldLoadFailed,
    },
};

use super::shell_selection_types::ShellSelection;

pub(super) fn enter_game_or_return_to_map_selection_after_world_load_result(
    mut finished: MessageReader<WorldLoadFinished>,
    mut failed: MessageReader<WorldLoadFailed>,
    selection: Res<ShellSelection>,
    mut next_phase: ResMut<NextState<GamePhase>>,
) {
    for result in finished.read() {
        if selection.scenario == Some(result.scenario) {
            next_phase.set(GamePhase::InGame);
        }
    }
    for result in failed.read() {
        if selection.scenario == Some(result.scenario) {
            next_phase.set(GamePhase::MapSelection);
        }
    }
}
