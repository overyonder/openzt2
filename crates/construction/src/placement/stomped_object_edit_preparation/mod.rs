use std::collections::BTreeSet;

use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::world_definitions::object_placement::{
    PlaceableDefinition, placement_stomp_rules::PlacementStompAction,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId, prefab_source_asset_handle::PrefabSourceAssetHandle,
};
use openzt2_zoo_state::placement::{
    object_placement_validation::object_placement_stomp_action,
    placed_object_types::{IndexedObjectPlacementCells, PlacedObjectDefinitionReference},
    placement_occupancy_index::PlacedObjectFootprintQueries,
    placement_transaction_types::{PreparedObjectPlacementEditMutationKind, PreparedObjectPlacementMutation},
};

pub(crate) type StompablePlacedObject = (
    &'static PersistentId,
    &'static PlacedObjectDefinitionReference,
    &'static Transform,
    &'static IndexedObjectPlacementCells,
    &'static PrefabSourceAssetHandle,
);

pub(super) fn object_stomp_prevents_placement_at_cell(
    cell: IVec2,
    definition: &PlaceableDefinition,
    catalogue: WorldDefinitionsView<'_>,
    root: Entity,
    placement: &PlacedObjectFootprintQueries,
    targets: &Query<StompablePlacedObject>,
    ignored: impl Fn(Entity) -> bool,
) -> bool {
    placement.entities_occupying_cell(root, cell).any(|entity| {
        if ignored(entity) {
            return false;
        }
        let Ok((_, target, _, cells, _)) = targets.get(entity) else {
            return true;
        };
        match object_placement_stomp_action(definition, catalogue, Some(target.definition)) {
            PlacementStompAction::Allow => false,
            PlacementStompAction::Prevent => true,
            PlacementStompAction::Delete => !cells.0.contains(&cell),
        }
    })
}

pub(super) fn prepare_stomped_object_removals(
    cells: &[IVec2],
    definition: &PlaceableDefinition,
    catalogue: WorldDefinitionsView<'_>,
    root: Entity,
    placement: &PlacedObjectFootprintQueries,
    targets: &Query<StompablePlacedObject>,
    ignored: impl Fn(Entity) -> bool,
) -> Option<Vec<PreparedObjectPlacementMutation>> {
    prepare_placement_stomp_removals(
        cells.iter().copied(),
        root,
        placement,
        targets,
        ignored,
        |target| object_placement_stomp_action(definition, catalogue, Some(target)),
    )
}

pub(crate) fn prepare_placement_stomp_removals(
    cells: impl Iterator<Item = IVec2>,
    root: Entity,
    placement: &PlacedObjectFootprintQueries,
    targets: &Query<StompablePlacedObject>,
    ignored: impl Fn(Entity) -> bool,
    action: impl Fn(AssetId) -> PlacementStompAction,
) -> Option<Vec<PreparedObjectPlacementMutation>> {
    let overlapping = cells
        .flat_map(|cell| placement.entities_occupying_cell(root, cell))
        .filter(|entity| !ignored(*entity))
        .collect::<BTreeSet<_>>();
    let mut removals = Vec::new();
    for entity in overlapping {
        let Ok((id, target, transform, cells, prefab)) = targets.get(entity) else {
            return None;
        };
        match action(target.definition) {
            PlacementStompAction::Allow => continue,
            PlacementStompAction::Prevent => return None,
            PlacementStompAction::Delete => {}
        }
        removals.push(PreparedObjectPlacementMutation {
            entity: *id,
            definition: target.definition,
            transform: *transform,
            cells: cells.0.clone(),
            previous_transform: None,
            previous_cells: Box::new([]),
            mutation: PreparedObjectPlacementEditMutationKind::Remove,
            applied_entity: Some(entity),
            preview: None,
            prefab: prefab.0.clone(),
        });
    }
    removals.sort_unstable_by_key(|removal| removal.entity.0);
    Some(removals)
}
