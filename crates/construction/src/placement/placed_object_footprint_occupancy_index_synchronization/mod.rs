//! Synchronizes authoritative placed-object footprints with the occupancy index.

use bevy::{platform::collections::HashSet, prelude::*};
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_world_state::world_spawn::prefab_source_asset_handle::PrefabSourceAssetHandle;
use openzt2_zoo_state::placement::object_placement_geometry::{
    ScenePlacementFootprintChanged, ScenePlacementFootprints,
};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::{WorldDefinitions, WorldDefinitionsView};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_zoo_state::placement::object_placement_definition_queries::authored_object_entrance_definitions;
use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_origin_and_quarter_turns;
use openzt2_zoo_state::placement::object_placement_definition_queries::project_authored_object_entrance_definition_to_component;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::calculate_object_placement_footprint_origin;
use openzt2_zoo_state::placement::placed_object_types::IndexedObjectPlacementCells;
use openzt2_zoo_state::placement::placed_object_types::PhysicsMovedPlacedObjectFootprint;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectFootprintOccupancy;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;

/// Projects loaded placement facts onto objects created by world hydration or
/// save restoration. The indexed cells track the contribution of each entity.
/// Docking points are ECS facts: the first remains on the placed entity for
/// the persistence contract and additional points are derived child entities.
pub(super) fn hydrate_placed_object_placement_components(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    footprints: Res<ScenePlacementFootprints>,
    objects: Query<
        (
            Entity,
            &DefinitionId,
            &Transform,
            &WorldMember,
            Option<&PrefabSourceAssetHandle>,
        ),
        (
            With<DefinitionId>,
            With<PersistentId>,
            Without<PlacedObjectDefinitionReference>,
        ),
    >,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, object, transform, member, prefab) in &objects {
        let Some(definition) = resolve_object_placeable_definition(catalogue, object.0) else {
            continue;
        };
        let geometry = footprints.resolve(definition, prefab.map(|prefab| &prefab.0));
        let Some((origin, eighth_turns)) = calculate_authored_object_placement_eighth_turns(
            definition, transform,
        )
        .and_then(|turns| {
            calculate_object_placement_footprint_origin(geometry, transform, turns)
                .map(|origin| (origin, turns))
        }) else {
            continue;
        };
        let authored_entrances = authored_object_entrance_definitions(definition);
        {
            let mut entity = commands.entity(entity);
            entity.insert((
                PlacedObjectDefinitionReference {
                    definition: object.0,
                },
                PlacedObjectFootprintOccupancy {
                    origin,
                    eighth_turns,
                },
            ));
            if let Some(entrance) = authored_entrances.first() {
                entity.insert(project_authored_object_entrance_definition_to_component(
                    entrance,
                ));
            }
            if definition.moving_footprint {
                entity.insert(PhysicsMovedPlacedObjectFootprint);
            }
        }
        for entrance in authored_entrances.iter().skip(1) {
            commands.spawn((
                project_authored_object_entrance_definition_to_component(entrance),
                *member,
                ChildOf(entity),
            ));
        }
    }
}

type PlacedFootprintSources = (
    Entity,
    &'static PersistentId,
    Ref<'static, PlacedObjectDefinitionReference>,
    &'static Transform,
    &'static mut PlacedObjectFootprintOccupancy,
    &'static mut IndexedObjectPlacementCells,
    Option<Ref<'static, PrefabSourceAssetHandle>>,
);

type ChangedPlacedFootprintSource = Or<(
    Changed<PlacedObjectDefinitionReference>,
    Changed<PlacedObjectFootprintOccupancy>,
    Changed<PrefabSourceAssetHandle>,
)>;

pub(super) fn synchronize_placed_footprints(
    mut index: ResMut<PlacedObjectFootprintOccupancyIndex>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    footprints: Res<ScenePlacementFootprints>,
    mut scene_changes: MessageReader<ScenePlacementFootprintChanged>,
    mut removed_prefabs: RemovedComponents<PrefabSourceAssetHandle>,
    mut placed: ParamSet<(
        Query<PlacedFootprintSources>,
        Query<PlacedFootprintSources, ChangedPlacedFootprintSource>,
    )>,
    mut scratch: Local<Vec<IVec2>>,
) {
    let changed_scenes = scene_changes
        .read()
        .map(|change| change.0)
        .collect::<HashSet<_>>();
    let removed_prefabs = removed_prefabs.read().collect::<HashSet<_>>();
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let definitions_changed = active_definitions.is_changed();
    if definitions_changed || !changed_scenes.is_empty() || !removed_prefabs.is_empty() {
        let mut all = placed.p0();
        refresh_changed_placed_footprints(
            all.iter_mut()
                .filter(|(entity, _, placeable, _, occupancy, _, prefab)| {
                    definitions_changed
                        || placeable.is_changed()
                        || occupancy.is_changed()
                        || removed_prefabs.contains(entity)
                        || prefab.as_ref().is_some_and(|source| {
                            source.is_changed() || changed_scenes.contains(&source.0.id())
                        })
                }),
            catalogue,
            &footprints,
            &mut index,
            &mut scratch,
        );
    } else {
        refresh_changed_placed_footprints(
            placed.p1().iter_mut(),
            catalogue,
            &footprints,
            &mut index,
            &mut scratch,
        );
    }
}

fn refresh_changed_placed_footprints<'w, 's>(
    changed: impl Iterator<Item = bevy::ecs::query::QueryItem<'w, 's, PlacedFootprintSources>>,
    catalogue: WorldDefinitionsView<'_>,
    footprints: &ScenePlacementFootprints,
    index: &mut ResMut<PlacedObjectFootprintOccupancyIndex>,
    scratch: &mut Vec<IVec2>,
) {
    let mut changed = changed.collect::<Vec<_>>();
    changed.sort_unstable_by_key(|(_, id, ..)| id.0);
    for (entity, _, placeable, transform, mut occupancy, mut cells, prefab) in changed {
        refresh_occupied_cells(
            entity,
            placeable.definition,
            transform,
            prefab.as_deref().map(|prefab| &prefab.0),
            catalogue,
            footprints,
            &mut occupancy,
            &mut cells,
            index,
            scratch,
        );
    }
}

pub(super) fn synchronize_physics_moved_object_footprints_with_occupancy_index(
    mut index: ResMut<PlacedObjectFootprintOccupancyIndex>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    footprints: Res<ScenePlacementFootprints>,
    mut moving: Query<
        PlacedFootprintSources,
        (With<PhysicsMovedPlacedObjectFootprint>, Changed<Transform>),
    >,
    mut scratch: Local<Vec<IVec2>>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, _, placeable, transform, mut occupancy, mut cells, prefab) in &mut moving {
        refresh_occupied_cells(
            entity,
            placeable.definition,
            transform,
            prefab.as_deref().map(|prefab| &prefab.0),
            catalogue,
            &footprints,
            &mut occupancy,
            &mut cells,
            &mut index,
            &mut scratch,
        );
    }
}

fn refresh_occupied_cells(
    entity: Entity,
    definition: openzt2_game_data::AssetId,
    transform: &Transform,
    prefab: Option<&Handle<ScenePrefabAsset>>,
    catalogue: WorldDefinitionsView<'_>,
    footprints: &ScenePlacementFootprints,
    occupancy: &mut Mut<PlacedObjectFootprintOccupancy>,
    indexed: &mut Mut<IndexedObjectPlacementCells>,
    index: &mut ResMut<PlacedObjectFootprintOccupancyIndex>,
    scratch: &mut Vec<IVec2>,
) {
    let next = resolve_object_placeable_definition(catalogue, definition).and_then(|definition| {
        let eighth_turns = calculate_authored_object_placement_eighth_turns(definition, transform)?;
        let geometry = footprints.resolve(definition, prefab);
        let origin =
            calculate_object_placement_footprint_origin(geometry, transform, eighth_turns)?;
        Some((origin, eighth_turns, geometry))
    });
    let next_cells: &[IVec2] = if let Some((origin, eighth_turns, geometry)) = next {
        occupancy.set_if_neq(PlacedObjectFootprintOccupancy {
            origin,
            eighth_turns,
        });
        collect_occupied_placement_cells_for_origin_and_quarter_turns(
            geometry.cells(eighth_turns),
            origin,
            eighth_turns / 2,
            scratch,
        )
        .unwrap_or_default()
    } else {
        &[]
    };
    if indexed.0.as_ref() == next_cells {
        return;
    }
    for cell in indexed.0.iter().copied() {
        index.remove_entity_from_cell(cell, entity);
    }
    for cell in next_cells.iter().copied() {
        index.insert_entity_into_cell(cell, entity);
    }
    indexed.0 = next_cells.into();
}

pub(super) fn remove_indexed_placement_footprint(
    removed: On<Remove<IndexedObjectPlacementCells>>,
    placed: Query<&IndexedObjectPlacementCells>,
    mut index: ResMut<PlacedObjectFootprintOccupancyIndex>,
) {
    if let Ok(indexed) = placed.get(removed.entity) {
        for cell in indexed.0.iter().copied() {
            index.remove_entity_from_cell(cell, removed.entity);
        }
    }
}
