use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted, world_membership_types::WorldRoot,
};

use super::staff_job_types::UnsuccessfulStaffJobCandidates;

#[derive(Component)]
pub(super) struct StaffCandidateCleanupDeadline(u64);

pub(super) fn staff_candidate_cleanup_is_due(
    clock: Res<ZooClock>,
    roots: Query<
        Option<&StaffCandidateCleanupDeadline>,
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
) -> bool {
    roots
        .single()
        .is_ok_and(|deadline| clock.tick > deadline.map_or(0, |deadline| deadline.0))
}

pub(super) fn clear_unsuccessful_candidates_at_authored_manager_interval(
    mut commands: Commands,
    clock: Res<ZooClock>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut requests: Query<&mut UnsuccessfulStaffJobCandidates>,
    mut roots: Query<
        (Entity, Option<&mut StaffCandidateCleanupDeadline>),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let (Some(policy), Some(timing)) = (catalogue.staff_manager(), catalogue.simulation_timing())
    else {
        return;
    };
    let Ok((root, deadline)) = roots.single_mut() else {
        return;
    };
    let interval_ticks = (f64::from(policy.bad_entity_cleanup_interval)
        * f64::from(timing.fixed_hz))
    .max(0.0) as u64;
    let next_tick = clock.tick.saturating_add(interval_ticks);
    if let Some(mut deadline) = deadline {
        deadline.0 = next_tick;
    } else {
        commands
            .entity(root)
            .insert(StaffCandidateCleanupDeadline(next_tick));
    }
    for mut request in &mut requests {
        request.clear();
    }
}
