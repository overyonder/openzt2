use bevy::{ecs::relationship::RelationshipTarget, platform::collections::HashSet, prelude::*};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    simulation_time::deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed},
    world_spawn::{
        persistent_id_types::{PersistentId, PersistentIdAllocator},
        prefab_presentation_types::PrefabPresentation,
        world_load_completion_marker::WorldLoadCompleted,
        world_membership_types::{DefinitionId, WorldMember, WorldMembers, WorldRoot},
    },
};
use openzt2_zoo_state::{
    extinct_animals::{
        fossil_collection_and_assembly_types::{
            CollectedFossilPiece, FossilAssemblyTable, WorldFossilPieces,
        },
        fossil_recovery_types::{
            FossilDiscoveryLevel, FossilDiscoveryRandomStream, FossilRecoverySite,
        },
    },
    placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex,
    progression::fame_types::Fame,
    terrain::{
        dry_surface_eligibility::DrySurfaceEligibility,
        terrain_chunk_types::{TerrainChunk, TerrainIndex},
    },
    topology::topology_graph_types::TopologyGrid,
};

use super::fossil_definition_index::FossilDefinitionIndex;

pub(super) fn bind_live_fossil_assembly_tables(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut entities: Query<
        (Entity, Ref<DefinitionId>, Option<&mut FossilAssemblyTable>),
        With<WorldMember>,
    >,
) {
    let changed = definitions.is_changed() || events.read().next().is_some();
    events.clear();
    let Some(view) = definitions.get(&assets) else {
        return;
    };
    for (entity, definition, table) in &mut entities {
        if !changed && !definition.is_changed() {
            continue;
        }
        if view
            .find_object(definition.0)
            .is_some_and(|object| object.authored_runtime.fossil_assembly_surface)
        {
            let next = FossilAssemblyTable {
                object_definition_identifier: definition.0,
            };
            if let Some(mut table) = table {
                table.set_if_neq(next);
            } else {
                commands.entity(entity).insert(next);
            }
        } else if definitions.is_complete() && table.is_some() {
            commands.entity(entity).remove::<FossilAssemblyTable>();
        }
    }
}

pub(super) fn spawn_world_fossil_recovery_sites(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    catalogue: Res<FossilDefinitionIndex>,
    seed: Res<ZooSeed>,
    mut worlds: Query<
        (
            Entity,
            &WorldRoot,
            Option<&WorldMembers>,
            Option<Ref<WorldFossilPieces>>,
            Option<Ref<Fame>>,
            Option<&mut FossilDiscoveryLevel>,
            &mut PersistentIdAllocator,
        ),
        With<WorldLoadCompleted>,
    >,
    chunks: Query<(&WorldMember, &TerrainChunk)>,
    terrain_index: Res<TerrainIndex>,
    ground: DrySurfaceEligibility,
    grid: Res<TopologyGrid>,
    occupied: Res<PlacedObjectFootprintOccupancyIndex>,
    existing: Query<(Entity, &FossilRecoverySite, &Transform)>,
    discovered: Query<&CollectedFossilPiece>,
) {
    let Some(view) = definitions.get(&assets) else {
        return;
    };
    if view.fossil_placement().is_none() || catalogue.pieces.is_empty() {
        return;
    }
    let Some(marker) = view.find_object(AssetId::from_key("fossilsite")) else {
        return;
    };
    let Some(presentation) = view.scene(marker.prefab) else {
        return;
    };
    for (root, world, members, pieces, fame, progress, mut ids) in &mut worlds {
        if progress.is_some()
            && !catalogue.is_changed()
            && !fame.as_ref().is_some_and(|fame| fame.is_changed())
            && !pieces.as_ref().is_some_and(|pieces| pieces.is_changed())
        {
            continue;
        }
        let found = discovered
            .iter_many(
                pieces
                    .as_deref()
                    .into_iter()
                    .flat_map(RelationshipTarget::iter),
            )
            .flatten()
            .map(|piece| piece.0)
            .collect::<HashSet<_>>();
        let mut set_ids = HashSet::<AssetId>::default();
        let sets: Vec<_> = catalogue
            .pieces
            .iter()
            .filter_map(|piece| {
                if set_ids.insert(piece.set) {
                    view.find_fossil_set(piece.set)
                } else {
                    None
                }
            })
            .collect();
        let previous = progress.as_deref().map_or(0, |progress| progress.0);
        let complete = |set: &&openzt2_game_data::world_definitions::extinct_animal_recovery::FossilSetDefinition| {
            set.pieces.iter().all(|piece| found.contains(piece))
        };
        if !sets
            .iter()
            .filter(|set| set.discovery_level == previous)
            .all(complete)
        {
            continue;
        }
        let fame = fame.as_deref().map_or(0, |fame| fame.half_stars);
        let level = if fame == 8 { 9 } else { fame }.div_ceil(2).max(1);
        let next = previous.saturating_add(1);
        if next > level {
            continue;
        }
        let candidates: Vec<_> = sets
            .into_iter()
            .filter(|set| set.discovery_level == next && !complete(set))
            .collect();
        if candidates.is_empty() {
            continue;
        }
        let mut minimum = Vec2::splat(f32::INFINITY);
        let mut maximum = Vec2::splat(f32::NEG_INFINITY);
        for (membership, chunk) in chunks
            .iter_many(terrain_index.chunks.values().copied())
            .flatten()
        {
            if membership.root == root {
                minimum = minimum.min(chunk.origin);
                maximum = maximum
                    .max(chunk.origin + Vec2::splat(f32::from(chunk.side - 1) * chunk.spacing_m));
            }
        }
        let size = ((maximum - minimum) / grid.spacing_m).floor().as_uvec2();
        if size.x == 0 || size.y == 0 {
            continue;
        }
        let mut cells = HashSet::<IVec2>::default();
        for (entity, site, pose) in existing
            .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
            .flatten()
        {
            if candidates
                .iter()
                .any(|set| set.id == site.fossil_set_definition_identifier)
            {
                cells.insert(grid.position_cell(pose.translation).truncate());
            } else {
                commands.entity(entity).try_despawn();
            }
        }
        let world_seed = u64::from_le_bytes(
            world.scenario.0[..8]
                .try_into()
                .expect("asset identifier has sixteen bytes"),
        );
        let mut random = DeterministicRng::from_entity(
            *seed,
            PersistentId(world_seed ^ u64::from(next)),
            RngDomain::Fossil,
        );
        for set in candidates {
            let missing = set
                .pieces
                .iter()
                .filter(|piece| !found.contains(*piece))
                .count();
            let required = missing.div_ceil(usize::from(set.pieces_per_location.max(1)));
            for _ in 0..required {
                let position = (0..1000).find_map(|_| {
                    let x = random.range_u32(size.x)?;
                    let z = random.range_u32(size.y)?;
                    let location = minimum
                        + (Vec2::new(x as f32, z as f32) + Vec2::splat(0.5)) * grid.spacing_m;
                    let height = ground.ground_height(location)?;
                    let position = Vec3::new(location.x, height, location.y);
                    let cell = grid.position_cell(position).truncate();
                    if cells.contains(&cell)
                        || occupied.entities_occupying_cell(cell).next().is_some()
                    {
                        return None;
                    }
                    cells.insert(cell);
                    Some(position)
                });
                let Some(position) = position else {
                    continue;
                };
                let Ok(id) = ids.allocate() else {
                    break;
                };
                commands.spawn((
                    WorldMember { root },
                    DefinitionId(marker.id),
                    id,
                    FossilRecoverySite {
                        fossil_set_definition_identifier: set.id,
                    },
                    FossilDiscoveryRandomStream(DeterministicRng::from_entity(
                        *seed,
                        id,
                        RngDomain::Fossil,
                    )),
                    Transform::from_translation(position)
                        .with_scale(Vec3::splat(marker.prefab_scale)),
                    Visibility::Hidden,
                    PrefabPresentation::new(presentation.clone()),
                ));
            }
        }
        if let Some(mut progress) = progress {
            progress.set_if_neq(FossilDiscoveryLevel(next));
        } else {
            commands.entity(root).insert(FossilDiscoveryLevel(next));
        }
    }
}
