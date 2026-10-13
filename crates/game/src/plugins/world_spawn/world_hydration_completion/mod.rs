use bevy::prelude::*;

use crate::plugins::simulation_time::simulation_time_world_lifecycle::remove_world_simulation_time_resources;
use openzt2_game_assets::asset_source::AssetArchives;

use super::{
    persistent_id_types::PersistentIdAllocator,
    world_hydration_types::WorldHydration,
    world_load_completion_marker::WorldLoadCompleted,
    world_load_failure::WorldLoadFailed,
    world_load_request_acceptance::WorldLoadOperationOwner,
    world_loading_performance_attribution::{
        WorldLoadingPerformanceAttribution, WorldLoadingPerformanceStage,
    },
    world_membership_types::WorldMember,
};

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorldLoadFinished {
    pub(crate) operation: Option<Entity>,
    pub(crate) scenario: openzt2_game_data::AssetId,
    pub(crate) root: Entity,
}

pub(super) fn complete_or_fail_finished_world_hydration(
    mut commands: Commands,
    archives: Res<AssetArchives>,
    mut performance: ResMut<WorldLoadingPerformanceAttribution>,
    pending: Query<(Entity, &WorldHydration, Option<&WorldLoadOperationOwner>)>,
    allocator: Option<Res<PersistentIdAllocator>>,
    members: Query<(Entity, &WorldMember)>,
    mut finished: MessageWriter<WorldLoadFinished>,
    mut failed: MessageWriter<WorldLoadFailed>,
) {
    let performance_timer = performance.measure(WorldLoadingPerformanceStage::CompletionCheck);
    let Ok((root, pending, operation)) = pending.single() else {
        return;
    };
    if let Some(reason) = pending.failure() {
        warn!(
            root = ?root,
            scenario = ?pending.selected_scenario_id(),
            hydration = ?pending,
            ?reason,
            "world hydration failed"
        );
        for (entity, member) in &members {
            if member.root == root {
                commands.entity(entity).despawn();
            }
        }
        commands.entity(root).despawn();
        if allocator
            .as_ref()
            .is_some_and(|allocator| allocator.owns_world(root))
        {
            commands.remove_resource::<PersistentIdAllocator>();
            remove_world_simulation_time_resources(&mut commands);
        }
        failed.write(WorldLoadFailed {
            operation: operation.map(|owner| owner.0),
            root: Some(root),
            scenario: pending.selected_scenario_id(),
            reason,
        });
        drop(performance_timer);
        performance.finish_and_report(&archives, "failed");
    } else if pending.is_complete() {
        commands.entity(root).insert(WorldLoadCompleted);
        finished.write(WorldLoadFinished {
            operation: operation.map(|owner| owner.0),
            scenario: pending.selected_scenario_id(),
            root,
        });
        commands.entity(root).remove::<WorldHydration>();
        drop(performance_timer);
        performance.finish_and_report(&archives, "completed");
    }
}
