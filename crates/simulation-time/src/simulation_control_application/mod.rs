use bevy::{prelude::*, time::Virtual};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::simulation_control_types::{
    SetSimulationSpeed, SimulationControl, SimulationControlChanged, SimulationPauseRequest,
    SimulationPaused, SimulationSpeedTier,
};
use super::simulation_speed_override::SimulationSpeedOverride;

pub(super) fn apply_simulation_control_requests(
    mut speed_requests: MessageReader<SetSimulationSpeed>,
    mut pause_requests: MessageReader<SimulationPauseRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut roots: Query<
        (
            &mut SimulationControl,
            &mut SimulationSpeedTier,
            &mut SimulationPaused,
        ),
        With<WorldRoot>,
    >,
    mut changes: MessageWriter<SimulationControlChanged>,
) {
    let Ok((mut control, mut tier, mut paused)) = roots.single_mut() else {
        return;
    };
    let mut next_control = *control;
    let speed_count = active_definitions
        .get(&definitions)
        .map_or(0, |definitions| {
            definitions.timing().speed_multipliers.len()
        });
    let requested_tier = speed_requests
        .read()
        .filter(|request| usize::from(request.tier) < speed_count)
        .last()
        .map_or(*tier, |request| SimulationSpeedTier(request.tier));
    for request in pause_requests.read() {
        next_control.apply_pause_request(*request);
    }
    let tier_changed = tier.set_if_neq(requested_tier);
    let pause_changed = paused.set_if_neq(next_control.effective_pause());
    control.set_if_neq(next_control);
    if tier_changed || pause_changed {
        changes.write(SimulationControlChanged {
            speed_tier: tier.0,
            paused: paused.0,
        });
    }
}

pub(super) fn simulation_speed_inputs_changed(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    tiers: Query<(), (With<WorldRoot>, Changed<SimulationSpeedTier>)>,
    speed_override: Res<SimulationSpeedOverride>,
) -> bool {
    active_definitions.is_changed()
        || definitions.is_changed()
        || speed_override.is_changed()
        || !tiers.is_empty()
}

pub(super) fn project_simulation_speed(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    roots: Query<&SimulationSpeedTier, With<WorldRoot>>,
    mut virtual_time: ResMut<Time<Virtual>>,
    speed_override: Res<SimulationSpeedOverride>,
) {
    let Ok(tier) = roots.single() else {
        return;
    };
    let Some(speed) = active_definitions
        .get(&definitions)
        .and_then(|definitions| {
            definitions
                .timing()
                .speed_multipliers
                .get(usize::from(tier.0))
        })
    else {
        return;
    };
    let relative_speed =
        f32::from(speed.numerator) / f32::from(speed.denominator) * speed_override.0;
    if virtual_time.relative_speed() != relative_speed {
        virtual_time.set_relative_speed(relative_speed);
    }
    // Paused worlds still run fixed player-owned transactions.
    if virtual_time.is_paused() {
        virtual_time.unpause();
    }
}
