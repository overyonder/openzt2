use bevy::{prelude::*, time::Virtual};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::simulation_control_types::{
    AuthoredUiTemporarySimulationPause, SetSimulationSpeed, SimulationControl,
    SimulationControlChanged, SimulationPauseRequest,
};

pub(crate) fn apply_requested_simulation_control_changes(
    mut speed_requests: MessageReader<SetSimulationSpeed>,
    mut pause_requests: MessageReader<SimulationPauseRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut simulation_control: ResMut<SimulationControl>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut control_changes: MessageWriter<SimulationControlChanged>,
    speed_override: Res<super::simulation_speed_override::SimulationSpeedOverride>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let speed_multipliers = &definitions.timing().speed_multipliers;
    if speed_multipliers.is_empty()
        || usize::from(simulation_control.speed_tier) >= speed_multipliers.len()
    {
        return;
    }
    let previous_control = *simulation_control;

    for request in speed_requests.read() {
        accept_requested_simulation_speed_tier(
            &mut simulation_control,
            request.tier,
            speed_multipliers.len(),
        );
    }
    for request in pause_requests.read() {
        simulation_control.apply_pause_request(*request);
    }

    let speed_multiplier = &speed_multipliers[usize::from(simulation_control.speed_tier)];
    let relative_speed =
        speed_multiplier.numerator as f32 / speed_multiplier.denominator as f32 * speed_override.0;
    if virtual_time.relative_speed() != relative_speed || virtual_time.is_paused() {
        virtual_time.set_relative_speed(relative_speed);
        // Domain simulation sets use `simulation_is_running`. Bevy virtual time
        // must continue driving player-owned fixed transactions while paused.
        virtual_time.unpause();
    }
    if simulation_control.speed_tier == previous_control.speed_tier
        && simulation_control.paused == previous_control.paused
    {
        return;
    }
    control_changes.write(SimulationControlChanged {
        speed_tier: simulation_control.speed_tier,
        paused: simulation_control.paused,
    });
}

/// Reconcile the surviving authored holds together, after UI routing/commands
/// have applied transfers and document teardown.
pub(super) fn reconcile_authored_ui_temporary_pause_holds(
    temporary_holds: Query<(Entity, &AuthoredUiTemporarySimulationPause)>,
    parents: Query<&ChildOf>,
    visibility: Query<&Visibility>,
    owners: Query<()>,
    pending_roles: Res<
        openzt2_interface_state::ui::ui_document_lifecycle_contracts::UiRoleRequests,
    >,
    mut simulation_control: ResMut<SimulationControl>,
    mut control_changes: MessageWriter<SimulationControlChanged>,
) {
    let shown = |node| {
        std::iter::successors(Some(node), |entity| {
            parents.get(*entity).ok().map(ChildOf::parent)
        })
        .all(|entity| {
            !visibility
                .get(entity)
                .is_ok_and(|value| *value == Visibility::Hidden)
        })
    };
    let held = temporary_holds
        .iter()
        .any(|(node, intent)| intent.0 && shown(node))
        || pending_roles
            .temporary_pause_transfer_owners()
            .any(|owner| owners.contains(owner) && shown(owner));
    if simulation_control.temporary_ui_pause_hold_matches_current_control(held) {
        return;
    }
    if simulation_control.set_temporary_ui_pause_held(held) {
        control_changes.write(SimulationControlChanged {
            speed_tier: simulation_control.speed_tier,
            paused: simulation_control.paused,
        });
    }
}

pub(crate) fn accept_requested_simulation_speed_tier(
    simulation_control: &mut SimulationControl,
    requested_speed_tier: u8,
    speed_tier_count: usize,
) -> bool {
    if usize::from(requested_speed_tier) >= speed_tier_count
        || simulation_control.speed_tier == requested_speed_tier
    {
        return false;
    }
    simulation_control.speed_tier = requested_speed_tier;
    true
}
