use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;
use crate::plugins::simulation_time::deterministic_random_stream::RngDomain;
use crate::plugins::simulation_time::deterministic_random_stream::ZooSeed;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    fossil_collection_and_assembly_types::{
        CollectedFossilPieceInventory, FossilAssemblyTable, FossilPlacementScope,
        FossilPlacementSurface,
    },
    fossil_recovery_types::{FossilDiscoveryRandomStream, FossilRecoverySite},
};

pub(super) fn hydrate_extinct_animal_world_state_from_loaded_definitions(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    seed: Res<ZooSeed>,
    mut collection: ResMut<CollectedFossilPieceInventory>,
    sites: Query<
        (Entity, &PersistentId),
        (
            With<FossilRecoverySite>,
            Without<FossilDiscoveryRandomStream>,
        ),
    >,
    tables: Query<(
        Entity,
        &FossilAssemblyTable,
        Option<&FossilPlacementSurface>,
    )>,
    roots: Query<Entity, (With<WorldRoot>, Without<FossilPlacementScope>)>,
    mut completed_definition_set_has_no_fossil_placement_policy: Local<bool>,
) {
    if active_definitions.is_changed() {
        *completed_definition_set_has_no_fossil_placement_policy = false;
    }
    if !collection.is_uninitialized()
        && sites.is_empty()
        && tables.iter().all(|(_, _, surface)| surface.is_some())
        && (roots.is_empty() || *completed_definition_set_has_no_fossil_placement_policy)
    {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    if collection.is_uninitialized() {
        let Ok(piece_count) = u32::try_from(definitions.fossil_pieces().count()) else {
            return;
        };
        *collection = CollectedFossilPieceInventory::with_global_fossil_piece_count(piece_count);
    }
    if let Some(policy) = definitions.fossil_placement() {
        for root in &roots {
            commands.entity(root).insert(FossilPlacementScope {
                puzzle_root: policy.puzzle_root,
                entity_root: policy.entity_root,
            });
        }
        let authored_placeable_object_identifiers = &policy.placeable_objects;
        let authored_non_placeable_object_identifiers = &policy.non_placeable_objects;
        for (entity, table, surface) in &tables {
            if surface.is_some() {
                continue;
            }
            let accepts_pieces = authored_placeable_object_identifiers
                .iter()
                .any(|candidate| candidate.0 == table.object_definition_identifier.0)
                && !authored_non_placeable_object_identifiers
                    .iter()
                    .any(|candidate| candidate.0 == table.object_definition_identifier.0);
            commands.entity(entity).insert(FossilPlacementSurface {
                accepts_fossil_pieces: accepts_pieces,
            });
        }
    } else if active_definitions.is_complete() {
        *completed_definition_set_has_no_fossil_placement_policy = true;
    }
    for (entity, persistent_identifier) in &sites {
        commands
            .entity(entity)
            .insert(FossilDiscoveryRandomStream(DeterministicRng::from_entity(
                *seed,
                *persistent_identifier,
                RngDomain::Fossil,
            )));
    }
}
