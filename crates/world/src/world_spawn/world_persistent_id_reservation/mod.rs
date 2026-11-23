use bevy::platform::collections::HashSet;
use bevy::prelude::Assets;
use bevy::prelude::Commands;
use bevy::prelude::Entity;
use bevy::prelude::Query;
use bevy::prelude::Res;
use bevy::prelude::With;
use bevy::prelude::Without;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::selected_world_document_handles::SelectedWorldDocumentHandles;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_load_completion_marker::ImportedPersistentIdsReserved;

use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_hydration_types::WorldHydration;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailure;

pub fn reserve_all_imported_world_persistent_ids(
    mut commands: Commands,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    mut pending: Query<
        (
            Entity,
            &SelectedWorldIdentity,
            &SelectedWorldDocumentHandles,
            &mut PersistentIdAllocator,
        ),
        (
            With<WorldHydration>,
            With<ZooClock>,
            Without<ImportedPersistentIdsReserved>,
            Without<WorldLoadFailure>,
        ),
    >,
) {
    let Ok((root, selected_world, documents, mut allocator)) = pending.single_mut() else {
        return;
    };
    let Some(asset) = scenarios.get(&documents.start) else {
        commands
            .entity(root)
            .insert_if_new(WorldLoadFailure::MissingScenario);
        return;
    };
    let Some(start) = asset.document.find_starting_zoo(selected_world.start) else {
        commands
            .entity(root)
            .insert_if_new(WorldLoadFailure::InvalidRecord(0));
        return;
    };
    let records = start.entities.as_slice();
    let paths = start.paths.as_slice();
    let topology_nodes = start.topology_nodes.as_slice();
    let fences = start.fences.as_slice();

    let mut seen =
        HashSet::with_capacity(records.len() + paths.len() + topology_nodes.len() + fences.len());
    for (index, record) in records.iter().enumerate() {
        let id = PersistentId(record.persistent_id);
        if !seen.insert(id.0) {
            commands
                .entity(root)
                .insert_if_new(WorldLoadFailure::DuplicatePersistentId(id.0));
            return;
        }
        if id.0 == 0 || id.0 == u64::MAX {
            commands
                .entity(root)
                .insert_if_new(WorldLoadFailure::InvalidRecord(index as u32));
            return;
        }
    }
    for (index, path) in paths.iter().enumerate() {
        let id = PersistentId(path.persistent_id);
        if !seen.insert(id.0) || id.0 == 0 || id.0 == u64::MAX {
            commands
                .entity(root)
                .insert_if_new(WorldLoadFailure::InvalidRecord(
                    u32::try_from(records.len() + index).unwrap_or(u32::MAX),
                ));
            return;
        }
    }
    for (index, node) in topology_nodes.iter().enumerate() {
        let id = PersistentId(node.persistent_id);
        if !seen.insert(id.0) || id.0 == 0 || id.0 == u64::MAX {
            commands
                .entity(root)
                .insert_if_new(WorldLoadFailure::InvalidRecord(
                    u32::try_from(records.len() + paths.len() + index).unwrap_or(u32::MAX),
                ));
            return;
        }
    }
    for (index, fence) in fences.iter().enumerate() {
        let id = PersistentId(fence.persistent_id);
        if !seen.insert(id.0) || id.0 == 0 || id.0 == u64::MAX {
            commands
                .entity(root)
                .insert_if_new(WorldLoadFailure::InvalidRecord(
                    u32::try_from(records.len() + paths.len() + topology_nodes.len() + index)
                        .unwrap_or(u32::MAX),
                ));
            return;
        }
    }
    for record in records {
        // Every row was validated above, so this second pass is the first
        // allocator mutation and cannot leave a partially accepted import.
        allocator
            .reserve_imported(PersistentId(record.persistent_id))
            .expect("validated imported persistent ID");
    }
    for path in paths {
        allocator
            .reserve_imported(PersistentId(path.persistent_id))
            .expect("validated imported path persistent ID");
    }
    for node in topology_nodes {
        allocator
            .reserve_imported(PersistentId(node.persistent_id))
            .expect("validated imported topology-node persistent ID");
    }
    for fence in fences {
        allocator
            .reserve_imported(PersistentId(fence.persistent_id))
            .expect("validated imported fence persistent ID");
    }
    commands.entity(root).insert(ImportedPersistentIdsReserved);
}
