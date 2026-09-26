use bevy::{
    platform::collections::HashSet,
    prelude::{warn, Assets, Entity, Local, Query, Res, ResMut},
};

use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

use super::{
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    world_hydration_types::WorldHydration,
    world_load_failure::WorldLoadFailure,
    world_loading_performance_attribution::{
        WorldLoadingPerformanceAttribution, WorldLoadingPerformanceStage,
    },
};

pub(super) fn reserve_all_imported_world_persistent_ids(
    mut performance: ResMut<WorldLoadingPerformanceAttribution>,
    scenarios: Res<Assets<WorldScenarioDocumentAsset>>,
    allocator: Option<ResMut<PersistentIdAllocator>>,
    mut pending: Query<(Entity, &mut WorldHydration)>,
    mut reported_missing: Local<bool>,
) {
    let _performance_timer =
        performance.measure(WorldLoadingPerformanceStage::PersistentIdReservation);
    let Some(mut allocator) = allocator else {
        if !*reported_missing && !pending.is_empty() {
            warn!("world hydration is waiting for its persistent-id allocator");
            *reported_missing = true;
        }
        return;
    };
    *reported_missing = false;
    let Ok((root, mut pending)) = pending.single_mut() else {
        return;
    };
    if !pending.persistent_id_reservation_is_pending() {
        return;
    }
    let Some(asset) = scenarios.get(pending.starting_zoo_document_asset_handle()) else {
        pending.record_failure(WorldLoadFailure::MissingScenario);
        return;
    };
    let Some(start) = asset.document.find_starting_zoo(pending.starting_zoo_id()) else {
        pending.record_failure(WorldLoadFailure::InvalidRecord(0));
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
            pending.record_failure(WorldLoadFailure::DuplicatePersistentId(id.0));
            return;
        }
        if id.0 == 0 || id.0 == u64::MAX {
            pending.record_failure(WorldLoadFailure::InvalidRecord(index as u32));
            return;
        }
    }
    for (index, path) in paths.iter().enumerate() {
        let id = PersistentId(path.persistent_id);
        if !seen.insert(id.0) || id.0 == 0 || id.0 == u64::MAX {
            pending.record_failure(WorldLoadFailure::InvalidRecord(
                u32::try_from(records.len() + index).unwrap_or(u32::MAX),
            ));
            return;
        }
    }
    for (index, node) in topology_nodes.iter().enumerate() {
        let id = PersistentId(node.persistent_id);
        if !seen.insert(id.0) || id.0 == 0 || id.0 == u64::MAX {
            pending.record_failure(WorldLoadFailure::InvalidRecord(
                u32::try_from(records.len() + paths.len() + index).unwrap_or(u32::MAX),
            ));
            return;
        }
    }
    for (index, fence) in fences.iter().enumerate() {
        let id = PersistentId(fence.persistent_id);
        if !seen.insert(id.0) || id.0 == 0 || id.0 == u64::MAX {
            pending.record_failure(WorldLoadFailure::InvalidRecord(
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
            .reserve_imported(root, PersistentId(record.persistent_id))
            .expect("validated imported persistent ID");
    }
    for path in paths {
        allocator
            .reserve_imported(root, PersistentId(path.persistent_id))
            .expect("validated imported path persistent ID");
    }
    for node in topology_nodes {
        allocator
            .reserve_imported(root, PersistentId(node.persistent_id))
            .expect("validated imported topology-node persistent ID");
    }
    for fence in fences {
        allocator
            .reserve_imported(root, PersistentId(fence.persistent_id))
            .expect("validated imported fence persistent ID");
    }
    pending.record_persistent_id_reservation_completion();
}
