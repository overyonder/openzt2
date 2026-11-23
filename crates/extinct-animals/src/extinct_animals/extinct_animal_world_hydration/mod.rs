use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::simulation_time::deterministic_random_stream::RngDomain;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use crate::fossil_site_types::FossilSiteKind;
use openzt2_zoo_state::extinct_animals::{
    fossil_collection_and_assembly_types::{
        FossilAssemblyTable, FossilPlacementScope, FossilPlacementSurface,
    },
    fossil_recovery_types::{FossilDiscoveryRandomStream, FossilRecoverySite},
};

pub(super) fn hydrate_extinct_animal_world_state_from_loaded_definitions(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut definition_events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    seed: Single<&ZooSeed>,
    restored_sites: Query<Entity, With<FossilRecoverySite>>,
    sites: Query<
        (Entity, &PersistentId),
        (With<FossilSiteKind>, Without<FossilDiscoveryRandomStream>),
    >,
    mut tables: Query<(
        Entity,
        &FossilAssemblyTable,
        Option<&mut FossilPlacementSurface>,
    )>,
    changed_tables: Query<Entity, Changed<FossilAssemblyTable>>,
    mut removed_tables: RemovedComponents<FossilAssemblyTable>,
    mut roots: Query<(Entity, Option<&mut FossilPlacementScope>), With<WorldRoot>>,
    new_roots: Query<Entity, Added<WorldRoot>>,
) {
    let seed = seed.into_inner();
    for site in &restored_sites {
        commands
            .entity(site)
            .remove::<FossilRecoverySite>()
            .insert(FossilSiteKind::Puzzle);
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
    for entity in removed_tables.read() {
        if !tables.contains(entity) {
            commands
                .entity(entity)
                .try_remove::<FossilPlacementSurface>();
        }
    }

    let modified_asset = definition_events.read().any(|event| {
        matches!(
            event,
            AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
        )
    });
    definition_events.clear();
    let all = modified_asset || active_definitions.is_changed();
    if !all && changed_tables.is_empty() && new_roots.is_empty() {
        return;
    }
    let view = active_definitions.get(&definitions);
    let policy = view.and_then(|view| view.fossil_placement());
    if policy.is_none() && !active_definitions.is_complete() {
        return;
    }
    let root_entities: Vec<_> = if all {
        roots.iter().map(|(entity, _)| entity).collect()
    } else {
        new_roots.iter().collect()
    };
    let table_entities: Vec<_> = if all {
        tables.iter().map(|(entity, ..)| entity).collect()
    } else {
        changed_tables.iter().collect()
    };
    for root in root_entities {
        let Ok((_, scope)) = roots.get_mut(root) else {
            continue;
        };
        if let Some(policy) = policy {
            let next = FossilPlacementScope {
                puzzle_root: policy.puzzle_root,
                entity_root: policy.entity_root,
            };
            if let Some(mut scope) = scope {
                scope.set_if_neq(next);
            } else {
                commands.entity(root).insert(next);
            }
        } else if scope.is_some() {
            commands.entity(root).remove::<FossilPlacementScope>();
        }
    }
    for entity in table_entities {
        let Ok((_, table, surface)) = tables.get_mut(entity) else {
            continue;
        };
        if let Some(policy) = policy {
            let id = table.object_definition_identifier;
            let object = view.and_then(|view| view.find_object(id));
            let matches = |keys: &[openzt2_game_data::AssetId]| {
                keys.contains(&id)
                    || object.is_some_and(|object| {
                        object
                            .behavior_type_identifiers
                            .iter()
                            .any(|kind| keys.contains(kind))
                    })
            };
            let next = FossilPlacementSurface {
                accepts_fossil_pieces: !matches(&policy.non_placeable_objects)
                    && (matches(&policy.placeable_objects)
                        || object
                            .is_some_and(|object| object.authored_runtime.fossil_assembly_surface)),
            };
            if let Some(mut surface) = surface {
                surface.set_if_neq(next);
            } else {
                commands.entity(entity).insert(next);
            }
        } else if surface.is_some() {
            commands.entity(entity).remove::<FossilPlacementSurface>();
        }
    }
}
