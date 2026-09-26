use bevy::prelude::*;

use crate::asset_source::AssetArchives;

use super::{
    persistent_id_types::PersistentIdAllocator,
    world_hydration_types::WorldHydration,
    world_load_completion_marker::WorldLoadCompleted,
    world_load_failure::WorldLoadFailed,
    world_loading_performance_attribution::{
        WorldLoadingPerformanceAttribution, WorldLoadingPerformanceStage,
    },
    world_membership_types::WorldMember,
};

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorldLoadFinished {
    pub(crate) scenario: openzt2_game_data::AssetId,
    pub(crate) root: Entity,
}

pub(super) fn complete_or_fail_finished_world_hydration(
    mut commands: Commands,
    archives: Res<AssetArchives>,
    mut performance: ResMut<WorldLoadingPerformanceAttribution>,
    pending: Query<(Entity, &WorldHydration)>,
    members: Query<(Entity, &WorldMember)>,
    mut finished: MessageWriter<WorldLoadFinished>,
    mut failed: MessageWriter<WorldLoadFailed>,
) {
    let performance_timer = performance.measure(WorldLoadingPerformanceStage::CompletionCheck);
    let Ok((root, pending)) = pending.single() else {
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
        commands.remove_resource::<PersistentIdAllocator>();
        failed.write(WorldLoadFailed {
            scenario: pending.selected_scenario_id(),
            reason,
        });
        drop(performance_timer);
        performance.finish_and_report(&archives, "failed");
    } else if pending.is_complete() {
        commands.entity(root).insert(WorldLoadCompleted);
        finished.write(WorldLoadFinished {
            scenario: pending.selected_scenario_id(),
            root,
        });
        commands.entity(root).remove::<WorldHydration>();
        drop(performance_timer);
        performance.finish_and_report(&archives, "completed");
    }
}
