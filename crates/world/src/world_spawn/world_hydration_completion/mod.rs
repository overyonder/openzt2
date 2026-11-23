use super::world_unloading::retire_world_root;
use bevy::prelude::*;
use openzt2_interface_state::shell::shell_screen_presentation_types::LoadingScreen;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::starting_animal_hydration::PendingStartingAnimal;
use openzt2_world_state::world_spawn::world_hydration_completion::WorldLoadFinished;
use openzt2_world_state::world_spawn::world_hydration_types::{
    PendingStartingAnimals, PendingTerrainProjections, StartingAnimalPendingFor,
    TerrainProjectionPendingFor, WorldHydration,
};
use openzt2_world_state::world_spawn::world_load_completion_marker::{
    StartingFenceTopologyHydrated, StartingPathsHydrated, StartingSpawnValuesApplied,
    StartingTerrainHydrated, WorldLoadCompleted,
};
use openzt2_world_state::world_spawn::world_load_failure::{WorldLoadFailed, WorldLoadFailure};
use openzt2_world_state::world_spawn::world_load_request_acceptance::WorldLoadOperationOwner;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::{
    TerrainChunkRejected, TerrainRenderChunk,
};

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

pub(super) fn conclude_starting_animal_projection(
    removed: On<Remove<PendingStartingAnimal>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<StartingAnimalPendingFor>();
}

pub(super) fn conclude_terrain_mesh_projection(
    added: On<Add<TerrainRenderChunk>>,
    mut commands: Commands,
) {
    commands
        .entity(added.entity)
        .try_remove::<TerrainProjectionPendingFor>();
}

pub(super) fn conclude_rejected_terrain_mesh_projection(
    added: On<Add<TerrainChunkRejected>>,
    mut commands: Commands,
) {
    commands
        .entity(added.entity)
        .try_remove::<TerrainProjectionPendingFor>();
}

pub fn complete_or_fail_finished_world_hydration(
    mut commands: Commands,
    failures: Query<
        (
            Entity,
            &WorldRoot,
            &WorldLoadFailure,
            Option<&WorldLoadOperationOwner>,
        ),
        With<WorldHydration>,
    >,
    completed_roots: Query<
        (Entity, &WorldRoot, Option<&WorldLoadOperationOwner>),
        (
            With<WorldHydration>,
            With<ZooClock>,
            With<StartingTerrainHydrated>,
            With<StartingPathsHydrated>,
            With<StartingFenceTopologyHydrated>,
            With<StartingSpawnValuesApplied>,
            Without<PendingTerrainProjections>,
            Without<PendingStartingAnimals>,
            Without<WorldLoadFailure>,
        ),
    >,
    mut finished: MessageWriter<WorldLoadFinished>,
    mut failed: MessageWriter<WorldLoadFailed>,
) {
    for (root, world, reason, operation) in &failures {
        warn!(?root, scenario = ?world.scenario, ?reason, "world hydration failed");
        retire_world_root(&mut commands, root);
        failed.write(WorldLoadFailed {
            operation: operation.map(|owner| owner.0),
            root: Some(root),
            scenario: world.scenario,
            reason: *reason,
        });
    }
    for (root, world, operation) in &completed_roots {
        commands
            .entity(root)
            .remove::<WorldHydration>()
            .insert(WorldLoadCompleted);
        finished.write(WorldLoadFinished {
            operation: operation.map(|owner| owner.0),
            scenario: world.scenario,
            root,
        });
    }
}
