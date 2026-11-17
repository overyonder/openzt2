//! A building a rampaging animal smashes is removed, and its authored ruin
//! (`entityRuinBinderName`) is put down where it stood.

use bevy::asset::LoadState;
use bevy::gltf::Gltf;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_data::AssetId;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world_state::world_spawn::persistent_id_types::{PersistentId, PersistentIdAllocator};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::placement::object_placement_definition_queries::{
    collect_occupied_placement_cells_for_transform, resolve_object_placeable_definition,
};
use openzt2_zoo_state::placement::placed_object_types::{
    PlacedObjectDefinitionReference, PlacedObjectFootprintOccupancy,
};
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementCellCollectionScratch;
use openzt2_zoo_state::placement::placement_transaction_types::{
    DestroyPlacedObject, ObjectPlacementCommitted, PreparedObjectPlacementEditMutationKind,
    PreparedObjectPlacementMutation,
};

use crate::placement::placed_object_creation_edit_application::apply_prepared_placed_object_creation_edit;

type PlacedObjects<'w, 's> = Query<
    'w,
    's,
    (
        &'static PersistentId,
        &'static PlacedObjectDefinitionReference,
        &'static mut Transform,
        &'static mut PlacedObjectFootprintOccupancy,
    ),
>;

// ponytail: the explosion effect, sound and flying rubble aren't played; the
// building just gives way to its ruin.
#[allow(clippy::too_many_arguments)]
pub(super) fn ruin_destroyed_placed_objects(
    mut commands: Commands,
    mut requests: MessageReader<DestroyPlacedObject>,
    // Smashed objects whose ruin prefab is still loading.
    mut pending: Local<Vec<(Entity, Option<(AssetId, Handle<ScenePrefabAsset>)>)>>,
    mut world_assets: ResMut<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    server: Res<AssetServer>,
    (prefabs, models): (Res<Assets<ScenePrefabAsset>>, Res<Assets<Gltf>>),
    members: Query<&WorldMember>,
    (mut placement, mut scratch, mut allocator): (
        ResMut<PlacedObjectFootprintOccupancyIndex>,
        ResMut<ObjectPlacementCellCollectionScratch>,
        ResMut<PersistentIdAllocator>,
    ),
    placed: PlacedObjects,
    mut committed: MessageWriter<ObjectPlacementCommitted>,
) {
    pending.extend(requests.read().map(|request| (request.entity, None)));
    // A ruin's footprint comes from its model's bounds once that has loaded;
    // nothing else hydrates it before the ruin is put down.
    for (ruin, handle) in pending.iter().filter_map(|(_, prefab)| prefab.as_ref()) {
        if let Some(bounds) = prefabs.get(handle).and_then(|prefab| {
            prefab
                .canonical_scene_prefab_document()
                .automatic_placement_bounds_xz
        }) {
            world_definitions.apply_source_lowered_automatic_placement_bounds(
                &mut world_assets,
                *ruin,
                bounds,
            );
        }
    }
    let Some(catalogue) = world_definitions.get(&world_assets) else {
        return;
    };
    let created = HashMap::new();
    pending.retain_mut(|(building, prefab)| {
        let Ok((_, reference, transform, _)) = placed.get(*building) else {
            return false;
        };
        let (definition, transform) = (reference.definition, *transform);
        let Ok(root) = members.get(*building).map(|member| member.root) else {
            return false;
        };
        let ruin = catalogue
            .find_object(definition)
            .and_then(|object| object.destruction)
            .and_then(|destruction| {
                // An abstract ruin (`rubble_xt`) stands for its concrete kinds
                // (`rubble_xt_1`, `rubble_xt_2`), one picked per building.
                let placeable = |ruin: &AssetId| {
                    resolve_object_placeable_definition(catalogue, *ruin).is_some()
                };
                let ruin = if placeable(&destruction.ruin) {
                    destruction.ruin
                } else {
                    let mut kinds = catalogue
                        .objects()
                        .filter(|object| {
                            object.behavior_type_identifiers.contains(&destruction.ruin)
                        })
                        .map(|object| object.id)
                        .filter(placeable)
                        .collect::<Vec<_>>();
                    kinds.sort_unstable();
                    // ponytail: picked by the building's entity, not a seeded
                    // random stream; it stays the same while the ruin loads.
                    *kinds.get(building.to_bits() as usize % kinds.len().max(1))?
                };
                Some((ruin, catalogue.find_object(ruin)?.prefab))
            });
        if let (Some((ruin, scene)), None) = (ruin, prefab.as_ref()) {
            *prefab = catalogue.scene(scene).map(|handle| (ruin, handle));
        }
        // Wait for the ruin's prefab, and for its footprint from the prefab's
        // bounds; if either fails, the building still falls.
        if ruin.is_some()
            && prefab
                .as_ref()
                .is_some_and(|(ruin, handle)| match prefabs.get(handle) {
                    None => !matches!(
                        server.get_load_state(handle.id()),
                        Some(LoadState::Failed(_))
                    ),
                    Some(loaded) => {
                        loaded
                            .canonical_scene_prefab_document()
                            .automatic_placement_bounds_xz
                            .is_some()
                            && resolve_object_placeable_definition(catalogue, *ruin)
                                .is_some_and(|placeable| placeable.footprint.is_empty())
                    }
                })
        {
            return true;
        }
        let ruin = ruin
            .zip(prefab.clone())
            .and_then(|((ruin, _), (_, handle))| {
                prefabs.contains(handle.id()).then_some((ruin, handle))
            });
        let Some(placeable) = resolve_object_placeable_definition(catalogue, definition) else {
            return false;
        };
        if let Some(cells) = collect_occupied_placement_cells_for_transform(
            placeable,
            &transform,
            &mut scratch.cells,
        ) {
            for cell in cells.to_vec() {
                placement.remove_entity_from_cell(cell, *building);
            }
        }
        commands.entity(*building).despawn();
        debug!(?building, ?definition, ruin = ?ruin.as_ref().map(|(ruin, _)| *ruin),
            "placed object destroyed");
        let Some((ruin, handle)) = ruin else {
            return false;
        };
        let Some(ruin_placeable) = resolve_object_placeable_definition(catalogue, ruin) else {
            return false;
        };
        let Some(cells) = collect_occupied_placement_cells_for_transform(
            ruin_placeable,
            &transform,
            &mut scratch.cells,
        )
        .map(|cells| cells.to_vec().into_boxed_slice()) else {
            warn!(
                ?building,
                ?ruin,
                "ruin of a destroyed object has no footprint"
            );
            return false;
        };
        let Ok(id) = allocator.allocate(root) else {
            warn!(
                ?building,
                ?ruin,
                "ruin of a destroyed object has no persistent id"
            );
            return false;
        };
        let mut edit = PreparedObjectPlacementMutation {
            entity: id,
            definition: ruin,
            transform,
            cells,
            previous_transform: None,
            previous_cells: Box::new([]),
            mutation: PreparedObjectPlacementEditMutationKind::Create,
            applied_entity: None,
            preview: None,
            prefab: handle,
        };
        if !apply_prepared_placed_object_creation_edit(
            &mut commands,
            *building,
            &mut edit,
            catalogue,
            Some(root),
            &mut placement,
            &mut scratch,
            &prefabs,
            &models,
            &placed,
            &created,
            false,
            &mut ConstructionTool::Inspect,
            &mut committed,
        ) {
            warn!(
                ?building,
                ?ruin,
                "ruin of a destroyed object could not be put down"
            );
        }
        false
    });
}
