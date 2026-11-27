use bevy::{ecs::relationship::RelationshipTarget, platform::collections::HashSet, prelude::*};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::world_definitions::extinct_animal_recovery::FossilSetDefinition;
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
        fossil_recovery_types::{FossilDiscoveryLevel, FossilDiscoveryRandomStream},
    },
    locomotion::locomotion_types::{ActiveTerrainDerivedNavigationGraph, NavFlags},
    placement::placement_occupancy_index::PlacedObjectFootprintQueries,
    progression::fame_types::Fame,
    terrain::{
        dry_surface_eligibility::DrySurfaceEligibility,
        terrain_chunk_types::{TerrainChunk, TerrainIndex},
    },
    topology::topology_graph_types::{PathTile, TopologyGrid},
};

use super::fossil_definition_index::FossilDefinitionIndex;
use openzt2_zoo_state::extinct_animals::fossil_recovery_types::{
    DigPit, FossilSiteKind, RequestedFossilPuzzle, SetFossilPuzzleToFind,
};

pub(super) fn reset_requested_fossil_search(
    mut requests: MessageReader<SetFossilPuzzleToFind>,
    mut worlds: Query<(Option<&WorldMembers>, Option<&mut RequestedFossilPuzzle>), With<WorldRoot>>,
    retired: Query<
        (),
        Or<(
            With<FossilSiteKind>,
            With<DigPit>,
            With<CollectedFossilPiece>,
        )>,
    >,
    assemblies: Query<(), With<openzt2_zoo_state::extinct_animals::fossil_collection_and_assembly_types::FossilSetAssembly>>,
    mut commands: Commands,
) {
    use openzt2_zoo_state::extinct_animals::fossil_collection_and_assembly_types::{
        FossilAssemblyCompleted, FossilAssemblySlots, FossilSetAssembly,
    };

    let mut latest =
        bevy::ecs::entity::EntityHashMap::<Option<openzt2_game_data::AssetId>>::default();
    latest.extend(
        requests
            .read()
            .map(|request| (request.root, request.puzzle)),
    );
    for (root, puzzle) in latest {
        let Ok((members, current)) = worlds.get_mut(root) else {
            continue;
        };
        for member in members.into_iter().flat_map(RelationshipTarget::iter) {
            if retired.contains(member) {
                commands.entity(member).try_despawn();
            } else if assemblies.contains(member) {
                commands
                    .entity(member)
                    .despawn_related::<FossilAssemblySlots>()
                    .remove::<(FossilSetAssembly, FossilAssemblyCompleted)>();
            }
        }
        let mut zoo = commands.entity(root);
        zoo.remove::<(FossilDiscoveryLevel, FossilTierBlockedNotice)>();
        match (puzzle, current) {
            (Some(puzzle), Some(mut current)) => {
                current.set_if_neq(RequestedFossilPuzzle(puzzle));
            }
            (Some(puzzle), None) => {
                zoo.insert(RequestedFossilPuzzle(puzzle));
            }
            (None, Some(_)) => {
                zoo.remove::<RequestedFossilPuzzle>();
            }
            (None, None) => {}
        }
    }
}

pub(super) fn bind_live_fossil_assembly_tables(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut entities: ParamSet<(
        Query<(Entity, &DefinitionId, Option<&mut FossilAssemblyTable>), With<WorldMember>>,
        Query<
            (Entity, &DefinitionId, Option<&mut FossilAssemblyTable>),
            (
                With<WorldMember>,
                Or<(Changed<DefinitionId>, Added<WorldMember>)>,
            ),
        >,
    )>,
) {
    let changed = definitions.is_changed()
        || events.read().any(|event| {
            matches!(
                event,
                AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
            )
        });
    events.clear();
    let Some(view) = definitions.get(&assets) else {
        return;
    };
    let mut bind =
        |(entity, definition, table): (Entity, &DefinitionId, Option<Mut<FossilAssemblyTable>>)| {
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
        };
    if changed {
        entities.p0().iter_mut().for_each(&mut bind);
    } else {
        entities.p1().iter_mut().for_each(bind);
    }
}

type RecoveryWorld = (
    Entity,
    &'static WorldRoot,
    (
        &'static ZooSeed,
        &'static TerrainIndex,
        &'static TopologyGrid,
        &'static ActiveTerrainDerivedNavigationGraph,
    ),
    Option<&'static WorldMembers>,
    Option<Ref<'static, WorldFossilPieces>>,
    Option<Ref<'static, Fame>>,
    Option<&'static mut FossilDiscoveryLevel>,
    Option<Ref<'static, RequestedFossilPuzzle>>,
    Has<FossilTierBlockedNotice>,
    &'static mut PersistentIdAllocator,
);

#[derive(Component)]
pub(super) struct FossilTierBlockedNotice;

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_world_fossil_recovery_sites(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    catalogue: Res<FossilDefinitionIndex>,
    mut worlds: Query<RecoveryWorld, With<WorldLoadCompleted>>,
    chunks: Query<(&WorldMember, &TerrainChunk)>,
    ground: DrySurfaceEligibility,
    occupied: PlacedObjectFootprintQueries,
    paths: Query<(), With<PathTile>>,
    existing: Query<&Transform, Or<(With<FossilSiteKind>, With<DigPit>)>>,
    discovered: Query<&CollectedFossilPiece>,
) {
    let Some(view) = definitions.get(&assets) else {
        return;
    };
    if view.fossil_placement().is_none() || catalogue.sets.is_empty() {
        return;
    }
    let Some(marker) = view.find_object(AssetId::from_key("fossilsite")) else {
        return;
    };
    let Some(presentation) = view.scene(marker.prefab) else {
        return;
    };
    for (
        root,
        world,
        (seed, terrain_index, grid, navigation),
        members,
        pieces,
        fame,
        progress,
        requested,
        notified_locked,
        mut ids,
    ) in &mut worlds
    {
        let projection_changed = catalogue.is_changed()
            || fame.as_ref().is_some_and(|fame| fame.is_changed())
            || pieces.as_ref().is_some_and(|pieces| pieces.is_changed())
            || requested
                .as_ref()
                .is_some_and(|request| request.is_changed());
        if progress.is_some() && !projection_changed {
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
        let previous = progress.as_deref().map_or(0, |level| level.0);
        let complete =
            |set: &FossilSetDefinition| set.pieces.iter().all(|piece| found.contains(piece));
        let sets = catalogue
            .sets
            .iter()
            .filter_map(|&id| view.find_fossil_set(id));
        let (next, puzzle_sites, bonus_sites) = if let Some(requested) = requested {
            let Some(set) = view
                .find_fossil_set(requested.0)
                .filter(|set| !complete(set))
            else {
                continue;
            };
            if progress.is_some() && !requested.is_changed() {
                continue;
            }
            (set.discovery_level, site_count(set), 0)
        } else {
            if !sets
                .clone()
                .filter(|set| set.discovery_level == previous)
                .all(complete)
            {
                continue;
            }
            let next = previous.saturating_add(1);
            if fame.as_deref().is_some_and(|fame| {
                let stars = fame.half_stars;
                next > (if stars == 8 { 9 } else { stars }).div_ceil(2)
            }) {
                if next < 6 && !notified_locked {
                    commands.entity(root).insert(FossilTierBlockedNotice);
                    commands.trigger(super::fossil_discovery::FossilFindFeedback::tier(
                        root, false,
                    ));
                }
                continue;
            }
            let puzzle_sites = sets
                .filter(|set| set.discovery_level == next && !complete(set))
                .map(site_count)
                .sum::<usize>();
            (next, puzzle_sites, 2)
        };
        if puzzle_sites == 0 {
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
        let mut cells = existing
            .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
            .flatten()
            .map(|pose| grid.position_cell(pose.translation).truncate())
            .collect::<HashSet<_>>();
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
        let graph = navigation.terrain_derived_navigation_graph();
        let terrain_spacing_m = f32::from(graph.terrain_cell_size_centimetres) / 100.0;
        let kinds = std::iter::repeat_n(FossilSiteKind::Puzzle, puzzle_sites)
            .chain(std::iter::repeat_n(
                FossilSiteKind::PlaceableObject,
                bonus_sites,
            ))
            .chain(std::iter::repeat_n(FossilSiteKind::Cash, bonus_sites));
        for kind in kinds {
            let position = (0..1000).find_map(|_| {
                let x = random.range_u32(size.x)?;
                let z = random.range_u32(size.y)?;
                let location =
                    minimum + (Vec2::new(x as f32, z as f32) + Vec2::splat(0.5)) * grid.spacing_m;
                let height = ground.ground_height(root, location)?;
                let position = Vec3::new(location.x, height, location.y);
                let cell = grid.position_cell(position).truncate();
                if cells.contains(&cell)
                    || occupied
                        .entities_occupying_cell(root, cell)
                        .any(|entity| !paths.contains(entity))
                {
                    return None;
                }
                let terrain_cell = ((location - terrain_index.origin) / terrain_spacing_m)
                    .floor()
                    .as_uvec2();
                let node = graph
                    .calculate_navigation_node_for_terrain_cell(terrain_cell.to_array())
                    .and_then(|index| graph.find_navigation_node(index))?;
                if node.eligibility_flags.0 & (NavFlags::GUEST.0 | NavFlags::STAFF.0) == 0 {
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
                kind,
                FossilDiscoveryRandomStream(DeterministicRng::from_entity(
                    *seed,
                    id,
                    RngDomain::Fossil,
                )),
                Transform::from_translation(position).with_scale(Vec3::splat(marker.prefab_scale)),
                Visibility::Hidden,
                PrefabPresentation::new(presentation.clone()),
            ));
        }
        if let Some(mut progress) = progress {
            progress.set_if_neq(FossilDiscoveryLevel(next));
        } else {
            commands.entity(root).insert(FossilDiscoveryLevel(next));
        }
        if bonus_sites != 0 {
            commands.entity(root).remove::<FossilTierBlockedNotice>();
            if next > 1 {
                commands.trigger(super::fossil_discovery::FossilFindFeedback::tier(
                    root, true,
                ));
            }
        }
    }
}

fn site_count(set: &FossilSetDefinition) -> usize {
    set.pieces
        .len()
        .div_ceil(usize::from(set.pieces_per_location.max(1)))
}
