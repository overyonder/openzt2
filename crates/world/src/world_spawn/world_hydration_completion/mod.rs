use bevy::prelude::*;
use openzt2_game_assets::asset_source::AssetArchives;
use openzt2_interface_state::shell::shell_screen_presentation_types::LoadingScreen;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_world_state::simulation_time::simulation_time_resource_removal::remove_world_simulation_time_resources;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_hydration_completion::WorldLoadFinished;
use openzt2_world_state::world_spawn::world_hydration_types::WorldHydration;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailed;
use openzt2_world_state::world_spawn::world_load_request_acceptance::WorldLoadOperationOwner;
use openzt2_world_state::world_spawn::world_loading_performance_attribution::WorldLoadingPerformanceAttribution;
use openzt2_world_state::world_spawn::world_loading_performance_attribution::WorldLoadingPerformanceStage;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::{
    TerrainChunkRejected, TerrainRenderChunk,
};
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;

/// Wait for the loading document before cached hydration can occupy the frame.
pub(super) fn loading_screen_is_ready(
    asset_server: Res<AssetServer>,
    screens: Query<(), With<LoadingScreen>>,
    roots: Query<(&ChildOf, &UiDocumentRoot)>,
) -> bool {
    roots.iter().any(|(parent, root)| {
        screens.contains(parent.parent())
            && asset_server.is_loaded_with_dependencies(&root.document)
    })
}

pub fn complete_or_fail_finished_world_hydration(
    mut commands: Commands,
    archives: Res<AssetArchives>,
    mut performance: ResMut<WorldLoadingPerformanceAttribution>,
    pending: Query<(Entity, &WorldHydration, Option<&WorldLoadOperationOwner>)>,
    allocator: Option<Res<PersistentIdAllocator>>,
    members: Query<(Entity, &WorldMember)>,
    unprojected_terrain: Query<
        (),
        (
            With<TerrainChunk>,
            Without<TerrainRenderChunk>,
            Without<TerrainChunkRejected>,
        ),
    >,
    starting_animals: Query<
        &WorldMember,
        With<openzt2_world_state::world_spawn::starting_animal_hydration::PendingStartingAnimal>,
    >,
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
    } else if pending.is_complete()
        && unprojected_terrain.is_empty()
        && !starting_animals.iter().any(|member| member.root == root)
    {
        // Surface composition stalls frames for seconds; keep it behind the loading screen.
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
