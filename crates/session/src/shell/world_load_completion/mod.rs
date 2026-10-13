use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::world_spawn::world_hydration_completion::WorldLoadFinished;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailed;

use openzt2_interface_state::shell::shell_selection_types::ShellSelection;

pub(super) fn enter_game_or_return_to_map_selection_after_world_load_result(
    mut finished: MessageReader<WorldLoadFinished>,
    mut failed: MessageReader<WorldLoadFailed>,
    selection: Res<ShellSelection>,
    snapshot_loads: Query<(), With<crate::persistence::slot_io::pending_snapshot_application_types::PendingSnapshotApplication>>,
    mut next_phase: ResMut<NextState<GamePhase>>,
) {
    // Snapshot application owns the transition after its demand-loaded assets and
    // saved entities are ready; the baseline must not start simulating first.
    for result in finished.read() {
        if result.operation.is_none()
            && snapshot_loads.is_empty()
            && selection.scenario == Some(result.scenario)
        {
            next_phase.set(GamePhase::InGame);
        }
    }
    for result in failed.read() {
        if result.operation.is_none()
            && snapshot_loads.is_empty()
            && selection.scenario == Some(result.scenario)
        {
            next_phase.set(GamePhase::MapSelection);
        }
    }
}
