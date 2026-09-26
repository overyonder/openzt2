use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;

use super::staff_job_types::UnsuccessfulStaffJobCandidates;

pub(super) fn clear_unsuccessful_candidates_at_authored_manager_interval(
    clock: Res<ZooClock>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut requests: Query<&mut UnsuccessfulStaffJobCandidates>,
    mut next_cleanup_tick: Local<u64>,
    mut previous_tick: Local<u64>,
) {
    if clock.tick < *previous_tick {
        *next_cleanup_tick = 0;
    }
    *previous_tick = clock.tick;
    // Native manager update uses a strict deadline comparison, including its
    // initial zero deadline. Keep this shared cadence separate from requests.
    if clock.tick <= *next_cleanup_tick {
        return;
    }
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(policy) = catalogue.staff_manager() else {
        return;
    };
    let Some(timing) = catalogue.simulation_timing() else {
        return;
    };
    let interval_ticks = (f64::from(policy.bad_entity_cleanup_interval)
        * f64::from(timing.fixed_hz))
    .max(0.0) as u64;
    *next_cleanup_tick = clock.tick.saturating_add(interval_ticks);
    for mut request in &mut requests {
        request.clear();
    }
}
